//! Single thermal authority: decides the fan command from the active mode and the
//! latest EC sample, with the user-space guard applied first.

pub mod guard;
pub mod smoother;
pub mod stall;
pub mod target;

use std::time::{Duration, Instant};

use r9t_hal::fan::FanCommand;
use r9t_protocol::profile::FanCurve;

use guard::Guard;
use smoother::Smoother;
use target::{TargetError, TemperatureTarget};

/// Missing EC temperatures for this long while r9td owns the fans → `auto`.
pub const INVALID_SENSOR_LIMIT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub enum Mode {
    /// Firmware tables; r9td writes nothing (except the guard).
    Auto,
    Boost,
    Manual { cpu: u8, gpu: u8 },
    Curve { curve: FanCurve, smoothers: Option<[Smoother; 2]> },
    Target(Box<TemperatureTarget>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Leave the hardware alone (already in firmware control).
    Idle,
    /// Write this command (also renews the 15 s kernel lease).
    Write(FanCommand),
    /// Hand control back to the EC and leave the current mode.
    FallbackAuto(String),
    /// Temperature target produced new frequency ceilings.
    WriteWithLimits { cmd: FanCommand, cpu_mhz: Option<u32>, gpu_mhz: Option<u32> },
}

#[derive(Debug)]
pub struct Authority {
    pub mode: Mode,
    pub guard: Guard,
    invalid_since: Option<Instant>,
    last_duty: Option<(u8, u8)>,
    /// The loaded module's lowest duty (`FanBridge::duty_floor`); every manual command
    /// is raised to it. 50 until the control path reads the module.
    pub duty_floor: u8,
}

impl Default for Authority {
    fn default() -> Self {
        Self { mode: Mode::Auto, guard: Guard::default(), invalid_since: None, last_duty: None, duty_floor: 50 }
    }
}

/// Manual command raised to the module floor `f`.
fn manual(f: u8, d: (u8, u8)) -> FanCommand {
    FanCommand::Manual { cpu: d.0.clamp(f, 100), gpu: d.1.clamp(f, 100) }
}

impl Authority {
    pub fn with_floor(duty_floor: u8) -> Self {
        Self { duty_floor, ..Self::default() }
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.invalid_since = None;
        self.last_duty = None;
    }

    /// One control tick with the newest EC temperatures (`None` = invalid/missing)
    /// and the kernel thermal-override flag.
    pub fn decide(&mut self, cpu_c: Option<f32>, gpu_c: Option<f32>, kernel_override: bool, now: Instant) -> Decision {
        if self.guard.update(cpu_c, gpu_c, now) {
            return Decision::Write(FanCommand::Boost);
        }
        let temps = match (cpu_c, gpu_c) {
            (Some(c), Some(g)) => {
                self.invalid_since = None;
                Some((c, g))
            }
            _ => None,
        };
        let floor = self.duty_floor;
        match &mut self.mode {
            Mode::Auto => Decision::Idle,
            Mode::Boost => Decision::Write(FanCommand::Boost),
            Mode::Manual { cpu, gpu } => Decision::Write(manual(floor, (*cpu, *gpu))),
            Mode::Curve { .. } | Mode::Target(_) if temps.is_none() => {
                // Invariant 8: never lower duty on a null reading; give up after 10 s.
                let since = *self.invalid_since.get_or_insert(now);
                if now.duration_since(since) >= INVALID_SENSOR_LIMIT {
                    self.mode = Mode::Auto;
                    return Decision::FallbackAuto("EC sıcaklığı 10 sn okunamadı".into());
                }
                match self.last_duty {
                    Some(d) => Decision::Write(manual(floor, d)),
                    None => Decision::Write(FanCommand::Boost),
                }
            }
            Mode::Curve { curve, smoothers } => {
                let (c, g) = temps.expect("checked above");
                let s = smoothers.get_or_insert_with(|| [Smoother::new(curve.duty_at(c), c), Smoother::new(curve.duty_at(g), g)]);
                let mut d = (s[0].update(curve.duty_at(c), c, now), s[1].update(curve.duty_at(g), g, now));
                if kernel_override {
                    d = (100, 100);
                    *s = [Smoother::new(100, c), Smoother::new(100, g)];
                }
                self.last_duty = Some(d);
                Decision::Write(manual(floor, d))
            }
            Mode::Target(tt) => {
                let (c, g) = temps.expect("checked above");
                match tt.step((c, g), kernel_override, now) {
                    None => match self.last_duty {
                        Some(d) => Decision::Write(manual(floor, d)),
                        None => Decision::Write(manual(floor, tt.initial_duty())),
                    },
                    Some(Ok(step)) => {
                        self.last_duty = Some(step.duty);
                        Decision::WriteWithLimits { cmd: manual(floor, step.duty), cpu_mhz: step.cpu_limit, gpu_mhz: step.gpu_limit }
                    }
                    Some(Err(TargetError::Interrupted)) => {
                        self.mode = Mode::Auto;
                        Decision::FallbackAuto("Sıcaklık hedefi kesildi (uyku ya da gecikme)".into())
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_raised_to_the_module_floor() {
        let t0 = Instant::now();
        let mut a = Authority::default();
        a.set_mode(Mode::Manual { cpu: 30, gpu: 60 });
        assert_eq!(a.decide(Some(60.0), Some(55.0), false, t0), Decision::Write(FanCommand::Manual { cpu: 50, gpu: 60 }), "0.2.0 floor");
        let mut a = Authority::with_floor(25);
        a.set_mode(Mode::Manual { cpu: 30, gpu: 20 });
        assert_eq!(a.decide(Some(60.0), Some(55.0), false, t0), Decision::Write(FanCommand::Manual { cpu: 30, gpu: 25 }), "min_duty=25");
        a.set_mode(Mode::Curve { curve: FanCurve::linear(&[[45, 25], [85, 100]]), smoothers: None });
        assert_eq!(a.decide(Some(40.0), Some(40.0), false, t0), Decision::Write(FanCommand::Manual { cpu: 25, gpu: 25 }));
    }

    fn s(t0: Instant, n: u64) -> Instant {
        t0 + Duration::from_secs(n)
    }

    #[test]
    fn guard_overrides_every_mode() {
        let t0 = Instant::now();
        let mut a = Authority::default();
        assert_eq!(a.decide(Some(70.0), Some(60.0), false, t0), Decision::Idle);
        assert_eq!(a.decide(Some(91.0), Some(60.0), false, s(t0, 1)), Decision::Write(FanCommand::Boost));
        a.set_mode(Mode::Manual { cpu: 50, gpu: 50 });
        assert_eq!(a.decide(Some(80.0), Some(60.0), false, s(t0, 2)), Decision::Write(FanCommand::Boost));
        assert_eq!(a.decide(Some(80.0), Some(60.0), false, s(t0, 12)), Decision::Write(FanCommand::Manual { cpu: 50, gpu: 50 }));
    }

    #[test]
    fn curve_mode_uses_smoothed_duty_per_fan() {
        let t0 = Instant::now();
        let mut a = Authority::default();
        a.set_mode(Mode::Curve { curve: FanCurve::linear(&[[45, 60], [60, 75], [75, 90], [85, 100]]), smoothers: None });
        assert_eq!(a.decide(Some(75.0), Some(52.0), false, t0), Decision::Write(FanCommand::Manual { cpu: 90, gpu: 67 }));
        // Cooling: held for 6 s.
        assert_eq!(a.decide(Some(60.0), Some(52.0), false, s(t0, 3)), Decision::Write(FanCommand::Manual { cpu: 90, gpu: 67 }));
        assert_eq!(a.decide(Some(60.0), Some(52.0), false, s(t0, 9)), Decision::Write(FanCommand::Manual { cpu: 88, gpu: 67 }));
        assert_eq!(a.decide(Some(60.0), Some(52.0), true, s(t0, 10)), Decision::Write(FanCommand::Manual { cpu: 100, gpu: 100 }));
    }

    #[test]
    fn null_sensor_keeps_duty_then_falls_back() {
        let t0 = Instant::now();
        let mut a = Authority::default();
        a.set_mode(Mode::Curve { curve: FanCurve::linear(&[[45, 60], [85, 100]]), smoothers: None });
        let first = a.decide(Some(70.0), Some(60.0), false, t0);
        assert_eq!(a.decide(None, Some(60.0), false, s(t0, 1)), first, "duty kept, not lowered");
        assert_eq!(a.decide(None, Some(60.0), false, s(t0, 9)), first);
        assert!(matches!(a.decide(None, Some(60.0), false, s(t0, 11)), Decision::FallbackAuto(_)));
        assert!(matches!(a.mode, Mode::Auto));
    }

    #[test]
    fn target_mode_emits_limits_and_aborts_on_gap() {
        let t0 = Instant::now();
        let mut a = Authority::default();
        a.set_mode(Mode::Target(Box::new(TemperatureTarget::new((75, 72), (86.0, 70.0), (3500, 2475), 1000))));
        assert_eq!(
            a.decide(Some(86.0), Some(70.0), false, t0),
            Decision::WriteWithLimits { cmd: FanCommand::Manual { cpu: 100, gpu: 90 }, cpu_mhz: Some(3400), gpu_mhz: None }
        );
        // Within 2 s: repeat last duty (lease renewal) without new limits.
        assert_eq!(a.decide(Some(86.0), Some(70.0), false, s(t0, 1)), Decision::Write(FanCommand::Manual { cpu: 100, gpu: 90 }));
        assert!(matches!(a.decide(Some(80.0), Some(70.0), false, s(t0, 15)), Decision::FallbackAuto(_)));
    }
}
