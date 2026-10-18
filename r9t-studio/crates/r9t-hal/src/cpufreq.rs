//! amd-pstate-epp CPU limits, boost and EPP, with all-policy rollback (port of
//! legacy `cooling.py` / `hardware.set_boost`).

use r9t_protocol::profile::Epp;

use crate::{HalError, Result, Sysfs};

const ROOT: &str = "/sys/devices/system/cpu/cpufreq";

pub struct Cpufreq {
    fs: Sysfs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuLimits {
    /// Current `scaling_max_freq` when equal on all policies.
    pub max_mhz: Option<u32>,
    pub allowed_min_mhz: u32,
    pub allowed_max_mhz: u32,
    pub policies: usize,
}

impl Cpufreq {
    pub fn new(fs: Sysfs) -> Self {
        Self { fs }
    }

    pub fn policies(&self) -> Vec<String> {
        let mut p: Vec<String> = self.fs.list(ROOT).into_iter().filter(|n| n.starts_with("policy")).collect();
        p.sort_by_key(|n| n[6..].parse::<u32>().unwrap_or(u32::MAX));
        p
    }

    fn file(policy: &str, name: &str) -> String {
        format!("{ROOT}/{policy}/{name}")
    }

    fn require_epp_driver(&self) -> Result<Vec<String>> {
        let policies = self.policies();
        if policies.is_empty() {
            return Err(HalError::Unsupported("cpufreq politikası yok".into()));
        }
        for p in &policies {
            if self.fs.read(Self::file(p, "scaling_driver"))? != "amd-pstate-epp" {
                return Err(HalError::Unsupported("amd-pstate-epp sürücüsü yok".into()));
            }
        }
        Ok(policies)
    }

    pub fn limits(&self) -> Result<CpuLimits> {
        let policies = self.require_epp_driver()?;
        let mut maxima = Vec::new();
        let mut hw_max = u32::MAX;
        let mut min_floor = 0u32;
        for p in &policies {
            maxima.push(self.fs.parse::<u32>(Self::file(p, "scaling_max_freq"))? / 1000);
            hw_max = hw_max.min(self.fs.parse::<u32>(Self::file(p, "amd_pstate_max_freq"))? / 1000);
            min_floor = min_floor.max(self.fs.parse::<u32>(Self::file(p, "scaling_min_freq"))? / 1000 + 1);
        }
        maxima.dedup();
        Ok(CpuLimits {
            max_mhz: (maxima.len() == 1).then(|| maxima[0]),
            allowed_min_mhz: min_floor.max(1000),
            allowed_max_mhz: hw_max,
            policies: policies.len(),
        })
    }

    pub fn snapshot_max(&self) -> Result<Vec<(String, String)>> {
        self.policies().into_iter().map(|p| Ok((p.clone(), self.fs.read(Self::file(&p, "scaling_max_freq"))?))).collect()
    }

    pub fn restore_max(&self, snapshot: &[(String, String)]) -> Result<()> {
        let mut errors = Vec::new();
        for (p, v) in snapshot {
            let f = Self::file(p, "scaling_max_freq");
            if let Err(e) = self.fs.write(&f, v).and_then(|_| {
                if self.fs.read(&f)? == *v { Ok(()) } else { Err(HalError::Verify(format!("{p} scaling_max_freq"))) }
            }) {
                errors.push(e.to_string());
            }
        }
        if errors.is_empty() { Ok(()) } else { Err(HalError::Verify(errors.join("; "))) }
    }

