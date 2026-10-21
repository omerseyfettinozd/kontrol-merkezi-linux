//! Single reader of the hardware (safety invariant 6 / plan "tek okuyucu"). Runs on a
//! dedicated thread because EC reads block for ~57 ms each.
//!
//! Schedule (EC budget ≤ 6 ops/s for r9td):
//!  * every 1 s: tuxedo_io EF12/EF13 (temps) = 2 ops/s; EF10/EF11 (duty) every 2 s
//!    only while the EC is in manual (r9td or legacy driving the fans); in auto the
//!    duty comes from the `status` read (10 s; 30 s in shadow mode).
//!  * every 5 s: EF14 mode byte
//!  * every 10 s in auto (3 s when r9td owns the fans): r9t_fan `status` (~11 ops)
//!  * shadow mode while the legacy service has the EC in manual: no `status`; RPM is
//!    taken from the legacy service's own `status` reply every 10 s.
//!
//! Shadow mode halves the EC load (fast path every 2 s, `status` every 30 s): the EC
//! channel is a single mutex and the legacy service's 1.3 s `status` (≈22 EC ops)
//! otherwise grew 17.5 % with r9td running (Faz 2 acceptance c allows 10 %).
//!
//! CPU budget (measured, `docs/11`): one tuxedo_io read costs ~0.75 ms of kernel
//! time, so non-thermal sources are thinned: `acpitz` is never read (ACPI `_TMP`
//! goes through the EC), NVMe temps every 10 s, other slow sensors every 5 s,
//! NVML clocks/memory every 5 s.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use r9t_hal::cpufreq::Cpufreq;
use r9t_hal::fan::{FanBridge, SysfsFan};
use r9t_hal::gpu::{Gpu, Nvml};
use r9t_hal::hwmon::{Hwmon, HwmonTemp};
use r9t_hal::power::Power;
use r9t_hal::rapl::PackagePower;
use r9t_hal::rgb::Rgb;
use r9t_hal::tuxedo::{RealDevice, Unverified, Verified};
use r9t_hal::{HalError, Sysfs};
use r9t_protocol::snapshot::{FanMode, HistoryPoint};

use crate::history::History;
use crate::state::{Live, Timed};

pub fn unix_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Counters exported in `capabilities`/metrics and used by the Faz 2 acceptance.
#[derive(Debug, Default)]
pub struct Metrics {
    pub ec_ops: AtomicU64,
    pub fan_status_reads: AtomicU64,
    pub legacy_queries: AtomicU64,
    pub samples: AtomicU64,
    pub nvml_calls: AtomicU64,
    pub nvml_skipped_inactive: AtomicU64,
}

pub struct Shared {
    pub live: RwLock<Live>,
    pub history: Mutex<History>,
    pub metrics: Metrics,
    pub stop: AtomicBool,
    /// Set by the control path (Faz 3) to sample `status` every 3 s.
    pub owns_fans: AtomicBool,
}

pub struct Config {
    pub shadow: bool,
    pub legacy_socket: PathBuf,
    /// NVML adds ~24 MB RSS (libcuda) and is never released; `--no-nvml` skips it.
    pub nvml: bool,
}

struct Ec {
    dev: Option<Verified<RealDevice>>,
    last_attempt: Option<Instant>,
}

impl Ec {
    /// Opens and verifies tuxedo_io (hwcheck first); retried at most every 30 s.
    fn get(&mut self, fs: &Sysfs, now: Instant) -> Option<&mut Verified<RealDevice>> {
        if self.dev.is_none() && self.last_attempt.is_none_or(|t| now.duration_since(t) >= Duration::from_secs(30)) {
            self.last_attempt = Some(now);
            match RealDevice::open(fs, false).and_then(|d| Unverified::new(d).verify()) {
                Ok(v) => self.dev = Some(v),
                Err(e) => eprintln!("r9td: tuxedo_io kullanılamıyor: {e}"),
            }
        }
        self.dev.as_mut()
    }
}

