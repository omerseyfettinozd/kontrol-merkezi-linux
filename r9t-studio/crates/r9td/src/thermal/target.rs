//! Temperature target (port of legacy `temperature_target.py`, docs/05 §2.5).
//!
//! Fan: `duty = clamp(100 + (T − target) × 5, 50, 100)`, smoothed. Frequency: when
//! T > target with the fan already at 100 %, the ceiling drops 100 MHz (floor: CPU
//! allowed minimum, GPU 600). When T ≤ target − 3 °C for 10 s it rises 100 MHz up
//! to the starting ceiling. Updates run every ≥ 2 s; a gap > 10 s (sleep, stall)
//! aborts with an emergency stop.

use std::time::{Duration, Instant};

use super::smoother::Smoother;

pub const STEP_MHZ: u32 = 100;
pub const MIN_INTERVAL: Duration = Duration::from_secs(2);
pub const MAX_GAP: Duration = Duration::from_secs(10);
pub const RAISE_HOLD: Duration = Duration::from_secs(10);
pub const GPU_FLOOR_MHZ: u32 = 600;

pub fn validate_targets(cpu: u8, gpu: u8) -> Result<(), String> {
    if (60..=85).contains(&cpu) && (60..=80).contains(&gpu) {
        Ok(())
    } else {
        Err("CPU hedefi 60–85 °C, GPU hedefi 60–80 °C olmalı.".into())
    }
}

