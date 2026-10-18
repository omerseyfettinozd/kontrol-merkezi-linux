//! Control mode: the single writer. One thread owns every hardware write; RPCs are
//! queued to it. The 1 s control loop renews the fan lease only from a fresh sample,
//! applies the guard and stall detection, watches for foreign cpufreq writes
//! (nvidia-powerd), runs automation and notifies the systemd watchdog.

pub mod power;
pub mod rgb;
pub mod sleep;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use r9t_hal::cpufreq::Cpufreq;
use r9t_hal::fan::{FanBridge, FanCommand, SysfsFan};
use r9t_hal::gpu::Nvml;
use r9t_hal::power::{Power, Source};
use r9t_hal::rgb::Rgb;
use r9t_hal::{HalError, Sysfs};
use r9t_protocol::policy::PolicyDocument;
use r9t_protocol::presets::{self, FanPreset};
use r9t_protocol::profile::{FanCurve, FanSetting, PowerProfile, ProfileSettings, DUTY_FLOOR};
use r9t_protocol::rpc::{code, Call, RgbEffect, RpcError, SessionEvent};
use r9t_protocol::snapshot::{AuthoritySnapshot, AuthorityState};

use crate::audit::{Audit, Event};
use crate::policy::{Action, Automation, Observation};
use crate::sampler::{unix_ms, Shared};
use crate::server::Peer;
use crate::thermal::stall::{StallAction, StallDetector};
use crate::thermal::target::{validate_targets, TemperatureTarget};
use crate::thermal::{Authority, Decision, Mode};

use power::{GpuClocks, PowerState, Ppd};
use rgb::{Inhibit, Msg as RgbMsg, RgbEngine};

/// Lease renewal period; the kernel lease is 15 s (safety invariant 1).
pub const RENEW: Duration = Duration::from_secs(3);
/// A sample older than this never renews the lease.
pub const FRESH_MS: u64 = 5000;

pub enum Msg {
    Call { call: Call, peer: Peer, conn_id: u64, reply: mpsc::Sender<Result<Value, RpcError>> },
    ConnClosed(u64),
    Sleep { entering: bool, ack: mpsc::Sender<()> },
    Shutdown(mpsc::Sender<()>),
}

#[derive(Clone)]
pub struct ControlHandle {
    tx: mpsc::Sender<Msg>,
}

impl ControlHandle {
    pub fn call(&self, call: Call, peer: Peer, conn_id: u64) -> Result<Value, RpcError> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Msg::Call { call, peer, conn_id, reply })
            .map_err(|_| RpcError::new(code::INTERNAL, "kontrol iş parçacığı yok"))?;
        rx.recv_timeout(Duration::from_secs(30)).map_err(|_| RpcError::new(code::BUSY, "kontrol zaman aşımı"))?
    }

    pub fn conn_closed(&self, conn_id: u64) {
        let _ = self.tx.send(Msg::ConnClosed(conn_id));
    }

    pub fn sleep(&self, entering: bool) {
        let (ack, rx) = mpsc::channel();
        if self.tx.send(Msg::Sleep { entering, ack }).is_ok() {
            let _ = rx.recv_timeout(sleep::MAX_DELAY);
        }
    }

    pub fn shutdown(&self) {
        let (ack, rx) = mpsc::channel();
        if self.tx.send(Msg::Shutdown(ack)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(10));
        }
    }
}

struct SessionInfo {
    conn_id: u64,
    uid: u32,
    desktop: String,
    id: String,
}

struct TargetSaved {
    cpu: Vec<(String, String)>,
    gpu_mhz: Option<u32>,
    mode: Mode,
}

pub struct Control {
    shared: Arc<Shared>,
    fs: Sysfs,
    fan: SysfsFan,
    cpu: Cpufreq,
    ppd: Ppd,
    gpu: GpuClocks<Nvml>,
    power: Power,
    rgb_hal: Option<Arc<Rgb>>,
    rgb: Option<RgbEngine>,
    authority: Authority,
    stall: StallDetector,
    stall_boost_until: Option<Instant>,
    last_cmd: Option<FanCommand>,
    last_write: Option<Instant>,
    baseline: PowerState,
    expected_cpu_max: Option<Vec<(String, String)>>,
    last_foreign_check: Instant,
    target_saved: Option<TargetSaved>,
    automation: Automation,
    state_dir: PathBuf,
    session: Option<SessionInfo>,
    classes: HashSet<String>,
    idle: bool,
    saved_brightness: Option<u32>,
    sleeping: bool,
    resume_at: Option<Instant>,
    valid_after_resume: u8,
    active_profile: Option<String>,
    audit: Arc<Mutex<Audit>>,
    last_exe_scan: Option<Instant>,
    executables: HashSet<String>,
    desktop_uid: Option<u32>,
    /// Cumulative settings the user asked for; reapplied after a restart.
    desired: ProfileSettings,
}

/// Defaults captured at the first control start of a trial (`r9t-switch` clears it).
pub const BASELINE: &str = "baseline.json";
/// Last requested settings, reapplied on restart (plan Faz 3: ≤ 5 s).
pub const LAST: &str = "last.json";

fn hal(e: HalError) -> RpcError {
    let c = match e {
        HalError::Permission(_) => code::UNAUTHORIZED,
        HalError::Unsupported(_) => code::UNSUPPORTED,
        HalError::Busy(_) => code::BUSY,
        HalError::Invalid(_) => code::INVALID_PARAMS,
        _ => code::HARDWARE,
    };
    RpcError::new(c, e.to_string())
}

fn ok(message: impl Into<String>, verified: bool) -> Result<Value, RpcError> {
    Ok(json!({"verified": verified, "message": message.into()}))
}

