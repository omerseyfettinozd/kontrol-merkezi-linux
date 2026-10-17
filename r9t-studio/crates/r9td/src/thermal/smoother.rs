//! Raise immediately; reduce only after cooling (port of legacy `CurveSmoother`).
//!
//! Duty follows any increase at once. A decrease starts only when the temperature
//! has stayed at least 2 °C below the temperature of the last increase for 6 s;
//! then duty falls by at most 2 % per update until it meets the request.

use std::time::{Duration, Instant};

pub const COOLING_HYSTERESIS_C: f32 = 2.0;
pub const COOLING_HOLD: Duration = Duration::from_secs(6);
pub const DOWN_STEP_PCT: u8 = 2;

#[derive(Debug, Clone)]
pub struct Smoother {
    duty: u8,
    anchor_c: f32,
    cooling_since: Option<Instant>,
}

impl Smoother {
    pub fn new(duty: u8, temp_c: f32) -> Self {
        Self { duty, anchor_c: temp_c, cooling_since: None }
    }

    pub fn duty(&self) -> u8 {
        self.duty
    }

    pub fn update(&mut self, desired: u8, temp_c: f32, now: Instant) -> u8 {
        if desired >= self.duty {
            self.duty = desired;
            self.anchor_c = temp_c;
            self.cooling_since = None;
        } else if temp_c > self.anchor_c - COOLING_HYSTERESIS_C {
            self.cooling_since = None;
        } else {
            let since = *self.cooling_since.get_or_insert(now);
            if now.duration_since(since) >= COOLING_HOLD {
                self.duty = desired.max(self.duty.saturating_sub(DOWN_STEP_PCT));
                if self.duty == desired {
                    self.anchor_c = temp_c;
                    self.cooling_since = None;
                }
            }
        }
        self.duty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t0: Instant, s: u64) -> Instant {
        t0 + Duration::from_secs(s)
    }

    #[test]
    fn heating_is_immediate() {
        let t0 = Instant::now();
        let mut s = Smoother::new(60, 60.0);
        assert_eq!(s.update(80, 70.0, t0), 80);
    }

    #[test]
    fn jitter_holds_and_cooling_ramps_down_gradually() {
        let t0 = Instant::now();
        let mut s = Smoother::new(80, 75.0);
        // Within 2 °C of the anchor: hold.
        assert_eq!(s.update(60, 73.5, at(t0, 0)), 80);
        assert_eq!(s.update(60, 73.5, at(t0, 30)), 80);
        // Cooler, but hold time not yet elapsed.
        assert_eq!(s.update(60, 72.0, at(t0, 31)), 80);
        assert_eq!(s.update(60, 72.0, at(t0, 36)), 80);
        // Then -2 % per update.
        assert_eq!(s.update(60, 72.0, at(t0, 37)), 78);
        assert_eq!(s.update(60, 72.0, at(t0, 39)), 76);
    }

    #[test]
    fn interrupted_cooling_restarts_hold() {
        let t0 = Instant::now();
        let mut s = Smoother::new(80, 75.0);
        s.update(60, 72.0, at(t0, 0));
        s.update(60, 74.0, at(t0, 4)); // warmed back within hysteresis
        assert_eq!(s.update(60, 72.0, at(t0, 7)), 80);
        assert_eq!(s.update(60, 72.0, at(t0, 12)), 80);
        assert_eq!(s.update(60, 72.0, at(t0, 13)), 78);
    }

    #[test]
    fn reaches_request_and_re_anchors() {
        let t0 = Instant::now();
        let mut s = Smoother::new(62, 75.0);
        s.update(60, 70.0, at(t0, 0));
        assert_eq!(s.update(60, 70.0, at(t0, 6)), 60);
        // New anchor is 70 °C: 69 °C is within hysteresis.
        assert_eq!(s.update(55, 69.0, at(t0, 20)), 60);
    }
}
