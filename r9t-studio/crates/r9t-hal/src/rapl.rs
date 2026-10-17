//! CPU package power from RAPL `package-N` energy counters (root-only, 0400).
//! Sub-zones (`core`) are never added to the package total.

use std::collections::HashMap;
use std::time::Instant;

use crate::{Result, Sysfs};

const ROOT: &str = "/sys/class/powercap";

pub struct PackagePower {
    fs: Sysfs,
    previous: HashMap<String, (u64, Instant)>,
}

impl PackagePower {
    pub fn new(fs: Sysfs) -> Self {
        Self { fs, previous: HashMap::new() }
    }

    fn zones(&self) -> Vec<String> {
        self.fs
            .list(ROOT)
            .into_iter()
            .filter(|z| {
                self.fs
                    .read(format!("{ROOT}/{z}/name"))
                    .is_ok_and(|n| n.strip_prefix("package-").is_some_and(|i| i.parse::<u32>().is_ok()))
            })
            .collect()
    }

    /// Watts since the previous call; `Ok(None)` on the first sample, after a gap
    /// longer than `max_gap_s` (suspend) or when a package is missing.
    pub fn sample(&mut self, now: Instant, max_gap_s: f64) -> Result<Option<f32>> {
        let zones = self.zones();
        if zones.is_empty() {
            return Ok(None);
        }
        let mut total = 0.0f64;
        let mut complete = true;
        for z in zones {
            let energy: u64 = self.fs.parse(format!("{ROOT}/{z}/energy_uj"))?;
            let range: u64 = self.fs.parse(format!("{ROOT}/{z}/max_energy_range_uj")).unwrap_or(u64::MAX);
            match self.previous.insert(z, (energy, now)) {
                Some((prev, t)) => {
                    let dt = now.duration_since(t).as_secs_f64();
                    if dt <= 0.0 || dt > max_gap_s {
                        complete = false;
                        continue;
                    }
                    let delta = if energy >= prev { energy - prev } else { range.saturating_sub(prev) + energy };
                    total += delta as f64 / 1e6 / dt;
                }
                None => complete = false,
            }
        }
        Ok(complete.then_some(total as f32).filter(|w| (0.0..=400.0).contains(w)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sysfs::fake::file;
    use std::time::Duration;

    #[test]
    fn delta_wrap_gap_and_subzones() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        file(r, &format!("{ROOT}/intel-rapl:0/name"), "package-0");
        file(r, &format!("{ROOT}/intel-rapl:0/energy_uj"), "1000000");
        file(r, &format!("{ROOT}/intel-rapl:0/max_energy_range_uj"), "65532610987");
        file(r, &format!("{ROOT}/intel-rapl:0:0/name"), "core");
        file(r, &format!("{ROOT}/intel-rapl:0:0/energy_uj"), "999999999");
        let mut p = PackagePower::new(Sysfs::new(r));
        let t0 = Instant::now();
        assert_eq!(p.sample(t0, 5.0).unwrap(), None);
        file(r, &format!("{ROOT}/intel-rapl:0/energy_uj"), "31000000");
        assert_eq!(p.sample(t0 + Duration::from_secs(1), 5.0).unwrap(), Some(30.0));
        // Counter wrap at max_energy_range_uj: 65532610987 → 1000000 is 1.61 J in 1 s.
        file(r, &format!("{ROOT}/intel-rapl:0/energy_uj"), "65532000000");
        p.sample(t0 + Duration::from_secs(2), 5.0).unwrap();
        file(r, &format!("{ROOT}/intel-rapl:0/energy_uj"), "1000000");
        let w = p.sample(t0 + Duration::from_secs(3), 5.0).unwrap().unwrap();
        assert!((w - 1.610987).abs() < 1e-3, "{w}");
        // An implausible jump (counter reset to a small value mid-range) is rejected.
        file(r, &format!("{ROOT}/intel-rapl:0/energy_uj"), "500");
        assert_eq!(p.sample(t0 + Duration::from_secs(4), 5.0).unwrap(), None);
        file(r, &format!("{ROOT}/intel-rapl:0/energy_uj"), "30000500");
        // Long gap (suspend) is a hole, not an average.
        assert_eq!(p.sample(t0 + Duration::from_secs(60), 5.0).unwrap(), None);
    }
}