pub fn run(shared: Arc<Shared>, cfg: Config, fs: Sysfs, notify: impl Fn() + Send + 'static) {
    let fan = SysfsFan::new(fs.clone());
    let gpu = Nvml::new(fs.clone()).with_preflight(Box::new(crate::nvml_preflight));
    let hwmon = Hwmon::new(fs.clone());
    let cpu = Cpufreq::new(fs.clone());
    let power = Power::new(fs.clone());
    let mut rapl = PackagePower::new(fs.clone());
    let rgb = Rgb::discover(fs.clone()).ok();
    let ppd = zbus::blocking::Connection::system().ok();
    let mut ec = Ec { dev: None, last_attempt: None };
    let discover = |h: &Hwmon| -> Vec<HwmonTemp> { h.discover().into_iter().filter(|t| t.chip != "acpitz").collect() };
    let mut temps: Vec<HwmonTemp> = discover(&hwmon);
    let mut sensor_values: Vec<Timed<f32>> = vec![Timed::default(); temps.len()];
    let apu_path = hwmon.apu_ppt_path();
    let mut tick: u64 = 0;
    let mut last_status: Option<Instant> = None;
    let mut ec_manual = false;
    let legacy_busy = Arc::new(AtomicBool::new(false));
    let mut last_legacy: Option<Instant> = None;
    let start = Instant::now();

    while !shared.stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        let m = &shared.metrics;

        // --- EC fast path ---
        let owns = shared.owns_fans.load(Ordering::Relaxed);
        let ec_every: u64 = if cfg.shadow && !owns { 2 } else { 1 };
        let mut fast = None;
        if tick % ec_every != 0 {
            // skip this tick (shadow mode thinning)
        } else if let Some(dev) = ec.get(&fs, now) {
            let temps_r = dev.temps();
            m.ec_ops.fetch_add(2, Ordering::Relaxed);
            // Duty only while the EC is manual. In shadow mode it comes from the 30 s
            // status: reading it every 2 s / 6 s slowed the legacy status by 15.5 % /
            // 10.8 % (limit 10 %), see docs/11.
            let duty_r = if tick % 2 == 0 && (ec_manual || owns) {
                m.ec_ops.fetch_add(2, Ordering::Relaxed);
                Some(dev.duties())
            } else {
                None
            };
            let mode_r = if tick % (5 * ec_every) < ec_every {
                m.ec_ops.fetch_add(1, Ordering::Relaxed);
                Some(dev.mode())
            } else {
                None
            };
            fast = Some((temps_r, duty_r, mode_r));
        }

        // --- r9t_fan status (full path) ---
        let interval = match (owns, cfg.shadow) {
            (true, _) => Duration::from_secs(3),
            (false, true) => Duration::from_secs(30),
            (false, false) => Duration::from_secs(10),
        };
        let want_status = fan.loaded()
            && (!cfg.shadow || !ec_manual)
            && last_status.is_none_or(|t| now.duration_since(t) >= interval);
        let status = if want_status {
            last_status = Some(now);
            m.ec_ops.fetch_add(11, Ordering::Relaxed);
            m.fan_status_reads.fetch_add(1, Ordering::Relaxed);
            Some(fan.status())
        } else {
            None
        };

        // --- cheap sysfs sources ---
        let ccds: Option<Vec<Option<f32>>> = (tick % 5 == 0)
            .then(|| ["Tccd1", "Tccd2"].iter().map(|l| Hwmon::find(&temps, "k10temp", l).and_then(|t| hwmon.read_c(t))).collect());
        for (t, slot) in temps.iter().zip(sensor_values.iter_mut()) {
            let every = match t.chip.as_str() {
                "k10temp" if t.label == "Tctl" => 1,
                "nvme" => 10,
                _ => 5,
            };
            if tick % every == 0 || slot.at.is_none() {
                slot.set(hwmon.read_c(t), now);
            }
        }
        let pkg = rapl.sample(now, 5.0);
        let apu = apu_path.as_deref().and_then(|p| hwmon.read_w(p));
        let tctl = temps.iter().zip(&sensor_values).find(|(t, _)| t.chip == "k10temp" && t.label == "Tctl").and_then(|(_, v)| v.value);
        let freq = if tick % 2 == 0 { Some(cpu.current_mhz()) } else { None };
        // Limits/boost/EPP change only on writes (ours, legacy, nvidia-powerd): 10 s.
        let limits = if tick % 10 == 0 { Some(cpu.limits()) } else { None };
        let boost = if tick % 10 == 0 { Some(cpu.boost()) } else { None };
        let epp = if tick % 10 == 0 { Some(cpu.epp()) } else { None };
        let ac = power.ac_online();
        let (bat, usb) = if tick % 2 == 0 { (power.battery(), Some(power.usb_pd())) } else { (None, None) };
        let profile = if tick % 5 == 0 { ppd.as_ref().map(read_profile) } else { None };
        let brightness = if tick % 5 == 0 { rgb.as_ref().map(|r| r.brightness()) } else { None };
        let fn_lock = if tick % 30 == 0 { Some(fs.read("/sys/devices/platform/tuxedo_keyboard/fn_lock")) } else { None };
        // Plain USB attribute, no EC traffic: 5 s so a toggle shows up quickly.
        let camera = if tick % 5 == 0 { Some(fs.read(format!("{}/authorized", r9t_hal::CAMERA_USB))) } else { None };

        // --- GPU (gated) ---
        let runtime = gpu.runtime_status();
        let gpu_sample = match runtime.as_deref() {
            Ok("active") if cfg.nvml => {
                let before = gpu.nvml_calls();
                let r = if tick % 5 == 0 { gpu.sample().map(|s| (s, true)) } else { gpu.sample_fast().map(|s| (s, false)) };
                m.nvml_calls.fetch_add(gpu.nvml_calls() - before, Ordering::Relaxed);
                Some(r)
            }
            _ => {
                m.nvml_skipped_inactive.fetch_add(1, Ordering::Relaxed);
                None
            }
        };

        // --- publish ---
        {
            let mut live = shared.live.write().unwrap_or_else(|p| p.into_inner());
            if let Some((temps_r, duty_r, mode_r)) = &fast {
                match temps_r {
                    Ok((c, g)) => {
                        live.cpu_ec_c.set(c.map(f32::from), now);
                        live.gpu_ec_c.set(g.map(f32::from), now);
                    }
                    Err(_) => {
                        live.cpu_ec_c.set(None, now);
                        live.gpu_ec_c.set(None, now);
                    }
                }
                match duty_r {
                    Some(Ok((c, g))) => {
                        live.cpu_duty.set(c.map(|d| d / 2), now);
                        live.gpu_duty.set(g.map(|d| d / 2), now);
                    }
                    Some(Err(_)) => {
                        live.cpu_duty.set(None, now);
                        live.gpu_duty.set(None, now);
                    }
                    None => {}
                }
                if let Some(mr) = mode_r {
                    let byte = mr.as_ref().ok().copied().flatten();
                    live.mode_byte.set(byte, now);
                    if let Some(b) = byte {
                        ec_manual = b & 0x40 != 0;
                    }
                }
            }
            match status {
                Some(Ok(s)) => {
                    let mode = match s.mode {
                        0 => Some(FanMode::Auto),
                        1 => Some(FanMode::Manual),
                        2 => Some(FanMode::Boost),
                        _ => None,
                    };
                    live.fan_mode.set(mode, now);
                    live.cpu_rpm.set(u32::try_from(s.cpu_rpm).ok(), now);
                    live.gpu_rpm.set(u32::try_from(s.gpu_rpm).ok(), now);
                    live.lease_ms.set(u32::try_from(s.lease_ms).ok(), now);
                    live.kernel_error.set(Some(s.error), now);
                    live.thermal_override.set(Some(s.thermal), now);
                    live.mode_byte.set(u8::try_from(s.mode_byte).ok(), now);
                    ec_manual = s.mode_byte & 0x40 != 0;
                    if fast.as_ref().is_none_or(|f| f.1.is_none()) {
                        live.cpu_duty.set(Some((s.cpu_pwm / 2) as u8), now);
                        live.gpu_duty.set(Some((s.gpu_pwm / 2) as u8), now);
                    }
                    if fast.is_none() {
                        let v = s.valid_temps();
                        live.cpu_ec_c.set(v.map(|t| t.0 as f32), now);
                        live.gpu_ec_c.set(v.map(|t| t.1 as f32), now);
                        live.cpu_duty.set(Some((s.cpu_pwm / 2) as u8), now);
                        live.gpu_duty.set(Some((s.gpu_pwm / 2) as u8), now);
                    }
                }
                // -ERANGE from status means implausible EC values: "fan control unavailable".
                Some(Err(e)) if !matches!(e, HalError::Busy(_)) => {
                    live.cpu_rpm.set(None, now);
                    live.gpu_rpm.set(None, now);
                }
                Some(Err(_)) => {}
                None => {}
            }
            live.tctl_c.set(tctl, now);
            if let Some(c) = &ccds {
                live.ccd_c = c.iter().map(|v| Timed { value: *v, at: Some(now) }).collect();
            }
            live.sensors = temps
                .iter()
                .zip(&sensor_values)
                .map(|(t, v)| (t.id.clone(), t.chip.clone(), t.label.clone(), v.clone()))
                .collect();
            match pkg {
                Ok(w) => live.package_w.set(w, now),
                Err(_) => live.package_w.set(None, now),
            }
            live.apu_ppt_w.set(apu, now);
            if let Some(f) = freq {
                live.cpu_freq_mhz.set(f, now);
            }
            if let Some(l) = limits {
                live.cpu_max_mhz.set(l.ok().and_then(|l| l.max_mhz), now);
            }
            if let Some(b) = boost {
                live.boost.set(b.ok().flatten(), now);
            }
            if let Some(e) = epp {
                live.epp.set(e.ok().flatten(), now);
            }
            live.ac_online.set(ac, now);
            if let Some(b) = &bat {
                live.bat_percent.set(b.percent, now);
                live.bat_status.set(b.status.clone(), now);
                live.bat_w.set(b.power_w, now);
                live.bat_wh.set(b.energy_wh, now);
                live.bat_full_wh.set(b.full_wh, now);
                live.bat_design_wh.set(b.design_wh, now);
                live.bat_v.set(b.voltage_v, now);
            }
            if let Some(usb) = usb {
                live.usb_pd = usb
                    .into_iter()
                    .map(|u| {
                        (u.name, Timed { value: u.online, at: Some(now) }, Timed { value: u.voltage_v, at: Some(now) }, Timed { value: u.current_a, at: Some(now) })
                    })
                    .collect();
            }
            if let Some(p) = profile {
                live.profile.set(p, now);
            }
            if let Some(b) = brightness {
                live.rgb_brightness.set(b.ok().and_then(|v| u8::try_from(v).ok()), now);
            }
            if let Some(f) = fn_lock {
                live.fn_lock.set(f.ok().and_then(|v| match v.as_str() { "1" => Some(true), "0" => Some(false), _ => None }), now);
            }
            if let Some(c) = camera {
                live.camera.set(c.ok().and_then(|v| match v.as_str() { "1" => Some(true), "0" => Some(false), _ => None }), now);
            }
            live.gpu_runtime.set(runtime.ok(), now);
            match gpu_sample {
                Some(Ok((g, full))) => {
                    live.gpu_temp_c.set(g.temp_c.map(|t| t as f32), now);
                    live.gpu_power_w.set(g.power_w, now);
                    live.gpu_util.set(g.util_gpu_pct, now);
                    live.gpu_mem_util.set(g.util_memory_pct, now);
                    if full {
                        live.gpu_limit_w.set(g.enforced_limit_w, now);
                        live.gpu_clock_mhz.set(g.clock_graphics_mhz, now);
                        live.gpu_mem_clock_mhz.set(g.clock_memory_mhz, now);
                        live.gpu_max_clock_mhz.set(g.max_clock_mhz, now);
                        live.vram_used.set(g.vram_used_mib, now);
                        live.vram_total.set(g.vram_total_mib, now);
                        live.pstate.set(g.pstate, now);
                    }
                }
                Some(Err(_)) | None => {
                    live.gpu_temp_c.set(None, now);
                    live.gpu_power_w.set(None, now);
                }
            }
            live.sequence += 1;
            let point = HistoryPoint {
                t_ms: unix_ms(),
                cpu_c: live.cpu_ec_c.value,
                gpu_c: live.gpu_ec_c.value,
                cpu_w: live.package_w.value,
                gpu_w: live.gpu_power_w.value,
                cpu_rpm: live.cpu_rpm.fresh(now, 15_000),
                gpu_rpm: live.gpu_rpm.fresh(now, 15_000),
                battery_w: live.bat_w.value,
            };
            drop(live);
            shared.history.lock().unwrap_or_else(|p| p.into_inner()).push(point);
        }
        m.samples.fetch_add(1, Ordering::Relaxed);

        // Shadow + legacy manual: RPM from the legacy service, off this thread.
        if cfg.shadow && ec_manual && last_legacy.is_none_or(|t| now.duration_since(t) >= Duration::from_secs(10))
            && !legacy_busy.swap(true, Ordering::AcqRel)
        {
            last_legacy = Some(now);
            m.legacy_queries.fetch_add(1, Ordering::Relaxed);
            let shared2 = shared.clone();
            let busy = legacy_busy.clone();
            let socket = cfg.legacy_socket.clone();
            std::thread::spawn(move || {
                if let Ok(data) = crate::legacy::status(&socket, Duration::from_secs(5)) {
                    let fans = &data["fans"]["measured"];
                    let t = Instant::now();
                    let mut live = shared2.live.write().unwrap_or_else(|p| p.into_inner());
                    live.cpu_rpm.set(fans["cpu_rpm"].as_u64().and_then(|v| u32::try_from(v).ok()), t);
                    live.gpu_rpm.set(fans["gpu_rpm"].as_u64().and_then(|v| u32::try_from(v).ok()), t);
                    live.lease_ms.set(fans["lease_ms"].as_u64().and_then(|v| u32::try_from(v).ok()), t);
                    live.fan_mode.set(match fans["mode"].as_i64() { Some(0) => Some(FanMode::Auto), Some(1) => Some(FanMode::Manual), Some(2) => Some(FanMode::Boost), _ => None }, t);
                }
                busy.store(false, Ordering::Release);
            });
        }

        if tick % 60 == 59 {
            let fresh = discover(&hwmon);
            if fresh.iter().map(|t| &t.id).ne(temps.iter().map(|t| &t.id)) {
                sensor_values = vec![Timed::default(); fresh.len()];
                temps = fresh;
            }
        }
        notify();
        tick += 1;
        // Keep a 1 s cadence relative to start (no drift).
        let next = start + Duration::from_secs(tick);
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else {
            tick = start.elapsed().as_secs();
        }
    }
}