impl Control {
    /// Start-up sequence for control mode. Fails (→ daemon stays read-only) when the
    /// fan bridge is not 0.2.0.
    pub fn start(shared: Arc<Shared>, fs: Sysfs, state_dir: &Path, audit: Arc<Mutex<Audit>>) -> Result<(Self, ControlHandle, mpsc::Receiver<Msg>), String> {
        let fan = SysfsFan::new(fs.clone());
        if fan.version().as_deref() != Some("0.2.0") {
            return Err(format!("r9t_fan 0.2.0 gerekli, yüklü: {:?}", fan.version()));
        }
        // Safety invariant 1: start from firmware control.
        let st = fan.status().map_err(|e| format!("r9t_fan status: {e}"))?;
        if !st.is_auto() {
            fan.control(FanCommand::Auto).map_err(|e| format!("auto yazılamadı: {e}"))?;
            std::thread::sleep(Duration::from_millis(500));
            let st = fan.status().map_err(|e| format!("r9t_fan status: {e}"))?;
            if !st.is_auto() {
                return Err("açılışta fan auto doğrulanamadı".into());
            }
        }
        let cpu = Cpufreq::new(fs.clone());
        let ppd = Ppd::new();
        let mut gpu = GpuClocks::new(Nvml::new(fs.clone()), state_dir);
        if let Ok(true) = gpu.recover() {
            eprintln!("r9td: önceki çalıştırmadan kalan GPU saat kilidi sıfırlandı");
        }
        recover_target_journal(&cpu, state_dir);
        // Baseline read only now, after the legacy service stopped (plan invariant 2),
        // and persisted so a crash/restart restores the same defaults (ExecStopPost too).
        let baseline_path = state_dir.join(BASELINE);
        let baseline = match std::fs::read(&baseline_path).ok().and_then(|b| serde_json::from_slice::<PowerState>(&b).ok()) {
            Some(b) => b,
            None => {
                let b = power::capture(&cpu, &ppd, &gpu);
                let _ = write_atomic(&baseline_path, &serde_json::to_vec(&b).unwrap_or_default());
                b
            }
        };
        let desired = std::fs::read(state_dir.join(LAST))
            .ok()
            .and_then(|b| serde_json::from_slice::<ProfileSettings>(&b).ok())
            .unwrap_or_default();
        let rgb_hal = Rgb::discover(fs.clone()).ok().map(Arc::new);
        let rgb = rgb_hal.as_ref().map(|r| {
            let _ = r.release_buffer();
            let base = r.read_frame().unwrap_or_else(|_| rgb::solid([255, 255, 255]));
            RgbEngine::start(r.clone(), base)
        });
        let policy_path = state_dir.join("policy.json");
        let doc = std::fs::read(&policy_path)
            .ok()
            .and_then(|b| serde_json::from_slice::<PolicyDocument>(&b).ok())
            .filter(|d| d.validate().is_ok())
            .unwrap_or_default();
        let paused = state_dir.join("policy.paused").exists();
        let (tx, rx) = mpsc::channel();
        let c = Self {
            power: Power::new(fs.clone()),
            shared,
            fs,
            fan,
            cpu,
            ppd,
            gpu,
            rgb_hal,
            rgb,
            authority: Authority::default(),
            stall: StallDetector::default(),
            stall_boost_until: None,
            last_cmd: None,
            last_write: None,
            baseline,
            expected_cpu_max: None,
            last_foreign_check: Instant::now(),
            target_saved: None,
            automation: Automation::new(doc, paused),
            state_dir: state_dir.to_path_buf(),
            session: None,
            classes: HashSet::new(),
            idle: false,
            saved_brightness: None,
            sleeping: false,
            resume_at: None,
            valid_after_resume: 0,
            active_profile: None,
            audit,
            last_exe_scan: None,
            executables: HashSet::new(),
            desktop_uid: None,
            desired,
        };
        Ok((c, ControlHandle { tx }, rx))
    }

    pub fn run(mut self, rx: mpsc::Receiver<Msg>, notify: impl Fn(&str)) {
        if !self.desired.is_empty() {
            let d = self.desired.clone();
            let r = self.apply_profile(&d);
            self.note("restore", r.is_ok(), match r {
                Ok(_) => "yeniden başlatma: son istenen ayarlar uygulandı".into(),
                Err(e) => format!("yeniden başlatma: son ayarlar uygulanamadı: {}", e.message),
            });
        }
        self.publish();
        let mut next_tick = Instant::now();
        loop {
            let timeout = next_tick.saturating_duration_since(Instant::now());
            match rx.recv_timeout(timeout) {
                Ok(Msg::Call { call, peer, conn_id, reply }) => {
                    let started = Instant::now();
                    let name = call.name();
                    let r = self.handle(call, &peer, conn_id);
                    self.record(name, &peer, r.is_ok(), started, r.as_ref().err().map(|e| e.message.clone()).unwrap_or_default());
                    let _ = reply.send(r);
                    self.publish();
                }
                Ok(Msg::ConnClosed(id)) => {
                    if self.session.as_ref().is_some_and(|s| s.conn_id == id) {
                        self.session = None;
                        self.classes.clear();
                        self.set_idle(false);
                    }
                }
                Ok(Msg::Sleep { entering, ack }) => {
                    if entering {
                        self.before_sleep();
                    } else {
                        self.sleeping = false;
                        self.resume_at = Some(Instant::now());
                        self.valid_after_resume = 0;
                    }
                    let _ = ack.send(());
                }
                Ok(Msg::Shutdown(ack)) => {
                    self.shutdown();
                    let _ = ack.send(());
                    return;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.shutdown();
                    return;
                }
            }
            if Instant::now() >= next_tick {
                self.tick();
                notify("WATCHDOG=1");
                next_tick = Instant::now() + Duration::from_secs(1);
            }
        }
    }

    fn record(&self, method: &str, peer: &Peer, ok: bool, started: Instant, message: String) {
        self.audit.lock().unwrap_or_else(|p| p.into_inner()).record(Event {
            time_ms: unix_ms(),
            method: method.into(),
            ok,
            uid: Some(peer.uid),
            pid: Some(peer.pid),
            duration_ms: started.elapsed().as_millis() as u64,
            message,
        });
    }

