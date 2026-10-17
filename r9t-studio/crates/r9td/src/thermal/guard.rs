//! User-space thermal guard (safety invariant 4). Applied before every other rule.
//!
//! Enter: CPU ≥ 90 °C or GPU ≥ 84 °C (EC temperatures) → both fans 100 %.
//! Release: CPU < 85 °C and GPU < 80 °C continuously for 10 s.

use std::time::{Duration, Instant};

pub const ENTER_CPU_C: f32 = 90.0;
pub const ENTER_GPU_C: f32 = 84.0;
pub const RELEASE_CPU_C: f32 = 85.0;
pub const RELEASE_GPU_C: f32 = 80.0;
pub const RELEASE_HOLD: Duration = Duration::from_secs(10);

#[derive(Debug, Default, Clone)]
pub struct Guard {
    active: bool,
    cool_since: Option<Instant>,
}

impl Guard {
    pub fn active(&self) -> bool {
        self.active
    }

    /// Returns whether the guard forces 100 %. `None` temperatures never release it.
    pub fn update(&mut self, cpu_c: Option<f32>, gpu_c: Option<f32>, now: Instant) -> bool {
        let hot = cpu_c.is_some_and(|c| c >= ENTER_CPU_C) || gpu_c.is_some_and(|g| g >= ENTER_GPU_C);
        if hot {
            self.active = true;
            self.cool_since = None;
            return true;
        }
        if self.active {
            let cool = matches!((cpu_c, gpu_c), (Some(c), Some(g)) if c < RELEASE_CPU_C && g < RELEASE_GPU_C);
            if cool {
                let since = *self.cool_since.get_or_insert(now);
                if now.duration_since(since) >= RELEASE_HOLD {
                    self.active = false;
                    self.cool_since = None;
                }
            } else {
                self.cool_since = None;
            }
        }
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_hold_and_release() {
        let t0 = Instant::now();
        let s = |n| t0 + Duration::from_secs(n);
        let mut g = Guard::default();
        assert!(!g.update(Some(89.0), Some(83.0), s(0)));
        assert!(g.update(Some(90.0), Some(60.0), s(1)));
        assert!(g.update(Some(60.0), Some(84.0), s(2)), "GPU enters too");
        // Between release and enter thresholds: stays on.
        assert!(g.update(Some(87.0), Some(70.0), s(3)));
        assert!(g.update(Some(84.0), Some(79.0), s(4)));
        assert!(g.update(Some(84.0), Some(79.0), s(13)));
        assert!(!g.update(Some(84.0), Some(79.0), s(14)), "released after 10 s");
    }

    #[test]
    fn missing_sensor_never_releases() {
        let t0 = Instant::now();
        let mut g = Guard::default();
        g.update(Some(95.0), Some(60.0), t0);
        for n in 1..30 {
            assert!(g.update(None, Some(60.0), t0 + Duration::from_secs(n)));
        }
    }

    #[test]
    fn warm_spell_restarts_release_timer() {
        let t0 = Instant::now();
        let s = |n| t0 + Duration::from_secs(n);
        let mut g = Guard::default();
        g.update(Some(91.0), Some(60.0), s(0));
        g.update(Some(80.0), Some(60.0), s(1));
        g.update(Some(86.0), Some(60.0), s(8)); // above release threshold
        assert!(g.update(Some(80.0), Some(60.0), s(9)));
        assert!(g.update(Some(80.0), Some(60.0), s(18)));
        assert!(!g.update(Some(80.0), Some(60.0), s(19)));
    }
}
