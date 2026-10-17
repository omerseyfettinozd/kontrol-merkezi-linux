//! Power supplies: AC, BAT0 (charge_* in µAh, no energy_* or power_now) and UCSI USB-C PD.

use crate::Sysfs;

const ROOT: &str = "/sys/class/power_supply";

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Battery {
    pub percent: Option<u8>,
    pub status: Option<String>,
    /// current × voltage; negative while discharging.
    pub power_w: Option<f32>,
    /// charge × voltage_min_design.
    pub energy_wh: Option<f32>,
    pub full_wh: Option<f32>,
    pub design_wh: Option<f32>,
    pub voltage_v: Option<f32>,
    /// Kernel exposes no charge threshold on this machine.
    pub charge_threshold_supported: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsbPd {
    pub name: String,
    pub online: Option<bool>,
    pub voltage_v: Option<f32>,
    pub current_a: Option<f32>,
}

/// Legacy classification (docs/05 §2.9): any Mains/USB online → AC; all definitely
/// offline → battery; anything unreadable → unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Ac,
    Battery,
    Unknown,
}

pub struct Power {
    fs: Sysfs,
}

impl Power {
    pub fn new(fs: Sysfs) -> Self {
        Self { fs }
    }

    fn supplies(&self) -> Vec<(String, String)> {
        self.fs
            .list(ROOT)
            .into_iter()
            .filter_map(|n| self.fs.read(format!("{ROOT}/{n}/type")).ok().map(|t| (n, t)))
            .collect()
    }

    pub fn source(&self) -> Source {
        let mut offline = false;
        for (name, kind) in self.supplies() {
            if kind != "Mains" && kind != "USB" {
                continue;
            }
            match self.fs.read(format!("{ROOT}/{name}/online")).as_deref() {
                Ok("1") => return Source::Ac,
                Ok("0") => offline = true,
                _ => return Source::Unknown,
            }
        }
        if offline { Source::Battery } else { Source::Unknown }
    }

    /// Mains adapter only (USB-C sources are reported separately).
    pub fn ac_online(&self) -> Option<bool> {
        self.supplies()
            .into_iter()
            .find(|(_, k)| k == "Mains")
            .and_then(|(n, _)| self.fs.read(format!("{ROOT}/{n}/online")).ok())
            .map(|v| v == "1")
    }

    pub fn battery(&self) -> Option<Battery> {
        let (name, _) = self.supplies().into_iter().find(|(_, k)| k == "Battery")?;
        let base = format!("{ROOT}/{name}");
        let num = |f: &str| self.fs.parse::<f64>(format!("{base}/{f}")).ok();
        let status = self.fs.read(format!("{base}/status")).ok();
        let v_now = num("voltage_now").map(|v| v / 1e6);
        let v_design = num("voltage_min_design").map(|v| v / 1e6);
        let wh = |f: &str| Some((num(f)? / 1e6 * v_design?) as f32);
        let power_w = match (num("current_now"), v_now) {
            (Some(i), Some(v)) => {
                let w = (i / 1e6 * v) as f32;
                Some(if status.as_deref() == Some("Discharging") { -w.abs() } else { w.abs() })
            }
            _ => None,
        };
        Some(Battery {
            percent: num("capacity").and_then(|c| u8::try_from(c as i64).ok()).filter(|c| *c <= 100),
            status,
            power_w,
            energy_wh: wh("charge_now"),
            full_wh: wh("charge_full"),
            design_wh: wh("charge_full_design"),
            voltage_v: v_now.map(|v| v as f32),
            charge_threshold_supported: self.fs.exists(format!("{base}/charge_control_end_threshold")),
        })
    }

    pub fn usb_pd(&self) -> Vec<UsbPd> {
        self.supplies()
            .into_iter()
            .filter(|(n, k)| k == "USB" && n.starts_with("ucsi-"))
            .map(|(n, _)| {
                let base = format!("{ROOT}/{n}");
                let f = |x: &str| self.fs.parse::<f64>(format!("{base}/{x}")).ok();
                UsbPd {
                    online: self.fs.read(format!("{base}/online")).ok().map(|v| v == "1"),
                    voltage_v: f("voltage_now").map(|v| (v / 1e6) as f32),
                    current_a: f("current_now").map(|v| (v / 1e6) as f32),
                    name: n,
                }
            })
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::sysfs::fake::file;
    use std::path::Path;

    pub fn fake_power(root: &Path, ac: bool, status: &str) {
        file(root, &format!("{ROOT}/AC0/type"), "Mains");
        file(root, &format!("{ROOT}/AC0/online"), if ac { "1" } else { "0" });
        let b = format!("{ROOT}/BAT0");
        for (f, v) in [
            ("type", "Battery"), ("status", status), ("capacity", "80"), ("charge_now", "4160000"),
            ("charge_full", "5200000"), ("charge_full_design", "5200000"), ("current_now", "1500000"),
            ("voltage_now", "16000000"), ("voltage_min_design", "15400000"),
        ] {
            file(root, &format!("{b}/{f}"), v);
        }
        let u = format!("{ROOT}/ucsi-source-psy-USBC000:001");
        for (f, v) in [("type", "USB"), ("online", "0"), ("voltage_now", "5000000"), ("current_now", "0")] {
            file(root, &format!("{u}/{f}"), v);
        }
    }

    #[test]
    fn battery_math_and_sign() {
        let dir = tempfile::tempdir().unwrap();
        fake_power(dir.path(), false, "Discharging");
        let p = Power::new(Sysfs::new(dir.path()));
        let b = p.battery().unwrap();
        assert_eq!(b.percent, Some(80));
        assert!((b.power_w.unwrap() + 24.0).abs() < 1e-3);
        assert!((b.full_wh.unwrap() - 80.08).abs() < 1e-3);
        assert!(!b.charge_threshold_supported);
        assert_eq!(p.source(), Source::Battery);
        assert_eq!(p.ac_online(), Some(false));
        assert_eq!(p.usb_pd().len(), 1);
    }

    #[test]
    fn source_classification() {
        let dir = tempfile::tempdir().unwrap();
        fake_power(dir.path(), true, "Charging");
        let p = Power::new(Sysfs::new(dir.path()));
        assert_eq!(p.source(), Source::Ac);
        assert!(p.battery().unwrap().power_w.unwrap() > 0.0);
        file(dir.path(), &format!("{ROOT}/AC0/online"), "0");
        file(dir.path(), &format!("{ROOT}/ucsi-source-psy-USBC000:001/online"), "1");
        assert_eq!(p.source(), Source::Ac, "USB-C PD counts as AC");
        file(dir.path(), &format!("{ROOT}/ucsi-source-psy-USBC000:001/online"), "?");
        assert_eq!(p.source(), Source::Unknown, "ambiguous is never battery");
    }
}
