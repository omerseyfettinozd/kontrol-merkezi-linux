//! Fan stall detection (safety invariant 5), using the measured duty→RPM curve
//! from `docs/07_fan_rpm_egrisi.json`.
//!
//! Stall: duty > 0 and RPM below 50 % of the expected RPM for that duty (never below
//! 500) in two consecutive `status` samples; or `status` failing for 10 s while in a
//! manual/curve mode. Response: `fan_boost` first, then `auto`.

use std::time::{Duration, Instant};

/// (duty %, median RPM) for the CPU and GPU fan: 50–100 % measured 2026-10-18 on
/// r9t_fan 0.2.0, 20–45 % on 0.3.0 with `r9tctl fan-sweep` 2026-10-19 (docs/07).
pub const CPU_CURVE: [(u8, u32); 12] = [
    (20, 1264), (25, 1597), (30, 1933), (35, 2253), (40, 2563), (45, 2855),
    (50, 3180), (60, 3717), (70, 4269), (80, 4759), (90, 5246), (100, 5765),
];
pub const GPU_CURVE: [(u8, u32); 12] = [
    (20, 1332), (25, 1668), (30, 2003), (35, 2328), (40, 2623), (45, 2917),
    (50, 3232), (60, 3756), (70, 4269), (80, 4718), (90, 5170), (100, 5615),
];
pub const STATUS_ERROR_LIMIT: Duration = Duration::from_secs(10);

/// Minimum acceptable RPM for `duty` (linear between measured points).
pub fn stall_threshold(curve: &[(u8, u32)], duty: u8) -> u32 {
    if duty == 0 {
        return 0;
    }
    let expected = if duty <= curve[0].0 {
        // Below the measured range: scale the lowest point.
        curve[0].1 * u32::from(duty) / u32::from(curve[0].0)
    } else {
        curve
            .windows(2)
            .find(|w| duty <= w[1].0)
            .map(|w| {
                let (d0, r0) = (u32::from(w[0].0), w[0].1);
                let (d1, r1) = (u32::from(w[1].0), w[1].1);
                r0 + (r1 - r0) * (u32::from(duty) - d0) / (d1 - d0)
            })
            .unwrap_or(curve[curve.len() - 1].1)
    };
    (expected / 2).max(500)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StallAction {
    None,
    /// First response: both fans full.
    Boost,
    /// Boost did not help (or status keeps failing): hand back to the EC.
    Auto,
}

#[derive(Debug, Default)]
pub struct StallDetector {
    low_streak: u8,
    error_since: Option<Instant>,
    boosted: bool,
}

impl StallDetector {
    /// One successful `status` sample. Duty is in percent.
    pub fn sample(&mut self, duty: (u8, u8), rpm: (i32, i32), now: Instant) -> StallAction {
        let _ = now;
        self.error_since = None;
        let low = |d: u8, r: i32, c: &[(u8, u32)]| d > 0 && r < stall_threshold(c, d) as i32;
        if low(duty.0, rpm.0, &CPU_CURVE) || low(duty.1, rpm.1, &GPU_CURVE) {
            self.low_streak = self.low_streak.saturating_add(1);
        } else {
            self.low_streak = 0;
            self.boosted = false;
        }
        if self.low_streak >= 2 {
            self.low_streak = 0;
            return self.escalate();
        }
        StallAction::None
    }

    /// `status` failed while r9td owns the fans.
    pub fn status_error(&mut self, now: Instant) -> StallAction {
        let since = *self.error_since.get_or_insert(now);
        if now.duration_since(since) >= STATUS_ERROR_LIMIT {
            self.error_since = Some(now);
            return self.escalate();
        }
        StallAction::None
    }

    fn escalate(&mut self) -> StallAction {
        if self.boosted {
            StallAction::Auto
        } else {
            self.boosted = true;
            StallAction::Boost
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_follow_measured_curve() {
        assert_eq!(stall_threshold(&CPU_CURVE, 50), 1590);
        assert_eq!(stall_threshold(&CPU_CURVE, 55), (3180 + (3717 - 3180) / 2) / 2);
        assert_eq!(stall_threshold(&CPU_CURVE, 100), 2882);
        assert_eq!(stall_threshold(&CPU_CURVE, 0), 0);
        assert_eq!(stall_threshold(&CPU_CURVE, 20), 632, "measured low-duty point");
        assert_eq!(stall_threshold(&CPU_CURVE, 30), 966);
        assert_eq!(stall_threshold(&GPU_CURVE, 25), 834);
        assert_eq!(stall_threshold(&CPU_CURVE, 10), 500, "below 20 %: scaled, then the 500 floor");
        assert_eq!(stall_threshold(&CPU_CURVE, 5), 500, "never below 500");
    }

    #[test]
    fn two_low_samples_boost_then_auto() {
        let t = Instant::now();
        let mut d = StallDetector::default();
        assert_eq!(d.sample((60, 60), (3700, 3700), t), StallAction::None);
        assert_eq!(d.sample((60, 60), (1000, 3700), t), StallAction::None);
        assert_eq!(d.sample((60, 60), (1000, 3700), t), StallAction::Boost);
        assert_eq!(d.sample((100, 100), (900, 5600), t), StallAction::None);
        assert_eq!(d.sample((100, 100), (900, 5600), t), StallAction::Auto);
    }

    #[test]
    fn single_glitch_is_ignored() {
        let t = Instant::now();
        let mut d = StallDetector::default();
        d.sample((60, 60), (0, 3700), t);
        assert_eq!(d.sample((60, 60), (3700, 3700), t), StallAction::None);
        assert_eq!(d.sample((60, 60), (0, 3700), t), StallAction::None);
    }

    #[test]
    fn status_errors_for_10s() {
        let t = Instant::now();
        let mut d = StallDetector::default();
        assert_eq!(d.status_error(t), StallAction::None);
        assert_eq!(d.status_error(t + Duration::from_secs(9)), StallAction::None);
        assert_eq!(d.status_error(t + Duration::from_secs(10)), StallAction::Boost);
        assert_eq!(d.status_error(t + Duration::from_secs(20)), StallAction::Auto);
    }
}
