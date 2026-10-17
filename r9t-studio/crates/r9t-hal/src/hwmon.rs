//! hwmon temperatures matched by chip name, device and label — never by hwmon index.

use crate::Sysfs;

#[derive(Debug, Clone, PartialEq)]
pub struct HwmonTemp {
    /// Stable id `chip:device:label`, e.g. `nvme:0000:02:00.0:Composite`.
    pub id: String,
    pub chip: String,
    pub device: String,
    pub label: String,
    /// sysfs path of the `tempN_input` file (absolute, below the Sysfs root).
    pub input: String,
}

pub struct Hwmon {
    fs: Sysfs,
}

impl Hwmon {
    pub fn new(fs: Sysfs) -> Self {
        Self { fs }
    }

    /// Every labelled (or numbered) temperature input on the system.
    pub fn discover(&self) -> Vec<HwmonTemp> {
        let mut out = Vec::new();
        for h in self.fs.list("/sys/class/hwmon") {
            let base = format!("/sys/class/hwmon/{h}");
            let Ok(chip) = self.fs.read(format!("{base}/name")) else { continue };
            // nvme/spd5118 hang off a controller or i2c address; use the PCI/I2C id
            // closest to the chip so two identical drives get distinct ids.
            let device = self
                .fs
                .resolve(format!("{base}/device"))
                .map(|p| {
                    let comps: Vec<String> = p.iter().map(|c| c.to_string_lossy().into_owned()).collect();
                    comps
                        .iter()
                        .rev()
                        .find(|c| is_pci(c) || is_i2c(c))
                        .or(comps.last())
                        .cloned()
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            let mut inputs: Vec<String> = self
                .fs
                .list(&base)
                .into_iter()
                .filter(|n| n.starts_with("temp") && n.ends_with("_input"))
                .collect();
            inputs.sort_by_key(|n| n[4..n.len() - 6].parse::<u32>().unwrap_or(0));
            for input in inputs {
                let stem = &input[..input.len() - 6];
                let label = self.fs.read(format!("{base}/{stem}_label")).unwrap_or_else(|_| stem.to_owned());
                out.push(HwmonTemp {
                    id: format!("{chip}:{device}:{label}"),
                    chip: chip.clone(),
                    device: device.clone(),
                    label,
                    input: format!("{base}/{input}"),
                });
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    /// °C, or `None` when unreadable or outside −40…150 °C.
    pub fn read_c(&self, t: &HwmonTemp) -> Option<f32> {
        let milli: i64 = self.fs.parse(&t.input).ok()?;
        let c = milli as f32 / 1000.0;
        (-40.0..=150.0).contains(&c).then_some(c)
    }

    pub fn find<'a>(temps: &'a [HwmonTemp], chip: &str, label: &str) -> Option<&'a HwmonTemp> {
        temps.iter().find(|t| t.chip == chip && t.label == label)
    }

    /// Path of amdgpu `power1_input`, for repeated reads without rescanning.
    pub fn apu_ppt_path(&self) -> Option<String> {
        self.fs.list("/sys/class/hwmon").into_iter().map(|h| format!("/sys/class/hwmon/{h}")).find_map(|base| {
            (self.fs.read(format!("{base}/name")).ok().as_deref() == Some("amdgpu")).then(|| format!("{base}/power1_input"))
        })
    }

    /// amdgpu `power1_input` (µW) = APU package PPT. Not the iGPU's own power.
    pub fn apu_ppt_w(&self) -> Option<f32> {
        self.read_w(&self.apu_ppt_path()?)
    }

    pub fn read_w(&self, path: &str) -> Option<f32> {
        let uw: u64 = self.fs.parse(path).ok()?;
        Some(uw as f32 / 1e6)
    }
}

fn is_pci(s: &str) -> bool {
    // 0000:02:00.0
    let b = s.as_bytes();
    b.len() == 12 && b[4] == b':' && b[7] == b':' && b[10] == b'.'
}

fn is_i2c(s: &str) -> bool {
    // 9-0050
    s.split_once('-').is_some_and(|(bus, addr)| bus.parse::<u32>().is_ok() && addr.len() == 4)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::sysfs::fake::{file, link};
    use std::path::Path;

    pub fn fake_hwmon(root: &Path) {
        let chip = |n: u32, name: &str, dev: &str, temps: &[(u32, &str, &str)]| {
            let base = format!("/sys/class/hwmon/hwmon{n}");
            file(root, &format!("{base}/name"), name);
            file(root, &format!("/sys/devices/{dev}/.keep"), "");
            link(root, &format!("{base}/device"), &root.join(format!("sys/devices/{dev}")).display().to_string());
            for (i, label, val) in temps {
                file(root, &format!("{base}/temp{i}_input"), val);
                if !label.is_empty() {
                    file(root, &format!("{base}/temp{i}_label"), label);
                }
            }
        };
        chip(3, "nvme", "pci0000:00/0000:00:01.5/0000:04:00.0/nvme/nvme1", &[(1, "Composite", "35850"), (2, "Sensor 1", "34850")]);
        chip(4, "nvme", "pci0000:00/0000:00:01.2/0000:02:00.0/nvme/nvme0", &[(1, "Composite", "36850")]);
        chip(6, "k10temp", "pci0000:00/0000:00:18.3", &[(1, "Tctl", "70875"), (3, "Tccd1", "66875"), (4, "Tccd2", "67875")]);
        chip(7, "spd5118", "pci0000:00/0000:00:14.0/i2c-9/9-0050", &[(1, "", "56500")]);
        chip(8, "spd5118", "pci0000:00/0000:00:14.0/i2c-9/9-0051", &[(1, "", "-273000")]);
        chip(5, "amdgpu", "pci0000:00/0000:00:08.1/0000:07:00.0", &[(1, "edge", "68000")]);
        file(root, "/sys/class/hwmon/hwmon5/power1_input", "41250000");
    }

    #[test]
    fn identical_chips_get_distinct_stable_ids() {
        let dir = tempfile::tempdir().unwrap();
        fake_hwmon(dir.path());
        let h = Hwmon::new(Sysfs::new(dir.path()));
        let temps = h.discover();
        let ids: Vec<&str> = temps.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"nvme:0000:02:00.0:Composite"));
        assert!(ids.contains(&"nvme:0000:04:00.0:Composite"));
        assert!(ids.contains(&"spd5118:9-0050:temp1"));
        assert!(ids.contains(&"spd5118:9-0051:temp1"));
        let tctl = Hwmon::find(&temps, "k10temp", "Tctl").unwrap();
        assert_eq!(h.read_c(tctl), Some(70.875));
        let bad = temps.iter().find(|t| t.id == "spd5118:9-0051:temp1").unwrap();
        assert_eq!(h.read_c(bad), None, "implausible value is null, not a number");
        assert_eq!(h.apu_ppt_w(), Some(41.25));
    }
}
