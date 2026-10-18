//! CPU/GPU/power-profile writes with read-back and all-or-nothing profile rollback
//! (port of legacy `apply_profile`, docs/05 §2.7).

use std::path::PathBuf;

use r9t_hal::cpufreq::Cpufreq;
use r9t_hal::gpu::Gpu;
use r9t_hal::{HalError, Result};
use r9t_protocol::profile::{Epp, PowerProfile, ProfileSettings};

/// power-profiles-daemon over the system bus (`ActiveProfile` is writable).
pub struct Ppd {
    conn: Option<zbus::blocking::Connection>,
}

const PPD: [(&str, &str); 2] = [
    ("org.freedesktop.UPower.PowerProfiles", "/org/freedesktop/UPower/PowerProfiles"),
    ("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles"),
];

impl Ppd {
    pub fn new() -> Self {
        Self { conn: zbus::blocking::Connection::system().ok() }
    }

    fn proxy(&self) -> Result<zbus::blocking::Proxy<'_>> {
        let conn = self.conn.as_ref().ok_or_else(|| HalError::Unsupported("system D-Bus yok".into()))?;
        for (dest, path) in PPD {
            if let Ok(p) = zbus::blocking::Proxy::new(conn, dest, path, dest) {
                if p.get_property::<String>("ActiveProfile").is_ok() {
                    return Ok(p);
                }
            }
        }
        Err(HalError::Unsupported("power-profiles-daemon yok".into()))
    }

    pub fn get(&self) -> Result<String> {
        self.proxy()?.get_property::<String>("ActiveProfile").map_err(|e| HalError::Verify(format!("PPD: {e}")))
    }

    pub fn set(&self, profile: PowerProfile) -> Result<()> {
        let p = self.proxy()?;
        p.set_property("ActiveProfile", profile.as_str()).map_err(|e| HalError::Verify(format!("PPD: {e}")))?;
        if self.get()? != profile.as_str() {
            return Err(HalError::Verify("güç profili geri okunamadı".into()));
        }
        Ok(())
    }
}

impl Default for Ppd {
    fn default() -> Self {
        Self::new()
    }
}

/// GPU locked-clock ceiling with an ownership marker so a crash is undone at start.
pub struct GpuClocks<G: Gpu> {
    pub gpu: G,
    marker: PathBuf,
    pub requested: Option<u32>,
}

impl<G: Gpu> GpuClocks<G> {
    pub fn new(gpu: G, state_dir: &std::path::Path) -> Self {
        Self { gpu, marker: state_dir.join("gpu-clock-owned"), requested: None }
    }

    /// Resets clocks left locked by a previous run (marker present).
    pub fn recover(&mut self) -> Result<bool> {
        if self.marker.exists() {
            self.gpu.reset_locked_clocks()?;
            let _ = std::fs::remove_file(&self.marker);
            return Ok(true);
        }
        Ok(false)
    }

    /// `0` resets to automatic.
    pub fn set(&mut self, mhz: u32) -> Result<()> {
        if mhz == 0 {
            self.gpu.reset_locked_clocks()?;
            let _ = std::fs::remove_file(&self.marker);
            self.requested = None;
            return Ok(());
        }
        std::fs::write(&self.marker, mhz.to_string()).map_err(|e| HalError::io(&self.marker, e))?;
        match self.gpu.set_locked_clocks(mhz) {
            Ok(()) => {
                self.requested = Some(mhz);
                Ok(())
            }
            Err(e) => {
                if self.requested.is_none() {
                    let _ = std::fs::remove_file(&self.marker);
                }
                Err(e)
            }
        }
    }
}

/// Everything a profile may change, captured before applying it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PowerState {
    pub profile: Option<String>,
    pub boost: Option<bool>,
    pub cpu_max: Vec<(String, String)>,
    pub epp: Option<String>,
    pub gpu_mhz: Option<u32>,
}

pub fn capture<G: Gpu>(cpu: &Cpufreq, ppd: &Ppd, gpu: &GpuClocks<G>) -> PowerState {
    PowerState {
        profile: ppd.get().ok(),
        boost: cpu.boost().ok().flatten(),
        cpu_max: cpu.snapshot_max().unwrap_or_default(),
        epp: cpu.epp().ok().flatten(),
        gpu_mhz: gpu.requested,
    }
}

/// Restores a captured state; every step is attempted, failures are joined.
pub fn restore<G: Gpu>(cpu: &Cpufreq, ppd: &Ppd, gpu: &mut GpuClocks<G>, s: &PowerState) -> Result<()> {
    let mut errors = Vec::new();
    if let Some(p) = s.profile.as_deref().and_then(|p| serde_json::from_value::<PowerProfile>(p.into()).ok()) {
        if let Err(e) = ppd.set(p) {
            errors.push(e.to_string());
        }
    }
    if let Some(b) = s.boost {
        if let Err(e) = cpu.set_boost(b) {
            errors.push(e.to_string());
        }
    }
    if !s.cpu_max.is_empty() {
        if let Err(e) = cpu.restore_max(&s.cpu_max) {
            errors.push(e.to_string());
        }
    }
    if let Some(e) = s.epp.as_deref().and_then(Epp::parse) {
        if let Err(err) = cpu.set_epp(e) {
            errors.push(err.to_string());
        }
    }
    if gpu.requested != s.gpu_mhz {
        if let Err(e) = gpu.set(s.gpu_mhz.unwrap_or(0)) {
            errors.push(e.to_string());
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(HalError::Verify(errors.join("; "))) }
}

/// Applies the power part of a profile (fan and RGB are handled by their owners).
/// Order: power profile → boost → CPU ceiling → EPP → GPU ceiling. On any failure the
/// captured state is restored and the error says whether restoration was complete.
pub fn apply<G: Gpu>(cpu: &Cpufreq, ppd: &Ppd, gpu: &mut GpuClocks<G>, s: &ProfileSettings) -> Result<()> {
    let before = capture(cpu, ppd, gpu);
    let result: Result<()> = (|| {
        if let Some(p) = s.power_profile {
            ppd.set(p)?;
        }
        if let Some(b) = s.boost_enabled {
            cpu.set_boost(b)?;
        }
        if let Some(mhz) = s.cpu_max_mhz {
            cpu.set_max_mhz(mhz)?;
        }
        if let Some(e) = s.cpu_epp {
            cpu.set_epp(e)?;
        }
        if let Some(mhz) = s.gpu_max_mhz {
            gpu.set(mhz)?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        return Err(match restore(cpu, ppd, gpu, &before) {
            Ok(()) => HalError::Verify(format!("{e}; önceki ayarlar geri yüklendi")),
            Err(r) => HalError::Verify(format!("{e}; geri yükleme eksik: {r}")),
        });
    }
    Ok(())
}
