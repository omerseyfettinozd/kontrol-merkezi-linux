//! ITE 8291 per-key keyboard: 126 `rgb:kbd_backlight*` LEDs, global brightness,
//! frames batched through `controls/buffer_input` (one USB write instead of 126).

use crate::{HalError, Result, Sysfs};

pub const CHANNELS: usize = 126;

#[derive(Debug)]
pub struct Rgb {
    fs: Sysfs,
    leds: Vec<String>,
    buffer_input: Option<String>,
    max_brightness: u32,
}

impl Rgb {
    /// Finds exactly 126 ite_8291 LEDs in channel order; anything else is unsupported.
    pub fn discover(fs: Sysfs) -> Result<Self> {
        let mut leds: Vec<String> = fs
            .list("/sys/class/leds")
            .into_iter()
            .filter(|n| n == "rgb:kbd_backlight" || n.starts_with("rgb:kbd_backlight_"))
            .filter(|n| fs.exists(format!("/sys/class/leds/{n}/multi_intensity")))
            .collect();
        leds.sort_by_key(|n| n.strip_prefix("rgb:kbd_backlight_").and_then(|i| i.parse::<u32>().ok()).unwrap_or(0));
        let expected: Vec<String> = std::iter::once("rgb:kbd_backlight".to_owned())
            .chain((1..CHANNELS).map(|i| format!("rgb:kbd_backlight_{i}")))
            .collect();
        if leds != expected {
            return Err(HalError::Unsupported(format!("{} LED bulundu, 126 ite_8291 kanalı bekleniyor", leds.len())));
        }
        let driver = fs.link_name("/sys/class/leds/rgb:kbd_backlight/device/driver");
        if driver.as_deref().is_some_and(|d| d != "ite_8291") {
            return Err(HalError::Unsupported(format!("RGB sürücüsü {driver:?}")));
        }
        let max_intensity = fs.read("/sys/class/leds/rgb:kbd_backlight/multi_max_intensity").unwrap_or_default();
        if max_intensity.split_whitespace().any(|v| v.parse::<u32>().unwrap_or(0) < 255) {
            // R9T_RGB_INTENSITY_FIX local patch lost (tuxedo-drivers update?).
            return Err(HalError::Unsupported(format!("multi_max_intensity={max_intensity:?}, 255 bekleniyor")));
        }
        let buffer_input = fs
            .list("/sys/bus/hid/devices")
            .into_iter()
            .filter(|d| d.to_ascii_uppercase().starts_with("0003:048D:600B."))
            .map(|d| format!("/sys/bus/hid/devices/{d}/controls/buffer_input"))
            .find(|p| fs.exists(p));
        let max_brightness = fs.parse("/sys/class/leds/rgb:kbd_backlight/max_brightness")?;
        Ok(Self { fs, leds, buffer_input, max_brightness })
    }

    pub fn buffered(&self) -> bool {
        self.buffer_input.is_some()
    }

    pub fn max_brightness(&self) -> u32 {
        self.max_brightness
    }

    /// Global brightness 0..=max (writing one LED changes all).
    pub fn brightness(&self) -> Result<u32> {
        self.fs.parse("/sys/class/leds/rgb:kbd_backlight/brightness")
    }

    pub fn set_brightness(&self, level: u32) -> Result<()> {
        if level > self.max_brightness {
            return Err(HalError::Invalid(format!("parlaklık {level} > {}", self.max_brightness)));
        }
        self.fs.write("/sys/class/leds/rgb:kbd_backlight/brightness", &level.to_string())
    }

    pub fn read_frame(&self) -> Result<Vec<[u8; 3]>> {
        self.leds
            .iter()
            .map(|led| {
                let base = format!("/sys/class/leds/{led}");
                let index = self.fs.read(format!("{base}/multi_index"))?;
                let values: Vec<u8> = self
                    .fs
                    .read(format!("{base}/multi_intensity"))?
                    .split_whitespace()
                    .map(|v| v.parse().map_err(|_| HalError::Invalid(format!("{led} multi_intensity"))))
                    .collect::<Result<_>>()?;
                let mut rgb = [0u8; 3];
                for (ch, v) in index.split_whitespace().zip(values) {
                    match ch {
                        "red" => rgb[0] = v,
                        "green" => rgb[1] = v,
                        "blue" => rgb[2] = v,
                        _ => return Err(HalError::Invalid(format!("{led} kanal {ch}"))),
                    }
                }
                Ok(rgb)
            })
            .collect()
    }