fn read_profile(conn: &zbus::blocking::Connection) -> Option<String> {
    for (dest, path) in [
        ("org.freedesktop.UPower.PowerProfiles", "/org/freedesktop/UPower/PowerProfiles"),
        ("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles"),
    ] {
        if let Ok(p) = zbus::blocking::Proxy::new(conn, dest, path, dest) {
            if let Ok(v) = p.get_property::<String>("ActiveProfile") {
                return Some(v);
            }
        }
    }
    None
}

/// Executables of a uid's processes (`/proc/*/exe`), for automation rules.
pub fn executables(uid: u32) -> HashSet<String> {
    let mut out = HashSet::new();
    let Ok(rd) = std::fs::read_dir("/proc") else { return out };
    for e in rd.flatten() {
        let name = e.file_name();
        let Some(pid) = name.to_str().filter(|s| s.bytes().all(|b| b.is_ascii_digit())) else { continue };
        let base = PathBuf::from("/proc").join(pid);
        let owned = std::fs::metadata(&base).map(|m| std::os::unix::fs::MetadataExt::uid(&m) == uid).unwrap_or(false);
        if owned {
            if let Ok(exe) = std::fs::read_link(base.join("exe")) {
                out.insert(exe.to_string_lossy().into_owned());
            }
        }
    }
    out
}