    /// `mhz == 0` restores the driver ceiling. With boost off the driver clamps to the
    /// non-boost ceiling, which the read-back check accounts for.
    pub fn set_max_mhz(&self, mhz: u32) -> Result<()> {
        let limits = self.limits()?;
        if mhz != 0 && !(limits.allowed_min_mhz..=limits.allowed_max_mhz).contains(&mhz) {
            return Err(HalError::Invalid(format!(
                "CPU sınırı {}–{} MHz ya da 0 olmalı",
                limits.allowed_min_mhz, limits.allowed_max_mhz
            )));
        }
        let snapshot = self.snapshot_max()?;
        let result = (|| {
            for p in self.policies() {
                let cpuinfo_max: u32 = self.fs.parse(Self::file(&p, "cpuinfo_max_freq"))?;
                let desired = if mhz == 0 { cpuinfo_max } else { mhz * 1000 };
                let f = Self::file(&p, "scaling_max_freq");
                self.fs.write(&f, &desired.to_string())?;
                let expected = desired.min(self.fs.parse(Self::file(&p, "cpuinfo_max_freq"))?);
                self.settle(&f, &desired.to_string(), expected)
                    .map_err(|actual| HalError::Verify(format!("{p}: scaling_max_freq {actual} ≠ {expected}")))?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            return Err(match self.restore_max(&snapshot) {
                Ok(()) => e,
                Err(r) => HalError::Verify(format!("{e}; geri yükleme eksik: {r}")),
            });
        }
        Ok(())
    }

    /// Read-back with a short grace period: under load amd-pstate (or nvidia-powerd)
    /// occasionally reports the previous ceiling for a few ms (measured: 1 of 16
    /// policies). Re-read for up to 100 ms, then write once more and re-read.
    fn settle(&self, file: &str, value: &str, expected: u32) -> std::result::Result<(), u32> {
        let mut actual = 0;
        for attempt in 0..2 {
            for _ in 0..5 {
                actual = self.fs.parse::<u32>(file).unwrap_or(0);
                if actual == expected {
                    return Ok(());
                }
                if self.fs.is_real() {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            }
            if attempt == 0 {
                let _ = self.fs.write(file, value);
            }
        }
        Err(actual)
    }

    /// `Some(true)` only when every policy agrees.
    pub fn boost(&self) -> Result<Option<bool>> {
        let mut values = Vec::new();
        for p in self.policies() {
            values.push(self.fs.read(Self::file(&p, "boost"))?);
        }
        values.dedup();
        Ok(match values.as_slice() {
            [v] if v == "1" => Some(true),
            [v] if v == "0" => Some(false),
            _ => None,
        })
    }

    pub fn set_boost(&self, enabled: bool) -> Result<()> {
        let policies = self.require_epp_driver()?;
        let want = if enabled { "1" } else { "0" };
        let mut previous = Vec::new();
        let result = (|| {
            for p in &policies {
                let f = Self::file(p, "boost");
                previous.push((f.clone(), self.fs.read(&f)?));
                self.fs.write(&f, want)?;
                if self.fs.read(&f)? != want {
                    return Err(HalError::Verify(format!("{p} boost")));
                }
            }
            Ok(())
        })();
        if result.is_err() {
            for (f, v) in &previous {
                let _ = self.fs.write(f, v);
            }
        }
        result
    }

    pub fn epp(&self) -> Result<Option<String>> {
        let mut values = Vec::new();
        for p in self.policies() {
            values.push(self.fs.read(Self::file(&p, "energy_performance_preference"))?);
        }
        values.dedup();
        Ok((values.len() == 1).then(|| values.remove(0)))
    }

    pub fn epp_choices(&self) -> Result<Vec<Epp>> {
        let policies = self.require_epp_driver()?;
        let mut common: Option<Vec<Epp>> = None;
        for p in &policies {
            let here: Vec<Epp> = self
                .fs
                .read(Self::file(p, "energy_performance_available_preferences"))?
                .split_whitespace()
                .filter_map(Epp::parse)
                .collect();
            common = Some(match common {
                None => here,
                Some(c) => c.into_iter().filter(|e| here.contains(e)).collect(),
            });
        }
        Ok(common.unwrap_or_default())
    }

    pub fn set_epp(&self, epp: Epp) -> Result<()> {
        if !self.epp_choices()?.contains(&epp) {
            return Err(HalError::Unsupported(format!("EPP {}", epp.as_str())));
        }
        let snapshot: Vec<(String, String)> = self
            .policies()
            .into_iter()
            .map(|p| {
                let f = Self::file(&p, "energy_performance_preference");
                Ok((f.clone(), self.fs.read(&f)?))
            })
            .collect::<Result<_>>()?;
        let result = (|| {
            for (f, _) in &snapshot {
                self.fs.write(f, epp.as_str())?;
            }
            if self.epp()?.as_deref() != Some(epp.as_str()) {
                return Err(HalError::Verify("EPP tüm politikalarda doğrulanamadı".into()));
            }
            Ok(())
        })();
        if result.is_err() {
            for (f, v) in &snapshot {
                let _ = self.fs.write(f, v);
            }
        }
        result
    }

    /// Current frequency average across policies (MHz).
    pub fn current_mhz(&self) -> Option<u32> {
        let v: Vec<u32> = self
            .policies()
            .iter()
            .filter_map(|p| self.fs.parse::<u32>(Self::file(p, "scaling_cur_freq")).ok())
            .collect();
        (!v.is_empty()).then(|| (v.iter().map(|&x| x as u64).sum::<u64>() / v.len() as u64 / 1000) as u32)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::sysfs::fake::file;
    use std::path::Path;

    pub fn fake_cpus(root: &Path, n: usize, boost: bool) {
        for i in 0..n {
            let d = format!("{ROOT}/policy{i}");
            file(root, &format!("{d}/scaling_driver"), "amd-pstate-epp");
            file(root, &format!("{d}/amd_pstate_max_freq"), "5386028");
            file(root, &format!("{d}/cpuinfo_max_freq"), if boost { "5386028" } else { "2401000" });
            file(root, &format!("{d}/scaling_min_freq"), "421798");
            file(root, &format!("{d}/scaling_max_freq"), "2401000");
            file(root, &format!("{d}/scaling_cur_freq"), "1500000");
            file(root, &format!("{d}/boost"), if boost { "1" } else { "0" });
            file(root, &format!("{d}/energy_performance_preference"), "power");
            file(root, &format!("{d}/energy_performance_available_preferences"), "default performance balance_performance balance_power power custom");
        }
    }

    #[test]
    fn limits_and_set_max() {
        let dir = tempfile::tempdir().unwrap();
        fake_cpus(dir.path(), 4, true);
        let c = Cpufreq::new(Sysfs::new(dir.path()));
        let l = c.limits().unwrap();
        assert_eq!((l.max_mhz, l.allowed_min_mhz, l.allowed_max_mhz, l.policies), (Some(2401), 1000, 5386, 4));
        c.set_max_mhz(3500).unwrap();
        assert_eq!(c.limits().unwrap().max_mhz, Some(3500));
        c.set_max_mhz(0).unwrap();
        assert_eq!(c.limits().unwrap().max_mhz, Some(5386));
        assert!(c.set_max_mhz(900).is_err());
        assert!(c.set_max_mhz(6000).is_err());
        assert_eq!(c.current_mhz(), Some(1500));
    }

    #[test]
    fn boost_and_epp_with_rollback() {
        let dir = tempfile::tempdir().unwrap();
        fake_cpus(dir.path(), 3, false);
        let c = Cpufreq::new(Sysfs::new(dir.path()));
        assert_eq!(c.boost().unwrap(), Some(false));
        c.set_boost(true).unwrap();
        assert_eq!(c.boost().unwrap(), Some(true));
        c.set_epp(Epp::BalancePerformance).unwrap();
        assert_eq!(c.epp().unwrap().as_deref(), Some("balance_performance"));
        // Third policy lacks a preference: the common set shrinks and EPP is refused.
        file(dir.path(), &format!("{ROOT}/policy2/energy_performance_available_preferences"), "performance power");
        assert!(c.set_epp(Epp::BalancePower).is_err());
        assert_eq!(c.epp().unwrap().as_deref(), Some("balance_performance"));
    }

    #[test]
    fn wrong_driver_is_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        fake_cpus(dir.path(), 2, true);
        file(dir.path(), &format!("{ROOT}/policy1/scaling_driver"), "acpi-cpufreq");
        let c = Cpufreq::new(Sysfs::new(dir.path()));
        assert!(matches!(c.limits(), Err(HalError::Unsupported(_))));
        assert!(c.set_boost(false).is_err());
    }
}