    fn note(&self, method: &str, ok: bool, message: impl Into<String>) {
        self.audit.lock().unwrap_or_else(|p| p.into_inner()).record(Event {
            time_ms: unix_ms(),
            method: method.into(),
            ok,
            uid: None,
            pid: None,
            duration_ms: 0,
            message: message.into(),
        });
    }

    fn publish(&self) {
        let owns = !matches!(self.authority.mode, Mode::Auto) || self.authority.guard.active();
        self.shared.owns_fans.store(owns, Ordering::Relaxed);
        let mut live = self.shared.live.write().unwrap_or_else(|p| p.into_inner());
        live.authority = Some(AuthoritySnapshot {
            state: AuthorityState::Control,
            reason: match &self.authority.mode {
                Mode::Auto => "EC otomatik".into(),
                Mode::Boost => "maksimum fan".into(),
                Mode::Manual { cpu, gpu } => format!("manuel %{cpu}/%{gpu}"),
                Mode::Curve { .. } => "fan eğrisi".into(),
                Mode::Target(_) => "sıcaklık hedefi".into(),
            },
            active_profile: self.active_profile.clone().or_else(|| self.automation.active_label()),
            guard_active: self.authority.guard.active(),
            session: self.session.as_ref().map(|s| format!("{}:{}", s.desktop, s.id)),
        });
    }

    // ---------------- fan writes ----------------

    fn write_fan(&mut self, cmd: FanCommand) -> Result<(), HalError> {
        self.fan.control(cmd)?;
        self.last_cmd = Some(cmd);
        self.last_write = Some(Instant::now());
        Ok(())
    }

    /// Writes `auto` and confirms `mode=0` and EC bit 0x40 clear.
    fn fan_auto(&mut self) -> Result<bool, HalError> {
        self.fan.control(FanCommand::Auto)?;
        self.last_cmd = None;
        self.last_write = Some(Instant::now());
        let st = self.fan.status()?;
        Ok(st.is_auto())
    }

    fn set_mode(&mut self, mode: Mode) -> Result<Value, RpcError> {
        let auto = matches!(mode, Mode::Auto);
        self.authority.set_mode(mode);
        self.stall = StallDetector::default();
        if auto {
            return match self.fan_auto() {
                Ok(true) => ok("Fanlar EC otomatiğinde; mod biti doğrulandı.", true),
                Ok(false) => Err(RpcError::new(code::HARDWARE, "auto yazıldı ama mod biti doğrulanamadı")),
                Err(e) => Err(hal(e)),
            };
        }
        // Apply at once with the newest sample (the loop keeps renewing it).
        self.control_step(Instant::now());
        ok("Fan hedefi uygulandı; PWM ve RPM doğrulaması kontrol döngüsünde sürüyor.", false)
    }

    fn temps(&self, now: Instant) -> (Option<f32>, Option<f32>, bool) {
        let live = self.shared.live.read().unwrap_or_else(|p| p.into_inner());
        (
            live.cpu_ec_c.fresh(now, FRESH_MS),
            live.gpu_ec_c.fresh(now, FRESH_MS),
            live.thermal_override.fresh(now, 15_000).unwrap_or(false),
        )
    }

