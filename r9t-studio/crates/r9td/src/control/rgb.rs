//! Keyboard light engine: static colour / map / 4 zones and software effects.
//! Effects run at ≤ 20 FPS (thermal gradient 2 FPS) and stop (0 FPS) on battery,
//! when the session is idle or the lid is closed. Frames go through `buffer_input`
//! (one USB transfer, ~26 ms); the buffer is released on every exit path.

use std::f32::consts::PI;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use r9t_hal::rgb::{Rgb, CHANNELS};
use r9t_protocol::rpc::RgbEffect;

pub const MAX_FPS: u32 = 20;
const COLS: usize = 21;
/// Audio frames older than this fall back to the static base.
const FRAME_TIMEOUT: Duration = Duration::from_secs(2);

pub type Frame = Vec<[u8; 3]>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Inhibit {
    pub battery: bool,
    pub idle: bool,
    pub lid_closed: bool,
}

impl Inhibit {
    pub fn any(&self) -> bool {
        self.battery || self.idle || self.lid_closed
    }
}

pub enum Msg {
    Static(Frame),
    Effect(RgbEffect),
    AudioFrame(Frame),
    Inhibit(Inhibit),
    /// CPU EC temperature for the thermal gradient.
    CpuTemp(f32),
    Stop(mpsc::Sender<()>),
}

pub struct RgbEngine {
    pub tx: mpsc::Sender<Msg>,
    handle: Option<JoinHandle<()>>,
}

impl RgbEngine {
    pub fn start(rgb: Arc<Rgb>, base: Frame) -> Self {
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::Builder::new().name("rgb".into()).spawn(move || run(rgb, base, rx)).ok();
        Self { tx, handle }
    }

    pub fn send(&self, m: Msg) {
        let _ = self.tx.send(m);
    }

