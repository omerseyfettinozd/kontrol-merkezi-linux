//! Volume through WirePlumber (`wpctl`) and the audio-reactive keyboard effect.
//! The capture uses `pw-record` on the default sink monitor (PipeWire), so no C
//! bindings or clang are needed; only band energies are computed here.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

pub const SINK: &str = "@DEFAULT_AUDIO_SINK@";
pub const SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";

/// `Volume: 0.40 [MUTED]` → (40, muted)
pub fn parse_volume(s: &str) -> Option<(u8, bool)> {
    let v: f32 = s.split_whitespace().nth(1)?.parse().ok()?;
    Some(((v * 100.0).round().clamp(0.0, 150.0) as u8, s.contains("[MUTED]")))
}

pub fn get(target: &str) -> Option<(u8, bool)> {
    let out = Command::new("wpctl").args(["get-volume", target]).output().ok()?;
    parse_volume(&String::from_utf8_lossy(&out.stdout))
}

pub fn set(target: &str, pct: u8) -> Result<u8, String> {
    if pct > 100 {
        return Err("ses %0–100 olmalı".into());
    }
    let st = Command::new("wpctl").args(["set-volume", target, &format!("{:.2}", pct as f32 / 100.0)]).status().map_err(|e| e.to_string())?;
    if !st.success() {
        return Err("wpctl set-volume başarısız".into());
    }
    match get(target) {
        Some((v, _)) if v == pct => Ok(v),
        Some((v, _)) => Err(format!("geri okuma {v} %")),
        None => Err("ses geri okunamadı".into()),
    }
}

pub const RATE: u32 = 24_000;
/// 50 ms of mono s16 → 20 frames per second.
pub const CHUNK: usize = (RATE as usize) / 20;
const BANDS: [f32; 7] = [60.0, 150.0, 400.0, 1000.0, 2500.0, 6000.0, 10000.0];

/// Goertzel magnitude of `freq` in `samples` (normalised to 0..1-ish).
fn goertzel(samples: &[f32], freq: f32) -> f32 {
    let k = 2.0 * (2.0 * std::f32::consts::PI * freq / RATE as f32).cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &x in samples {
        let s0 = x + k * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    ((s1 * s1 + s2 * s2 - k * s1 * s2).max(0.0)).sqrt() / samples.len() as f32
}

/// Seven bands (each 3 columns) lit bottom-up across 6 rows, one hue per band.
pub fn frame(samples: &[i16], peaks: &mut [f32; 7]) -> Vec<[u8; 3]> {
    let x: Vec<f32> = samples.iter().map(|&s| s as f32 / 32768.0).collect();
    let mut levels = [0usize; 7];
    for (i, f) in BANDS.iter().enumerate() {
        let m = goertzel(&x, *f);
        peaks[i] = (peaks[i] * 0.995).max(m).max(1e-4);
        levels[i] = ((m / peaks[i]) * 6.0).round().clamp(0.0, 6.0) as usize;
    }
    let mut out = vec![[0u8; 3]; 126];
    for (b, lvl) in levels.iter().enumerate() {
        let hue = b as f32 / 7.0;
        let color = hsv(hue);
        for col in b * 3..b * 3 + 3 {
            for row in 0..*lvl {
                out[(5 - row) * 21 + col] = color;
            }
        }
    }
    out
}

fn hsv(h: f32) -> [u8; 3] {
    let i = (h * 6.0).floor() as i32;
    let f = h * 6.0 - i as f32;
    let (q, t) = ((1.0 - f) * 255.0, f * 255.0);
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (255.0, t, 0.0),
        1 => (q, 255.0, 0.0),
        2 => (0.0, 255.0, t),
        3 => (0.0, q, 255.0),
        4 => (t, 0.0, 255.0),
        _ => (255.0, 0.0, q),
    };
    [r as u8, g as u8, b as u8]
}

pub struct Capture {
    child: Child,
    rx: Receiver<Vec<i16>>,
}

pub enum Next {
    Chunk(Vec<i16>),
    /// Nothing arrived in time: sink suspended, or the audio device is stuck.
    Idle,
    /// The recorder exited.
    Closed,
}

impl Capture {
    /// Records the default sink monitor (what is playing), mono s16 at 24 kHz.
    pub fn start() -> Result<Self, String> {
        let mut cmd = Command::new("pw-record");
        cmd.args(["--rate", &RATE.to_string(), "--channels", "1", "--format", "s16", "-P", "{ stream.capture.sink=true node.name=r9t-audio-reactive }", "-"]);
        Self::spawn(cmd)
    }

    /// Reads on its own thread so a recorder that never produces data (stuck
    /// device) cannot block `next`, and therefore cannot block switching off.
    fn spawn(mut cmd: Command) -> Result<Self, String> {
        let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| format!("pw-record: {e}"))?;
        let mut out = child.stdout.take().ok_or("pw-record: stdout yok")?;
        let (tx, rx) = mpsc::sync_channel(4);
        std::thread::Builder::new()
            .name("r9t-audio-read".into())
            .spawn(move || {
                let mut buf = vec![0u8; CHUNK * 2];
                while out.read_exact(&mut buf).is_ok() {
                    if tx.send(buf.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { child, rx })
    }

    /// Waits up to `wait` for the next 50 ms chunk.
    pub fn next(&mut self, wait: Duration) -> Next {
        match self.rx.recv_timeout(wait) {
            Ok(c) => Next::Chunk(c),
            Err(RecvTimeoutError::Timeout) => Next::Idle,
            Err(RecvTimeoutError::Disconnected) => Next::Closed,
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // The reader thread sees EOF once the child is gone and exits.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_parse() {
        assert_eq!(parse_volume("Volume: 0.40"), Some((40, false)));
        assert_eq!(parse_volume("Volume: 0.15 [MUTED]"), Some((15, true)));
        assert_eq!(parse_volume("garbage"), None);
    }

    #[test]
    fn tone_lights_its_band() {
        let samples: Vec<i16> = (0..CHUNK).map(|i| ((2.0 * std::f32::consts::PI * 1000.0 * i as f32 / RATE as f32).sin() * 20000.0) as i16).collect();
        let mut peaks = [1e-4; 7];
        let f = frame(&samples, &mut peaks);
        // 1 kHz is band 3 (columns 9..12): bottom row lit.
        assert_ne!(f[5 * 21 + 9], [0, 0, 0]);
        let silent = frame(&vec![0; CHUNK], &mut peaks);
        assert!(silent.iter().all(|c| *c == [0, 0, 0]));
    }

    #[test]
    fn silent_recorder_does_not_block_and_is_killed() {
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        let mut cap = Capture::spawn(cmd).unwrap();
        let pid = cap.child.id();
        let t = std::time::Instant::now();
        assert!(matches!(cap.next(Duration::from_millis(100)), Next::Idle));
        assert!(t.elapsed() < Duration::from_secs(1));
        drop(cap);
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists(), "recorder left running");
    }

    #[test]
    fn chunks_then_closed() {
        let mut cmd = Command::new("head");
        cmd.args(["-c", &(CHUNK * 2 * 2).to_string(), "/dev/zero"]);
        let mut cap = Capture::spawn(cmd).unwrap();
        for _ in 0..2 {
            assert!(matches!(cap.next(Duration::from_secs(2)), Next::Chunk(c) if c.len() == CHUNK));
        }
        assert!(matches!(cap.next(Duration::from_secs(2)), Next::Closed));
    }
}