    /// One authority decision + write. Renewal only with fresh temperatures.
    fn control_step(&mut self, now: Instant) {
        let (c, g, kernel_override) = self.temps(now);
        if let Some(until) = self.stall_boost_until {
            if now < until {
                if self.last_write.is_none_or(|t| now.duration_since(t) >= RENEW) {
                    let _ = self.write_fan(FanCommand::Boost);
                }
                return;
            }
            self.stall_boost_until = None;
        }
        let decision = self.authority.decide(c, g, kernel_override, now);
        let fresh = c.is_some() && g.is_some();
        match decision {
            Decision::Idle => {}
            Decision::Write(cmd) | Decision::WriteWithLimits { cmd, .. } if !fresh => {
                // Invariant 1: a stale sample never renews the lease; the kernel falls
                // back to auto after 15 s, the authority after 10 s (invariant 8).
                let _ = cmd;
            }
            Decision::Write(cmd) => {
                if self.last_cmd != Some(cmd) || self.last_write.is_none_or(|t| now.duration_since(t) >= RENEW) {
                    if let Err(e) = self.write_fan(cmd) {
                        self.note("fan", false, format!("fan yazılamadı: {e}"));
                    }
                }
            }
            Decision::WriteWithLimits { cmd, cpu_mhz, gpu_mhz } => {
                if let Err(e) = self.write_fan(cmd) {
                    self.note("fan", false, format!("fan yazılamadı: {e}"));
                }
                if let Some(m) = cpu_mhz {
                    if let Err(e) = self.cpu.set_max_mhz(m) {
                        self.stop_target(true, &format!("CPU sınırı yazılamadı: {e}"));
                        return;
                    }
                    self.expected_cpu_max = self.cpu.snapshot_max().ok();
                }
                if let Some(m) = gpu_mhz {
                    if let Err(e) = self.gpu.set(m) {
                        self.stop_target(true, &format!("GPU sınırı yazılamadı: {e}"));
                    }
                }
            }
            Decision::FallbackAuto(reason) => {
                let ok = self.fan_auto().unwrap_or(false);
                self.note("fan", ok, format!("EC otomatiğine dönüldü: {reason}"));
                if self.target_saved.is_some() {
                    self.stop_target(true, &reason);
                }
            }
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        if self.sleeping {
            return;
        }
        if let Some(t) = self.resume_at {
            // Invariant 7: 3 s after resume and two valid EC samples before writing.
            let (c, g, _) = self.temps(now);
            if now.duration_since(t) < Duration::from_secs(3) || c.is_none() || g.is_none() {
                return;
            }
            self.valid_after_resume += 1;
            if self.valid_after_resume < 2 {
                return;
            }
            self.resume_at = None;
            self.note("resume", true, "uyanma sonrası politika yeniden uygulanıyor");
        }
        self.control_step(now);
        self.check_stall(now);
        if now.duration_since(self.last_foreign_check) >= Duration::from_secs(10) {
            self.last_foreign_check = now;
            self.check_foreign_cpu_writes();
        }
        self.run_automation(now);
        self.update_rgb_inhibit();
        self.publish();
    }

    fn check_stall(&mut self, now: Instant) {
        if self.last_cmd.is_none() {
            return;
        }
        let (duty, rpm, rpm_age) = {
            let live = self.shared.live.read().unwrap_or_else(|p| p.into_inner());
            let d = (live.cpu_duty.fresh(now, FRESH_MS), live.gpu_duty.fresh(now, FRESH_MS));
            let r = (live.cpu_rpm.value, live.gpu_rpm.value);
            let age = live.cpu_rpm.at.map(|t| now.duration_since(t));
            (d, r, age)
        };
        let action = match (duty, rpm, rpm_age) {
            ((Some(cd), Some(gd)), (Some(cr), Some(gr)), Some(age)) if age < Duration::from_secs(4) => {
                self.stall.sample((cd, gd), (cr as i32, gr as i32), now)
            }
            (_, _, Some(age)) if age >= Duration::from_secs(10) => self.stall.status_error(now),
            _ => StallAction::None,
        };
        match action {
            StallAction::None => {}
            StallAction::Boost => {
                self.stall_boost_until = Some(now + Duration::from_secs(10));
                let _ = self.write_fan(FanCommand::Boost);
                self.note("stall", false, "fan durması şüphesi: iki fan %100");
            }
            StallAction::Auto => {
                self.authority.set_mode(Mode::Auto);
                self.stall_boost_until = None;
                let ok = self.fan_auto().unwrap_or(false);
                self.note("stall", ok, "fan durması sürdü: EC otomatiğine dönüldü");
            }
        }
    }

    /// nvidia-powerd (Dynamic Boost) may rewrite `scaling_max_freq`; detect and reapply.
    fn check_foreign_cpu_writes(&mut self) {
        let Some(expected) = &self.expected_cpu_max else { return };
        let Ok(now) = self.cpu.snapshot_max() else { return };
        if &now != expected {
            let msg = "scaling_max_freq başka bir süreç tarafından değiştirildi (nvidia-powerd?); yeniden uygulanıyor";
            let _ = self.cpu.restore_max(expected);
            self.note("cpu_limit", false, msg);
        }
    }

    fn run_automation(&mut self, now: Instant) {
        if self.automation.paused {
            return;
        }
        if self.last_exe_scan.is_none_or(|t| now.duration_since(t) >= Duration::from_secs(2)) {
            self.last_exe_scan = Some(now);
            if let Some(uid) = self.desktop_uid.or(self.session.as_ref().map(|s| s.uid)) {
                self.executables = crate::sampler::executables(uid);
            }
        }
        let obs = Observation { source: self.power.source(), executables: &self.executables, classes: &self.classes };
        match self.automation.tick(&obs, now) {
            Action::None => {}
            Action::Apply { name, settings, reason, snapshot_first } => {
                if snapshot_first {
                    self.save_snapshot();
                }
                let r = self.apply_profile(&settings);
                self.active_profile = Some(name);
                self.note("automation", r.is_ok(), match r {
                    Ok(_) => reason,
                    Err(e) => format!("{reason}: {}", e.message),
                });
            }
            Action::Restore { reason } => {
                if let Some(s) = self.load_snapshot() {
                    let r = self.apply_profile(&s);
                    self.note("automation", r.is_ok(), reason);
                }
                self.active_profile = None;
            }
        }
    }

    fn snapshot_settings(&self) -> ProfileSettings {
        let live = self.shared.live.read().unwrap_or_else(|p| p.into_inner());
        ProfileSettings {
            power_profile: live.profile.value.as_deref().and_then(|p| serde_json::from_value(Value::from(p)).ok()),
            boost_enabled: live.boost.value,
            cpu_max_mhz: self.cpu.limits().ok().and_then(|l| l.max_mhz),
            gpu_max_mhz: Some(self.gpu.requested.unwrap_or(0)),
            cpu_epp: live.epp.value.as_deref().and_then(r9t_protocol::profile::Epp::parse),
            fan: Some(match &self.authority.mode {
                Mode::Auto | Mode::Target(_) => FanSetting::Auto,
                Mode::Boost => FanSetting::Boost,
                Mode::Manual { cpu, gpu } => FanSetting::Manual { cpu: *cpu, gpu: *gpu },
                Mode::Curve { curve, .. } => FanSetting::Curve { points: curve.points.clone(), interpolation: curve.interpolation },
            }),
            ..Default::default()
        }
    }

    fn save_snapshot(&self) {
        let s = self.snapshot_settings();
        let _ = std::fs::write(self.state_dir.join("automation-snapshot.json"), serde_json::to_vec(&s).unwrap_or_default());
    }

    fn load_snapshot(&self) -> Option<ProfileSettings> {
        let p = self.state_dir.join("automation-snapshot.json");
        let s = serde_json::from_slice(&std::fs::read(&p).ok()?).ok();
        let _ = std::fs::remove_file(p);
        s
    }

    fn update_rgb_inhibit(&mut self) {
        let lid_closed = self.fs.list("/proc/acpi/button/lid").iter().any(|l| {
            self.fs.read(format!("/proc/acpi/button/lid/{l}/state")).is_ok_and(|s| s.contains("closed"))
        });
        let inhibit = Inhibit { battery: self.power.source() == Source::Battery, idle: self.idle, lid_closed };
        if let Some(r) = &self.rgb {
            r.send(RgbMsg::Inhibit(inhibit));
            if let Some(c) = self.shared.live.read().unwrap_or_else(|p| p.into_inner()).cpu_ec_c.value {
                r.send(RgbMsg::CpuTemp(c));
            }
        }
    }

    fn set_idle(&mut self, idle: bool) {
        if idle == self.idle {
            return;
        }
        self.idle = idle;
        let Some(r) = &self.rgb_hal else { return };
        if idle {
            if let Ok(b) = r.brightness() {
                if b > 0 {
                    self.saved_brightness = Some(b);
                    let _ = r.set_brightness(0);
                }
            }
        } else if let Some(b) = self.saved_brightness.take() {
            // Restore only if nobody changed brightness meanwhile.
            if r.brightness().ok() == Some(0) {
                let _ = r.set_brightness(b);
            }
        }
    }

    // ---------------- profiles ----------------

    fn apply_fan_setting(&mut self, f: &FanSetting) -> Result<(), RpcError> {
        f.validate().map_err(|e| RpcError::new(code::INVALID_PARAMS, e.0))?;
        let mode = match f {
            FanSetting::Auto => Mode::Auto,
            FanSetting::Boost => Mode::Boost,
            FanSetting::Manual { cpu, gpu } => Mode::Manual { cpu: *cpu, gpu: *gpu },
            FanSetting::Curve { points, interpolation } => {
                Mode::Curve { curve: FanCurve { points: points.clone(), interpolation: *interpolation }, smoothers: None }
            }
            FanSetting::Preset { preset } => match presets::fan_preset(preset) {
                Some(FanPreset::Curve(c)) => Mode::Curve { curve: c, smoothers: None },
                Some(FanPreset::Boost) => Mode::Boost,
                None => return Err(RpcError::new(code::INVALID_PARAMS, "Bilinmeyen fan profili.")),
            },
        };
        self.set_mode(mode).map(|_| ())
    }

    fn brightness_level(&self, pct: u8) -> u32 {
        let max = self.rgb_hal.as_ref().map(|r| r.max_brightness()).unwrap_or(50);
        (u32::from(pct.min(100)) * max + 50) / 100
    }

    fn apply_rgb(&mut self, frame: rgb::Frame, brightness_pct: u8) -> Result<(), RpcError> {
        let level = self.brightness_level(brightness_pct);
        let (Some(engine), Some(r)) = (&self.rgb, &self.rgb_hal) else {
            return Err(RpcError::new(code::UNSUPPORTED, "126 kanallı RGB arayüzü yok"));
        };
        engine.send(RgbMsg::Static(frame));
        r.set_brightness(level).map_err(hal)
    }

    /// Full profile: power part atomically (with rollback), then RGB and fan.
    /// Merges `s` into the persisted desired state.
    fn remember(&mut self, s: &ProfileSettings) {
        let d = &mut self.desired;
        macro_rules! take {
            ($($f:ident),*) => { $( if s.$f.is_some() { d.$f = s.$f.clone(); } )* };
        }
        take!(power_profile, boost_enabled, cpu_max_mhz, gpu_max_mhz, cpu_epp, fan, brightness);
        if s.rgb.is_some() {
            d.rgb = s.rgb;
            d.rgb_map = None;
        }
        if s.rgb_map.is_some() {
            d.rgb_map = s.rgb_map.clone();
            d.rgb = None;
        }
        let _ = write_atomic(&self.state_dir.join(LAST), &serde_json::to_vec(&self.desired).unwrap_or_default());
    }

    fn apply_profile(&mut self, s: &ProfileSettings) -> Result<Value, RpcError> {
        let r = self.apply_profile_inner(s);
        if r.is_ok() {
            self.remember(s);
        }
        r
    }

    fn apply_profile_inner(&mut self, s: &ProfileSettings) -> Result<Value, RpcError> {
        s.validate().map_err(|e| RpcError::new(code::INVALID_PARAMS, e.0))?;
        if self.target_saved.is_some() {
            self.stop_target(false, "profil uygulanıyor");
        }
        power::apply(&self.cpu, &self.ppd, &mut self.gpu, s).map_err(hal)?;
        if s.cpu_max_mhz.is_some() {
            self.expected_cpu_max = self.cpu.snapshot_max().ok();
        }
        if let (Some(c), Some(b)) = (s.rgb, s.brightness) {
            self.apply_rgb(rgb::solid(c), b)?;
        }
        if let (Some(m), Some(b)) = (&s.rgb_map, s.brightness) {
            self.apply_rgb(m.clone(), b)?;
        }
        if let Some(f) = &s.fan {
            self.apply_fan_setting(f)?;
        }
        let gpu_note = if s.gpu_max_mhz.is_some_and(|m| m > 0) { " GPU sınırının bağımsız geri okuması yok." } else { "" };
        ok(format!("Profil uygulandı; geri okunabilen ayarlar doğrulandı.{gpu_note}"), s.fan.is_none() && s.gpu_max_mhz.is_none())
    }

    // ---------------- temperature target ----------------

    fn start_target(&mut self, cpu_t: u8, gpu_t: u8) -> Result<Value, RpcError> {
        validate_targets(cpu_t, gpu_t).map_err(|m| RpcError::new(code::INVALID_PARAMS, m))?;
        let now = Instant::now();
        let (c, g, _) = self.temps(now);
        let (Some(c), Some(g)) = (c, g) else {
            return Err(RpcError::new(code::HARDWARE, "Güncel CPU/GPU EC sıcaklığı yok."));
        };
        let limits = self.cpu.limits().map_err(hal)?;
        let snapshot = self.cpu.snapshot_max().map_err(hal)?;
        let cpu_ceiling = limits.max_mhz.unwrap_or(limits.allowed_max_mhz);
        let gpu_max = self.shared.live.read().unwrap_or_else(|p| p.into_inner()).gpu_max_clock_mhz.value.unwrap_or(3090);
        let gpu_ceiling = self.gpu.requested.unwrap_or(gpu_max).min(gpu_max);
        // Persist before the first write; start-up restores CPU policies from it.
        let journal: serde_json::Map<String, Value> = snapshot.iter().map(|(p, v)| (p.clone(), Value::from(v.parse::<u64>().unwrap_or(0)))).collect();
        write_atomic(&self.state_dir.join("temp-target-recovery.json"), &serde_json::to_vec(&journal).unwrap_or_default())
            .map_err(|e| RpcError::new(code::HARDWARE, format!("kurtarma kaydı yazılamadı: {e}")))?;
        let prev_mode = std::mem::replace(&mut self.authority.mode, Mode::Auto);
        self.target_saved = Some(TargetSaved { cpu: snapshot, gpu_mhz: self.gpu.requested, mode: prev_mode });
        let tt = TemperatureTarget::new((cpu_t, gpu_t), (c, g), (cpu_ceiling, gpu_ceiling), limits.allowed_min_mhz);
        self.authority.set_mode(Mode::Target(Box::new(tt)));
        self.control_step(now);
        ok("Sıcaklık hedefi etkin; fan ve frekans yönetimi başladı. Hedef sıcaklık garanti edilmez.", false)
    }

    fn stop_target(&mut self, emergency: bool, reason: &str) {
        let Some(saved) = self.target_saved.take() else { return };
        let mut errors = Vec::new();
        if let Err(e) = self.cpu.restore_max(&saved.cpu) {
            errors.push(e.to_string());
        }
        if let Err(e) = self.gpu.set(saved.gpu_mhz.unwrap_or(0)) {
            errors.push(e.to_string());
        }
        if emergency {
            self.authority.set_mode(Mode::Auto);
            if !self.fan_auto().unwrap_or(false) {
                errors.push("fan auto doğrulanamadı".into());
            }
        } else {
            self.authority.set_mode(saved.mode);
        }
        if errors.is_empty() {
            let _ = std::fs::remove_file(self.state_dir.join("temp-target-recovery.json"));
        }
        self.note("temperature_target_stop", errors.is_empty(), format!("{reason} {}", errors.join("; ")));
    }

    // ---------------- sleep / shutdown ----------------

    fn before_sleep(&mut self) {
        self.sleeping = true;
        if let Some(r) = &self.rgb {
            r.send(RgbMsg::Inhibit(Inhibit { idle: true, ..Default::default() }));
        }
        let ok = self.fan_auto().unwrap_or(false);
        self.note("sleep", ok, if ok { "uyku öncesi fan auto doğrulandı" } else { "uyku öncesi fan auto doğrulanamadı" });
    }

    fn shutdown(&mut self) {
        if self.target_saved.is_some() {
            self.stop_target(true, "kapanış");
        }
        let fan_ok = self.fan_auto().unwrap_or(false);
        let restore = power::restore(&self.cpu, &self.ppd, &mut self.gpu, &self.baseline.clone());
        if let Some(mut r) = self.rgb.take() {
            r.stop();
        }
        if let Some(r) = &self.rgb_hal {
            let _ = r.release_buffer();
        }
        self.note("stop", fan_ok && restore.is_ok(), format!("kapanış: fan auto={fan_ok}, sınırlar={:?}", restore.err().map(|e| e.to_string())));
    }

    // ---------------- RPC ----------------

    fn handle(&mut self, call: Call, peer: &Peer, conn_id: u64) -> Result<Value, RpcError> {
        if self.sleeping && !matches!(call, Call::SessionEvent(_)) {
            return Err(RpcError::new(code::BUSY, "Sistem uykuya geçiyor."));
        }
        match call {
            Call::Auto(_) => self.fan_rpc(FanSetting::Auto),
            Call::FanBoost(_) => self.fan_rpc(FanSetting::Boost),
            Call::FanPreset(p) => self.fan_rpc(FanSetting::Preset { preset: p.preset }),
            Call::Manual(p) => {
                if !(DUTY_FLOOR..=100).contains(&p.cpu) || !(DUTY_FLOOR..=100).contains(&p.gpu) {
                    return Err(RpcError::new(code::INVALID_PARAMS, "CPU/GPU fanı %50–100 olmalı."));
                }
                self.fan_rpc(FanSetting::Manual { cpu: p.cpu, gpu: p.gpu })
            }
            Call::Curve(p) => {
                self.fan_rpc(FanSetting::Curve { points: p.points, interpolation: p.interpolation })
            }
            Call::Power(p) => self.apply_profile(&ProfileSettings { power_profile: Some(p.profile), ..Default::default() }),
            Call::Boost(p) => self.apply_profile(&ProfileSettings { boost_enabled: Some(p.enabled), ..Default::default() }),
            Call::ThermalPreset(p) => {
                let s = presets::thermal_preset(&p.preset).ok_or_else(|| RpcError::new(code::INVALID_PARAMS, "Bilinmeyen soğutma profili."))?;
                let r = self.apply_profile(&s);
                if r.is_ok() {
                    self.active_profile = Some(p.preset);
                }
                r
            }
            Call::Profile(p) => {
                let r = self.apply_profile(&p.settings);
                if r.is_ok() {
                    self.active_profile = None;
                }
                r
            }
            Call::ApplyProfile(p) => {
                let s = self.automation.doc.profiles.get(&p.name).cloned().ok_or_else(|| RpcError::new(code::INVALID_PARAMS, "Profil bulunamadı."))?;
                let r = self.apply_profile(&s);
                if r.is_ok() {
                    self.active_profile = Some(p.name);
                }
                r
            }
            Call::Scenario(p) => self.scenario(&p.name),
            Call::Rgb(p) => {
                if p.brightness > 100 {
                    return Err(RpcError::new(code::INVALID_PARAMS, "Parlaklık %0–100 olmalı."));
                }
                self.apply_rgb(rgb::solid(p.rgb), p.brightness)?;
                self.remember(&ProfileSettings { rgb: Some(p.rgb), brightness: Some(p.brightness), ..Default::default() });
                ok("Klavye rengi uygulandı.", true)
            }
            Call::RgbMap(p) => {
                if p.colors.len() != r9t_hal::rgb::CHANNELS || p.brightness > 100 {
                    return Err(RpcError::new(code::INVALID_PARAMS, "126 renk ve %0–100 parlaklık gerekli."));
                }
                self.apply_rgb(p.colors.clone(), p.brightness)?;
                self.remember(&ProfileSettings { rgb_map: Some(p.colors), brightness: Some(p.brightness), ..Default::default() });
                ok("Klavye haritası uygulandı.", true)
            }
            Call::RgbEffect(p) => {
                let r = self.rgb.as_ref().ok_or_else(|| RpcError::new(code::UNSUPPORTED, "RGB yok"))?;
                if let RgbEffect::Wave { speed } = p.effect {
                    if !(1..=10).contains(&speed) {
                        return Err(RpcError::new(code::INVALID_PARAMS, "Dalga hızı 1–10 olmalı."));
                    }
                }
                r.send(RgbMsg::Effect(p.effect));
                ok("Klavye efekti başlatıldı (pilde, boşta ve kapak kapalıyken durur).", true)
            }
            Call::RgbFrame(p) => {
                let r = self.rgb.as_ref().ok_or_else(|| RpcError::new(code::UNSUPPORTED, "RGB yok"))?;
                if p.colors.len() != r9t_hal::rgb::CHANNELS {
                    return Err(RpcError::new(code::INVALID_PARAMS, "126 renk gerekli."));
                }
                r.send(RgbMsg::AudioFrame(p.colors));
                Ok(json!({"accepted": true}))
            }
            Call::LightingIdle(p) => {
                self.set_idle(p.idle);
                ok("Klavye boşta kalma aydınlatması güncellendi.", true)
            }
            Call::FnLock(p) => {
                let path = "/sys/devices/platform/tuxedo_keyboard/fn_lock";
                self.fs.write(path, if p.enabled { "1" } else { "0" }).map_err(hal)?;
                if self.fs.read(path).map_err(hal)? != if p.enabled { "1" } else { "0" } {
                    return Err(RpcError::new(code::HARDWARE, "Fn Lock geri okuması eşleşmedi."));
                }
                ok("Fn Lock sistemden doğrulandı.", true)
            }
            Call::Camera(p) => camera(&self.fs, p.enabled),
            Call::ChargeLimit(_) => Err(RpcError::new(code::UNSUPPORTED, "Bu makinede şarj eşiği arayüzü yok (BAT0 charge_control_end_threshold).")),
            Call::TemperatureTarget(p) => self.start_target(p.cpu, p.gpu),
            Call::TemperatureTargetStop(_) => {
                if self.target_saved.is_none() {
                    return ok("Sıcaklık hedefi kapalı.", true);
                }
                self.stop_target(false, "kullanıcı");
                ok("Sıcaklık hedefi kapatıldı; önceki frekans sınırları geri yüklendi.", true)
            }
            Call::Configure(p) => {
                p.document.validate().map_err(|e| RpcError::new(code::INVALID_PARAMS, e.0))?;
                let bytes = serde_json::to_vec_pretty(&p.document).map_err(|e| RpcError::new(code::INTERNAL, e.to_string()))?;
                write_atomic(&self.state_dir.join("policy.json"), &bytes).map_err(|e| RpcError::new(code::HARDWARE, e.to_string()))?;
                self.automation.configure(p.document);
                ok("Profiller ve otomasyon kuralları kaydedildi.", true)
            }
            Call::AutomationPause(_) => {
                let _ = std::fs::write(self.state_dir.join("policy.paused"), b"{\"paused\":true}\n");
                self.automation.pause();
                ok("Otomasyon duraklatıldı.", true)
            }
            Call::AutomationResume(_) => {
                let _ = std::fs::remove_file(self.state_dir.join("policy.paused"));
                self.automation.resume();
                ok("Otomasyon yeniden etkin.", true)
            }
            Call::SessionRegister(p) => {
                if p.pid != peer.pid as u32 {
                    return Err(RpcError::new(code::UNAUTHORIZED, "pid bağlantının sahibiyle eşleşmiyor."));
                }
                if let Some(owner) = logind_session_of(p.pid) {
                    if !owner.ends_with(&p.logind_session_id) {
                        return Err(RpcError::new(code::UNAUTHORIZED, "pid bu logind oturumuna ait değil."));
                    }
                }
                self.session = Some(SessionInfo { conn_id, uid: peer.uid, desktop: p.desktop.clone(), id: p.logind_session_id.clone() });
                self.desktop_uid = Some(peer.uid);
                ok(format!("{} oturumu kaydedildi.", p.desktop), true)
            }
            Call::SessionEvent(p) => {
                if self.session.as_ref().is_none_or(|s| s.conn_id != conn_id) {
                    return Err(RpcError::new(code::UNAUTHORIZED, "Önce session_register gerekli."));
                }
                match p.event {
                    SessionEvent::GameStarted { class, .. } => {
                        self.classes.insert(class);
                    }
                    SessionEvent::GameStopped => self.classes.clear(),
                    SessionEvent::Idle { idle } => self.set_idle(idle),
                    SessionEvent::LidClosed { .. } => self.update_rgb_inhibit(),
                }
                Ok(json!({"accepted": true}))
            }
            other => Err(RpcError::new(code::METHOD_NOT_FOUND, format!("{} kontrol iş parçacığında işlenmez", other.name()))),
        }
    }

    fn fan_rpc(&mut self, f: FanSetting) -> Result<Value, RpcError> {
        f.validate().map_err(|e| RpcError::new(code::INVALID_PARAMS, e.0))?;
        if self.target_saved.is_some() {
            self.stop_target(false, "fan komutu");
        }
        self.apply_fan_setting(&f)?;
        self.remember(&ProfileSettings { fan: Some(f.clone()), ..Default::default() });
        match f {
            FanSetting::Auto => ok("Fanlar EC otomatiğinde; mod biti doğrulandı.", true),
            _ => ok("Fan hedefi uygulandı; PWM ve RPM doğrulaması kontrol döngüsünde sürüyor.", false),
        }
    }

    /// Atomic server-side scenarios (legacy GUI dropped every command after the first).
    fn scenario(&mut self, name: &str) -> Result<Value, RpcError> {
        let (preset, fan): (Option<&str>, FanSetting) = match name {
            "quiet" => (Some("quiet"), FanSetting::Auto),
            "balanced" => (Some("stock"), FanSetting::Auto),
            "game" => (Some("gaming_cool"), FanSetting::Preset { preset: "cool".into() }),
            "turbo" => {
                if self.power.source() == Source::Battery {
                    return Err(RpcError::new(code::BUSY, "Turbo yalnız prizde kullanılır."));
                }
                (None, FanSetting::Boost)
            }
            _ => return Err(RpcError::new(code::INVALID_PARAMS, "Senaryo: quiet, balanced, game, turbo.")),
        };
        let mut settings = match preset {
            Some(p) => presets::thermal_preset(p).expect("known preset"),
            None => ProfileSettings {
                power_profile: Some(PowerProfile::Performance),
                boost_enabled: Some(true),
                cpu_max_mhz: Some(0),
                gpu_max_mhz: Some(0),
                ..Default::default()
            },
        };
        settings.fan = Some(fan);
        let r = self.apply_profile(&settings)?;
        self.active_profile = Some(format!("senaryo:{name}"));
        Ok(r)
    }
}

/// Restores CPU ceilings left by an interrupted temperature target (before baseline).
fn recover_target_journal(cpu: &Cpufreq, state_dir: &Path) {
    let p = state_dir.join("temp-target-recovery.json");
    let Ok(bytes) = std::fs::read(&p) else { return };
    let Ok(map) = serde_json::from_slice::<serde_json::Map<String, Value>>(&bytes) else { return };
    let snapshot: Vec<(String, String)> = map
        .iter()
        .filter(|(k, v)| k.strip_prefix("policy").is_some_and(|n| n.parse::<u32>().is_ok()) && v.as_u64().is_some_and(|f| (1..=10_000_000).contains(&f)))
        .map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0).to_string()))
        .collect();
    if !snapshot.is_empty() && cpu.restore_max(&snapshot).is_ok() {
        let _ = std::fs::remove_file(p);
        eprintln!("r9td: kesilen sıcaklık hedefinin CPU sınırları geri yüklendi");
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    std::fs::rename(tmp, path)
}