    /// Writes all 126 colours. With `buffer_input` this is one USB transfer; the
    /// buffer is always released (`0`) on success and on every error path.
    pub fn write_frame(&self, colors: &[[u8; 3]]) -> Result<()> {
        if colors.len() != CHANNELS {
            return Err(HalError::Invalid(format!("{} renk, 126 bekleniyor", colors.len())));
        }
        let guard = match &self.buffer_input {
            Some(p) => {
                self.fs.write(p, "1")?;
                Some(BufferGuard { fs: &self.fs, path: p })
            }
            None => None,
        };
        for (led, [r, g, b]) in self.leds.iter().zip(colors) {
            self.fs.write(format!("/sys/class/leds/{led}/multi_intensity"), &format!("{r} {g} {b}"))?;
        }
        match guard {
            Some(g) => g.flush(),
            None => Ok(()),
        }
    }

    /// Used at startup, shutdown and in error paths (safety invariant 2).
    pub fn release_buffer(&self) -> Result<()> {
        match &self.buffer_input {
            Some(p) => self.fs.write(p, "0"),
            None => Ok(()),
        }
    }
}

struct BufferGuard<'a> {
    fs: &'a Sysfs,
    path: &'a str,
}

impl BufferGuard<'_> {
    fn flush(self) -> Result<()> {
        let r = self.fs.write(self.path, "0");
        std::mem::forget(self);
        r
    }
}

impl Drop for BufferGuard<'_> {
    fn drop(&mut self) {
        let _ = self.fs.write(self.path, "0");
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::sysfs::fake::file;
    use std::path::Path;

    pub fn fake_leds(root: &Path, with_buffer: bool) {
        for i in 0..CHANNELS {
            let name = if i == 0 { "rgb:kbd_backlight".to_owned() } else { format!("rgb:kbd_backlight_{i}") };
            let b = format!("/sys/class/leds/{name}");
            file(root, &format!("{b}/multi_intensity"), "255 255 255");
            file(root, &format!("{b}/multi_index"), "red green blue");
            file(root, &format!("{b}/multi_max_intensity"), "255 255 255");
            file(root, &format!("{b}/max_brightness"), "50");
            file(root, &format!("{b}/brightness"), "8");
        }
        if with_buffer {
            file(root, "/sys/bus/hid/devices/0003:048D:600B.0001/controls/buffer_input", "0");
        }
    }

    #[test]
    fn frame_round_trip_releases_buffer() {
        let dir = tempfile::tempdir().unwrap();
        fake_leds(dir.path(), true);
        let rgb = Rgb::discover(Sysfs::new(dir.path())).unwrap();
        assert!(rgb.buffered());
        let mut frame = vec![[0, 0, 0]; CHANNELS];
        frame[5] = [1, 2, 3];
        rgb.write_frame(&frame).unwrap();
        assert_eq!(rgb.read_frame().unwrap()[5], [1, 2, 3]);
        assert_eq!(std::fs::read_to_string(dir.path().join("sys/bus/hid/devices/0003:048D:600B.0001/controls/buffer_input")).unwrap(), "0");
        assert!(rgb.write_frame(&frame[..10]).is_err());
        assert!(rgb.set_brightness(51).is_err());
    }

    #[test]
    fn buffer_released_on_error_path() {
        let dir = tempfile::tempdir().unwrap();
        fake_leds(dir.path(), true);
        let rgb = Rgb::discover(Sysfs::new(dir.path())).unwrap();
        std::fs::remove_file(dir.path().join("sys/class/leds/rgb:kbd_backlight_40/multi_intensity")).unwrap();
        assert!(rgb.write_frame(&vec![[9, 9, 9]; CHANNELS]).is_err());
        assert_eq!(std::fs::read_to_string(dir.path().join("sys/bus/hid/devices/0003:048D:600B.0001/controls/buffer_input")).unwrap(), "0");
    }

    #[test]
    fn lost_intensity_patch_is_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        fake_leds(dir.path(), false);
        file(dir.path(), "/sys/class/leds/rgb:kbd_backlight/multi_max_intensity", "50 50 50");
        assert!(matches!(Rgb::discover(Sysfs::new(dir.path())), Err(HalError::Unsupported(_))));
    }
}
