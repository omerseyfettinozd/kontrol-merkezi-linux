//! One-shot capability probe used by `r9tctl probe` and r9td at start-up.
//! Never writes. Root-only sources are reported as `unavailable: permission`.

use serde::Serialize;
use serde_json::{json, Value};

use r9t_protocol::capability::{Capability, CapabilityId};

use crate::cpufreq::Cpufreq;
use crate::fan::{FanBridge, SysfsFan};
use crate::gpu::{Gpu, Nvml};
use crate::hwmon::Hwmon;
use crate::power::Power;
use crate::rapl::PackagePower;
use crate::rgb::Rgb;
use crate::tuxedo::{RealDevice, Unverified};
use crate::{HalError, Sysfs};

#[derive(Debug, Serialize)]
pub struct ProbeReport {
    pub device: Value,
    pub capabilities: Vec<Capability>,
    pub readings: Value,
}

fn unavailable(e: &HalError) -> Value {
    if e.is_permission() {
        Value::from("unavailable: permission")
    } else {
        Value::from(format!("unavailable: {e}"))
    }
}

fn val<T: Serialize>(r: crate::Result<T>) -> Value {
    match r {
        Ok(v) => serde_json::to_value(v).unwrap_or(Value::Null),
        Err(e) => unavailable(&e),
    }
}

/// `with_nvml = false` skips NVML entirely (e.g. when the caller must not risk a wake).
pub fn probe(fs: &Sysfs, with_nvml: bool) -> ProbeReport {
    probe_with(fs, with_nvml, false)
}

/// Daemon start-up variant: no EC, RAPL or NVML reads (the sampler is the only
/// reader and a full probe costs ~1.5 s); availability comes from file presence.
pub fn probe_light(fs: &Sysfs) -> ProbeReport {
    probe_with(fs, false, true)
}