/// logind session object path of `pid`, if logind answers.
fn logind_session_of(pid: u32) -> Option<String> {
    let conn = zbus::blocking::Connection::system().ok()?;
    let mgr = zbus::blocking::Proxy::new(&conn, "org.freedesktop.login1", "/org/freedesktop/login1", "org.freedesktop.login1.Manager").ok()?;
    let path: zbus::zvariant::OwnedObjectPath = mgr.call("GetSessionByPID", &(pid,)).ok()?;
    Some(path.to_string())
}

/// Internal FHD/IR camera USB authorisation (port of legacy `features.camera`).
fn camera(fs: &Sysfs, enabled: bool) -> Result<Value, RpcError> {
    const DEV: &str = "/sys/bus/usb/devices/5-1.4";
    let id = (fs.read(format!("{DEV}/idVendor")).ok(), fs.read(format!("{DEV}/idProduct")).ok());
    if id != (Some("2b7e".into()), Some("c906".into())) {
        return Err(RpcError::new(code::UNSUPPORTED, "Dahili FHD/IR kamera bulunamadı."));
    }
    let want = if enabled { "1" } else { "0" };
    if fs.read(format!("{DEV}/authorized")).ok().as_deref() == Some(want) {
        return ok("Kamera zaten seçilen durumda.", true);
    }
    if !enabled {
        // Refuse while a /dev/video node of this camera is open.
        let nodes: Vec<String> = fs
            .list("/sys/class/video4linux")
            .into_iter()
            .filter(|v| fs.resolve(format!("/sys/class/video4linux/{v}/device")).is_some_and(|p| p.to_string_lossy().contains("5-1.4")))
            .map(|v| format!("/dev/{v}"))
            .collect();
        for proc_ in fs.list("/proc").into_iter().filter(|p| p.bytes().all(|b| b.is_ascii_digit())) {
            for fd in fs.list(format!("/proc/{proc_}/fd")) {
                if let Ok(t) = std::fs::read_link(fs.path(format!("/proc/{proc_}/fd/{fd}"))) {
                    if nodes.iter().any(|n| t == Path::new(n)) {
                        return Err(RpcError::new(code::BUSY, "Kamera kullanımda; kamerayı kullanan uygulamayı kapatın."));
                    }
                }
            }
        }
    }
    fs.write(format!("{DEV}/authorized"), want).map_err(hal)?;
    std::thread::sleep(Duration::from_millis(300));
    if fs.read(format!("{DEV}/authorized")).ok().as_deref() != Some(want) {
        return Err(RpcError::new(code::HARDWARE, "Kamera durumu doğrulanamadı."));
    }
    ok(if enabled { "FHD/IR kamera etkinleştirildi." } else { "FHD/IR kamera kapatıldı." }, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hal_errors_map_to_rpc_codes() {
        assert_eq!(hal(HalError::Busy("x".into())).code, code::BUSY);
        assert_eq!(hal(HalError::Unsupported("x".into())).code, code::UNSUPPORTED);
        assert_eq!(hal(HalError::Invalid("x".into())).code, code::INVALID_PARAMS);
        assert_eq!(hal(HalError::Verify("x".into())).code, code::HARDWARE);
    }

    #[test]
    fn atomic_write_replaces() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.json");
        write_atomic(&p, b"1").unwrap();
        write_atomic(&p, b"2").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"2");
        assert!(!p.with_extension("tmp").exists());
    }
}