    /// Stops effects and releases the HID buffer; waits up to 1 s.
    pub fn stop(&mut self) {
        let (ack_tx, ack_rx) = mpsc::channel();
        if self.tx.send(Msg::Stop(ack_tx)).is_ok() {
            let _ = ack_rx.recv_timeout(Duration::from_secs(1));
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Four zones as column ranges (until the TR-Q keymap is calibrated).
pub fn zones(colors: [[u8; 3]; 4]) -> Frame {
    (0..CHANNELS).map(|i| colors[match i % COLS { 0..=4 => 0, 5..=9 => 1, 10..=14 => 2, _ => 3 }]).collect()
}

pub fn solid(c: [u8; 3]) -> Frame {
    vec![c; CHANNELS]
}

fn hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = (h.rem_euclid(1.0)) * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f32;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8]
}

/// 40 °C blue → 60 green → 75 yellow → 90 °C red.
pub fn thermal_color(c: f32) -> [u8; 3] {
    let stops = [(40.0, [0u8, 80, 255]), (60.0, [0, 220, 80]), (75.0, [255, 200, 0]), (90.0, [255, 0, 0])];
    if c <= stops[0].0 {
        return stops[0].1;
    }
    for w in stops.windows(2) {
        let ((t0, c0), (t1, c1)) = (w[0], w[1]);
        if c <= t1 {
            let k = (c - t0) / (t1 - t0);
            return [0, 1, 2].map(|i| (c0[i] as f32 + (c1[i] as f32 - c0[i] as f32) * k) as u8);
        }
    }
    stops[3].1
}

/// Renders one effect frame at time `t` seconds; `None` for static modes.
pub fn render(effect: &RgbEffect, _base: &Frame, t: f32, cpu_c: Option<f32>) -> Option<Frame> {
    match effect {
        RgbEffect::Static | RgbEffect::AudioReactive => None,
        RgbEffect::Breathing { rgb, period_ms } => {
            let period = (*period_ms).max(500) as f32 / 1000.0;
            let k = 0.15 + 0.85 * (0.5 - 0.5 * (2.0 * PI * t / period).cos());
            Some(solid(rgb.map(|c| (c as f32 * k) as u8)))
        }
        RgbEffect::Wave { speed } => {
            let s = (*speed).clamp(1, 10) as f32 / 10.0;
            Some((0..CHANNELS).map(|i| hsv((i % COLS) as f32 / COLS as f32 - t * s * 0.5, 1.0, 1.0)).collect())
        }
        RgbEffect::ThermalGradient => Some(solid(thermal_color(cpu_c.unwrap_or(40.0)))),
    }
}

fn interval(effect: &RgbEffect) -> Option<Duration> {
    match effect {
        RgbEffect::Static | RgbEffect::AudioReactive => None,
        RgbEffect::ThermalGradient => Some(Duration::from_millis(500)),
        _ => Some(Duration::from_millis(1000 / MAX_FPS as u64)),
    }
}

fn run(rgb: Arc<Rgb>, mut base: Frame, rx: mpsc::Receiver<Msg>) {
    let _ = rgb.release_buffer();
    let mut effect = RgbEffect::Static;
    let mut inhibit = Inhibit::default();
    let mut cpu_c: Option<f32> = None;
    let mut last_audio: Option<Instant> = None;
    let start = Instant::now();
    let write = |f: &Frame| {
        if let Err(e) = rgb.write_frame(f) {
            eprintln!("r9td: RGB yazılamadı: {e}");
        }
    };
    loop {
        let animated = !inhibit.any() && interval(&effect).is_some();
        let wait = if animated { interval(&effect).unwrap_or(Duration::from_secs(3600)) } else { Duration::from_secs(3600) };
        match rx.recv_timeout(wait) {
            Ok(Msg::Static(f)) => {
                base = f;
                effect = RgbEffect::Static;
                write(&base);
            }
            Ok(Msg::Effect(e)) => {
                effect = e;
                if interval(&effect).is_none() {
                    write(&base);
                }
            }
            Ok(Msg::AudioFrame(f)) => {
                if matches!(effect, RgbEffect::AudioReactive) && !inhibit.any() && f.len() == CHANNELS {
                    // Rate limit: drop frames arriving faster than 20 FPS.
                    if last_audio.is_none_or(|t| t.elapsed() >= Duration::from_millis(1000 / MAX_FPS as u64)) {
                        last_audio = Some(Instant::now());
                        write(&f);
                    }
                }
            }
            Ok(Msg::Inhibit(i)) => {
                let was = inhibit.any();
                inhibit = i;
                if inhibit.any() && !was {
                    write(&base); // freeze on the static base, 0 FPS
                }
            }
            Ok(Msg::CpuTemp(c)) => cpu_c = Some(c),
            Ok(Msg::Stop(ack)) => {
                let _ = rgb.release_buffer();
                let _ = ack.send(());
                return;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                let _ = rgb.release_buffer();
                return;
            }
        }
        if !inhibit.any() {
            if let Some(f) = render(&effect, &base, start.elapsed().as_secs_f32(), cpu_c) {
                write(&f);
            }
        }
        if matches!(effect, RgbEffect::AudioReactive) && last_audio.is_some_and(|t| t.elapsed() > FRAME_TIMEOUT) {
            last_audio = None;
            write(&base);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_split_columns() {
        let f = zones([[1, 0, 0], [2, 0, 0], [3, 0, 0], [4, 0, 0]]);
        assert_eq!(f.len(), CHANNELS);
        assert_eq!((f[0], f[5], f[10], f[20], f[21]), ([1, 0, 0], [2, 0, 0], [3, 0, 0], [4, 0, 0], [1, 0, 0]));
    }

    #[test]
    fn thermal_colors_and_effect_rates() {
        assert_eq!(thermal_color(30.0), [0, 80, 255]);
        assert_eq!(thermal_color(95.0), [255, 0, 0]);
        let mid = thermal_color(67.5);
        assert!(mid[0] > 100 && mid[1] > 150);
        assert_eq!(interval(&RgbEffect::Static), None);
        assert_eq!(interval(&RgbEffect::Wave { speed: 5 }), Some(Duration::from_millis(50)));
        assert_eq!(interval(&RgbEffect::ThermalGradient), Some(Duration::from_millis(500)));
    }

    #[test]
    fn breathing_stays_in_range_and_static_renders_nothing() {
        let base = solid([10, 10, 10]);
        let e = RgbEffect::Breathing { rgb: [200, 100, 0], period_ms: 2000 };
        let lo = render(&e, &base, 0.0, None).unwrap()[0];
        let hi = render(&e, &base, 1.0, None).unwrap()[0];
        assert!(lo[0] < hi[0] && hi[0] <= 200 && lo[0] >= 29);
        assert!(render(&RgbEffect::Static, &base, 1.0, None).is_none());
        assert!(render(&RgbEffect::AudioReactive, &base, 1.0, None).is_none());
    }

    #[test]
    fn inhibit_any() {
        assert!(!Inhibit::default().any());
        assert!(Inhibit { battery: true, ..Default::default() }.any());
    }
}