fn probe_with(fs: &Sysfs, with_nvml: bool, light: bool) -> ProbeReport {
    let mut caps = Vec::new();
    let mut r = serde_json::Map::new();
    let dmi = |f: &str| fs.read(format!("/sys/class/dmi/id/{f}")).ok();
    let device = json!({
        "vendor": dmi("sys_vendor"), "product": dmi("product_name"), "bios": dmi("bios_version"),
        "kernel": fs.read("/proc/sys/kernel/osrelease").ok(),
    });

    // Fan bridge (status is 0400 root).
    let fan = SysfsFan::new(fs.clone());
    let fan_status = if light {
        if fs.exists("/sys/module/r9t_fan/parameters/status") {
            Err(HalError::Unsupported("hafif yoklama: status örnekleyicide okunur".into()))
        } else {
            Err(HalError::NotFound("/sys/module/r9t_fan".into()))
        }
    } else {
        fan.status()
    };
    let module = json!({"loaded": fan.loaded(), "version": fan.version(), "srcversion": fan.srcversion()});
    let duty_floor = fan.duty_floor();
    match &fan_status {
        Ok(s) => {
            caps.push(Capability::yes(CapabilityId::FanRead));
            let version = fan.version();
            caps.push(if crate::fan::supported_version(version.as_deref()) {
                Capability::yes(CapabilityId::FanControl)
            } else {
                Capability::no(CapabilityId::FanControl, crate::fan::version_refusal(version.as_deref()))
            });
            r.insert("fan".into(), json!({
                "module": module, "mode": s.mode, "auto": s.is_auto(), "cpu_rpm": s.cpu_rpm, "gpu_rpm": s.gpu_rpm,
                "cpu_duty_pct": s.cpu_pwm / 2, "gpu_duty_pct": s.gpu_pwm / 2,
                "ec_temps": s.valid_temps(), "lease_ms": s.lease_ms, "error": s.error, "thermal": s.thermal,
                "duty_floor": duty_floor,
            }));
        }
        Err(HalError::Unsupported(_)) if light => {
            caps.push(Capability::yes(CapabilityId::FanRead));
            let version = fan.version();
            caps.push(if crate::fan::supported_version(version.as_deref()) {
                Capability::yes(CapabilityId::FanControl)
            } else {
                Capability::no(CapabilityId::FanControl, crate::fan::version_refusal(version.as_deref()))
            });
            r.insert("fan".into(), json!({"module": module, "duty_floor": duty_floor}));
        }
        Err(e) => {
            caps.push(Capability::no(CapabilityId::FanRead, e.to_string()));
            caps.push(Capability::no(CapabilityId::FanControl, e.to_string()));
            r.insert("fan".into(), json!({"module": module, "status": unavailable(e)}));
        }
    }

    // tuxedo_io fast path: hwcheck first, enforced by the type system.
    let ec = if light {
        Err(if fs.exists("/dev/tuxedo_io") { HalError::Unsupported("hafif yoklama".into()) } else { HalError::NotFound("/dev/tuxedo_io".into()) })
    } else {
        RealDevice::open(fs, false).and_then(|d| Unverified::new(d).verify())
    }
    .and_then(|mut v| {
        let sample = v.sample()?;
        Ok(json!({"model_id": format!("0x{:02x}", v.model_id), "interface": v.interface,
                  "cpu_temp": sample.cpu_temp, "gpu_temp": sample.gpu_temp,
                  "cpu_duty": sample.cpu_duty, "gpu_duty": sample.gpu_duty, "mode": sample.mode}))
    });
    caps.push(match &ec {
        Ok(_) => Capability::yes(CapabilityId::EcFastPath),
        Err(HalError::Unsupported(_)) if light => Capability::yes(CapabilityId::EcFastPath),
        Err(e) => Capability::no(CapabilityId::EcFastPath, e.to_string()),
    });
    r.insert("ec".into(), val(ec));

    // CPU.
    let cpu = Cpufreq::new(fs.clone());
    let limits = cpu.limits();
    caps.push(match &limits {
        Ok(_) => Capability::yes(CapabilityId::CpuLimit),
        Err(e) => Capability::no(CapabilityId::CpuLimit, e.to_string()),
    });
    let epp = cpu.epp_choices();
    caps.push(match &epp {
        Ok(c) if !c.is_empty() => Capability::yes(CapabilityId::CpuEpp),
        Ok(_) => Capability::no(CapabilityId::CpuEpp, "ortak EPP seçeneği yok"),
        Err(e) => Capability::no(CapabilityId::CpuEpp, e.to_string()),
    });
    let boost = cpu.boost();
    caps.push(match &boost {
        Ok(Some(_)) => Capability::yes(CapabilityId::CpuBoost),
        Ok(None) => Capability::no(CapabilityId::CpuBoost, "politikalar tutarsız"),
        Err(e) => Capability::no(CapabilityId::CpuBoost, e.to_string()),
    });
    r.insert("cpu".into(), json!({
        "limits": val(limits.map(|l| json!({"max_mhz": l.max_mhz, "allowed": [l.allowed_min_mhz, l.allowed_max_mhz], "policies": l.policies}))),
        "boost": val(boost), "epp": val(cpu.epp()), "epp_choices": val(epp.map(|c| c.iter().map(|e| e.as_str()).collect::<Vec<_>>())),
        "freq_mhz": cpu.current_mhz(),
    }));
    caps.push(Capability::no(CapabilityId::CpuPpt, "ryzenadj: Dragon Range güç tablosu desteklenmiyor (docs/06)"));

    // Package power needs two samples; a permission error is reported as such.
    let mut rapl = PackagePower::new(fs.clone());
    let now = std::time::Instant::now();
    let rapl_r = if light {
        Ok(fs.list("/sys/class/powercap").iter().any(|z| fs.read(format!("/sys/class/powercap/{z}/name")).is_ok_and(|n| n.starts_with("package-"))).then_some(0.0))
    } else {
        rapl.sample(now, 5.0).and_then(|_| {
            std::thread::sleep(std::time::Duration::from_millis(250));
            rapl.sample(std::time::Instant::now(), 5.0)
        })
    };
    caps.push(match &rapl_r {
        Ok(Some(_)) => Capability::yes(CapabilityId::PackagePower),
        Ok(None) => Capability::no(CapabilityId::PackagePower, "RAPL package bölgesi yok"),
        Err(e) => Capability::no(CapabilityId::PackagePower, e.to_string()),
    });
    r.insert("package_w".into(), val(rapl_r));

    // Power profile is a D-Bus service; presence is checked by r9td, here only noted.
    caps.push(Capability::yes(CapabilityId::PowerProfile));

    // Sensors.
    let hw = Hwmon::new(fs.clone());
    let temps: Vec<Value> = hw.discover().iter().map(|t| json!({"id": t.id, "c": hw.read_c(t)})).collect();
    r.insert("sensors".into(), Value::Array(temps));
    r.insert("apu_ppt_w".into(), json!(hw.apu_ppt_w()));

    // Power supplies.
    let pw = Power::new(fs.clone());
    let bat = pw.battery();
    caps.push(if bat.is_some() { Capability::yes(CapabilityId::Battery) } else { Capability::no(CapabilityId::Battery, "BAT0 yok") });
    caps.push(match &bat {
        Some(b) if b.charge_threshold_supported => Capability::yes(CapabilityId::ChargeLimit),
        _ => Capability::no(CapabilityId::ChargeLimit, "BAT0 charge_control_end_threshold yok"),
    });
    r.insert("power".into(), json!({
        "source": format!("{:?}", pw.source()), "ac_online": pw.ac_online(),
        "battery": bat.map(|b| json!({"percent": b.percent, "status": b.status, "power_w": b.power_w, "energy_wh": b.energy_wh,
                                       "full_wh": b.full_wh, "design_wh": b.design_wh, "voltage_v": b.voltage_v})),
        "usb_pd": pw.usb_pd().iter().map(|u| json!({"name": u.name, "online": u.online, "voltage_v": u.voltage_v, "current_a": u.current_a})).collect::<Vec<_>>(),
    }));

    // RGB.
    match Rgb::discover(fs.clone()) {
        Ok(rgb) => {
            caps.push(Capability::yes(CapabilityId::Rgb));
            caps.push(if rgb.buffered() {
                Capability::yes(CapabilityId::RgbBuffered)
            } else {
                Capability::no(CapabilityId::RgbBuffered, "buffer_input yok")
            });
            r.insert("rgb".into(), json!({"brightness": val(rgb.brightness()), "max": rgb.max_brightness(), "buffered": rgb.buffered()}));
        }
        Err(e) => {
            caps.push(Capability::no(CapabilityId::Rgb, e.to_string()));
            caps.push(Capability::no(CapabilityId::RgbBuffered, e.to_string()));
        }
    }

    // Fn-Lock and camera presence.
    let fn_lock = fs.read("/sys/devices/platform/tuxedo_keyboard/fn_lock");
    caps.push(match &fn_lock {
        Ok(v) if v == "0" || v == "1" => Capability::yes(CapabilityId::FnLock),
        Ok(v) => Capability::no(CapabilityId::FnLock, format!("değer {v}")),
        Err(e) => Capability::no(CapabilityId::FnLock, e.to_string()),
    });
    r.insert("fn_lock".into(), val(fn_lock));
    let cam = fs.read(format!("{}/product", crate::CAMERA_USB)).ok();
    caps.push(if cam.as_deref() == Some("FHD WebCam") {
        Capability::yes(CapabilityId::Camera)
    } else {
        Capability::no(CapabilityId::Camera, "dahili FHD/IR kamera bulunamadı")
    });

    // GPU: runtime status always; NVML only if requested and active.
    let gpu = Nvml::new(fs.clone());
    let status = gpu.runtime_status();
    let sample = if with_nvml { gpu.sample() } else { Err(HalError::Unsupported("--no-nvml".into())) };
    caps.push(match &sample {
        Ok(_) => Capability::yes(CapabilityId::GpuTelemetry),
        Err(_) if light && status.is_ok() => Capability::yes(CapabilityId::GpuTelemetry).with_reason("yalnız runtime_status == active iken"),
        Err(e) => Capability::no(CapabilityId::GpuTelemetry, e.to_string()),
    });
    caps.push(Capability::yes(CapabilityId::GpuLockedClocks).with_reason("yazma root ister; bağımsız geri okuma yok"));
    r.insert("gpu".into(), json!({
        "runtime_status": val(status), "nvml_calls": gpu.nvml_calls(),
        "sample": val(sample.map(|s| json!({"temp_c": s.temp_c, "power_w": s.power_w, "enforced_limit_w": s.enforced_limit_w,
            "clock_graphics_mhz": s.clock_graphics_mhz, "max_clock_mhz": s.max_clock_mhz, "util_gpu_pct": s.util_gpu_pct,
            "vram_used_mib": s.vram_used_mib, "vram_total_mib": s.vram_total_mib, "pstate": s.pstate}))),
    }));

    caps.sort_by_key(|c| c.id);
    ProbeReport { device, capabilities: caps, readings: Value::Object(r) }
}