pub fn fan_target(temp_c: f32, target_c: f32) -> u8 {
    (100.0 + (temp_c - target_c) * 5.0).round().clamp(50.0, 100.0) as u8
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetStep {
    pub duty: (u8, u8),
    /// New CPU ceiling in MHz when it changed this step.
    pub cpu_limit: Option<u32>,
    pub gpu_limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetError {
    /// Update gap > 10 s: restore limits, fans auto.
    Interrupted,
}

#[derive(Debug, Clone)]
pub struct TemperatureTarget {
    targets: [f32; 2],
    ceilings: [u32; 2],
    floors: [u32; 2],
    limits: [u32; 2],
    smoothers: [Smoother; 2],
    cool_since: [Option<Instant>; 2],
    last: Option<Instant>,
}

impl TemperatureTarget {
    /// `ceilings`: current CPU/GPU ceilings (restored on stop); `cpu_floor`: allowed minimum.
    pub fn new(targets: (u8, u8), temps: (f32, f32), ceilings: (u32, u32), cpu_floor: u32) -> Self {
        let t = [f32::from(targets.0), f32::from(targets.1)];
        let m = [temps.0, temps.1];
        Self {
            targets: t,
            ceilings: [ceilings.0, ceilings.1],
            floors: [cpu_floor, GPU_FLOOR_MHZ],
            limits: [ceilings.0, ceilings.1],
            smoothers: [Smoother::new(fan_target(m[0], t[0]), m[0]), Smoother::new(fan_target(m[1], t[1]), m[1])],
            cool_since: [None, None],
            last: None,
        }
    }

    pub fn initial_duty(&self) -> (u8, u8) {
        (self.smoothers[0].duty(), self.smoothers[1].duty())
    }

    pub fn limits(&self) -> (u32, u32) {
        (self.limits[0], self.limits[1])
    }

    pub fn ceilings(&self) -> (u32, u32) {
        (self.ceilings[0], self.ceilings[1])
    }

    /// `None` while the 2 s interval has not elapsed. `kernel_override` is the
    /// r9t_fan `thermal` flag (both fans already forced to 100 %).
    pub fn step(&mut self, temps: (f32, f32), kernel_override: bool, now: Instant) -> Option<Result<TargetStep, TargetError>> {
        if let Some(last) = self.last {
            let gap = now.saturating_duration_since(last);
            if gap < MIN_INTERVAL {
                return None;
            }
            if gap > MAX_GAP {
                return Some(Err(TargetError::Interrupted));
            }
        }
        self.last = Some(now);
        let m = [temps.0, temps.1];
        let mut duty = [0u8; 2];
        for i in 0..2 {
            duty[i] = self.smoothers[i].update(fan_target(m[i], self.targets[i]), m[i], now);
        }
        if kernel_override {
            duty = [100, 100];
            self.smoothers = [Smoother::new(100, m[0]), Smoother::new(100, m[1])];
        }
        let mut changed = [None, None];
        for i in 0..2 {
            let mut desired = self.limits[i];
            if m[i] > self.targets[i] {
                self.cool_since[i] = None;
                if duty[i] == 100 {
                    desired = self.limits[i].saturating_sub(STEP_MHZ).max(self.floors[i]);
                }
            } else if m[i] <= self.targets[i] - 3.0 {
                let since = *self.cool_since[i].get_or_insert(now);
                if now.duration_since(since) >= RAISE_HOLD {
                    desired = (self.limits[i] + STEP_MHZ).min(self.ceilings[i]);
                }
            } else {
                self.cool_since[i] = None;
            }
            if desired != self.limits[i] {
                self.limits[i] = desired;
                changed[i] = Some(desired);
            }
        }
        Some(Ok(TargetStep { duty: (duty[0], duty[1]), cpu_limit: changed[0], gpu_limit: changed[1] }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fan_formula_matches_legacy() {
        assert_eq!(fan_target(70.0, 75.0), 75);
        assert_eq!(fan_target(60.0, 75.0), 50);
        assert_eq!(fan_target(80.0, 75.0), 100);
        assert_eq!(fan_target(74.0, 75.0), 95);
        assert!(validate_targets(75, 72).is_ok());
        assert!(validate_targets(86, 72).is_err() && validate_targets(75, 59).is_err());
    }

    #[test]
    fn hot_at_full_fan_steps_frequency_down_to_floor() {
        let t0 = Instant::now();
        let s = |n| t0 + Duration::from_secs(n);
        let mut tt = TemperatureTarget::new((75, 72), (85.0, 70.0), (3500, 2475), 3300);
        assert_eq!(tt.initial_duty().0, 100);
        let step = tt.step((85.0, 70.0), false, s(0)).unwrap().unwrap();
        assert_eq!((step.duty.0, step.cpu_limit, step.gpu_limit), (100, Some(3400), None));
        assert!(tt.step((85.0, 70.0), false, s(1)).is_none(), "2 s minimum interval");
        assert_eq!(tt.step((85.0, 70.0), false, s(2)).unwrap().unwrap().cpu_limit, Some(3300));
        assert_eq!(tt.step((85.0, 70.0), false, s(4)).unwrap().unwrap().cpu_limit, None, "floor reached");
    }

    #[test]
    fn cool_for_10s_raises_back_to_ceiling() {
        let t0 = Instant::now();
        let s = |n| t0 + Duration::from_secs(n);
        let mut tt = TemperatureTarget::new((75, 72), (85.0, 60.0), (3500, 2475), 1000);
        tt.step((85.0, 60.0), false, s(0));
        assert_eq!(tt.limits().0, 3400);
        // 72 °C is exactly target − 3.
        tt.step((72.0, 60.0), false, s(2));
        assert_eq!(tt.step((72.0, 60.0), false, s(10)).unwrap().unwrap().cpu_limit, None);
        assert_eq!(tt.step((72.0, 60.0), false, s(12)).unwrap().unwrap().cpu_limit, Some(3500));
        assert_eq!(tt.step((72.0, 60.0), false, s(14)).unwrap().unwrap().cpu_limit, None, "ceiling");
    }

    #[test]
    fn gap_aborts_and_kernel_override_forces_full() {
        let t0 = Instant::now();
        let mut tt = TemperatureTarget::new((75, 72), (70.0, 60.0), (3500, 2475), 1000);
        tt.step((70.0, 60.0), false, t0);
        let s = tt.step((70.0, 60.0), true, t0 + Duration::from_secs(2)).unwrap().unwrap();
        assert_eq!(s.duty, (100, 100));
        assert_eq!(tt.step((70.0, 60.0), false, t0 + Duration::from_secs(13)), Some(Err(TargetError::Interrupted)));
    }
}