trait WithReason {
    fn with_reason(self, reason: &str) -> Self;
}

impl WithReason for Capability {
    fn with_reason(mut self, reason: &str) -> Self {
        self.reason = reason.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sysfs::fake::file;

    #[test]
    fn probe_fake_machine_without_root_sources() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        crate::cpufreq::tests::fake_cpus(root, 2, true);
        crate::hwmon::tests::fake_hwmon(root);
        crate::power::tests::fake_power(root, true, "Full");
        crate::rgb::tests::fake_leds(root, true);
        file(root, "/sys/class/dmi/id/product_name", "SLAYER R9T");
        file(root, "/sys/bus/pci/devices/0000:01:00.0/power/runtime_status", "suspended");
        let report = probe(&Sysfs::new(root), true);
        let cap = |id| report.capabilities.iter().find(|c| c.id == id).unwrap().available;
        assert!(cap(CapabilityId::CpuLimit) && cap(CapabilityId::Rgb) && cap(CapabilityId::RgbBuffered));
        assert!(!cap(CapabilityId::FanRead) && !cap(CapabilityId::EcFastPath) && !cap(CapabilityId::ChargeLimit));
        assert!(!cap(CapabilityId::CpuPpt));
        assert!(!cap(CapabilityId::GpuTelemetry), "suspended GPU is not sampled");
        assert_eq!(report.readings["gpu"]["nvml_calls"], 0);
        assert_eq!(report.readings["device"], Value::Null);
        assert_eq!(report.device["product"], "SLAYER R9T");
    }
}
