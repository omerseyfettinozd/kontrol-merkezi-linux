//! `r9t_fan` kernel bridge: `/sys/module/r9t_fan/parameters/{status,control}`.

use crate::{HalError, Result, Sysfs};

const STATUS: &str = "/sys/module/r9t_fan/parameters/status";
const CONTROL: &str = "/sys/module/r9t_fan/parameters/control";
const MODULE: &str = "/sys/module/r9t_fan";

/// Parsed `status` line. Field set is exactly the v1 format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FanStatus {
    /// 0 auto, 1 manual, 2 boost.
    pub mode: i32,
    pub cpu_target: u32,
    pub gpu_target: u32,
    pub cpu_rpm: i32,
    pub gpu_rpm: i32,
    pub cpu_temp: u32,
    pub gpu_temp: u32,
    pub mode_byte: u32,
    /// EC duty 0–200.
    pub cpu_pwm: u32,
    pub gpu_pwm: u32,
    pub lease_ms: i32,
    pub error: i32,
    pub thermal: bool,
}

impl FanStatus {
    pub const KEYS: [&'static str; 13] = [
        "mode", "cpu_target", "gpu_target", "cpu_rpm", "gpu_rpm", "cpu_temp", "gpu_temp", "mode_byte",
        "cpu_pwm", "gpu_pwm", "lease_ms", "error", "thermal",
    ];

    pub fn parse(line: &str) -> Result<Self> {
        let mut values = std::collections::BTreeMap::new();
        for pair in line.split_whitespace() {
            let (k, v) = pair.split_once('=').ok_or_else(|| HalError::Invalid(format!("status: {pair}")))?;
            let v: i64 = v.parse().map_err(|_| HalError::Invalid(format!("status: {pair}")))?;
            if values.insert(k, v).is_some() {
                return Err(HalError::Invalid(format!("status: yinelenen {k}")));
            }
        }
        if values.len() != Self::KEYS.len() || !Self::KEYS.iter().all(|k| values.contains_key(k)) {
            return Err(HalError::Invalid(format!("status alan kümesi beklenmiyor: {line}")));
        }
        let g = |k: &str| values[k];
        let u = |k: &str| u32::try_from(g(k)).map_err(|_| HalError::Invalid(format!("status {k}={}", g(k))));
        let i = |k: &str| i32::try_from(g(k)).map_err(|_| HalError::Invalid(format!("status {k}={}", g(k))));
        Ok(Self {
            mode: i("mode")?,
            cpu_target: u("cpu_target")?,
            gpu_target: u("gpu_target")?,
            cpu_rpm: i("cpu_rpm")?,
            gpu_rpm: i("gpu_rpm")?,
            cpu_temp: u("cpu_temp")?,
            gpu_temp: u("gpu_temp")?,
            mode_byte: u("mode_byte")?,
            cpu_pwm: u("cpu_pwm")?,
            gpu_pwm: u("gpu_pwm")?,
            lease_ms: i("lease_ms")?,
            error: i("error")?,
            thermal: g("thermal") != 0,
        })
    }

    /// Firmware is in control: driver mode 0 and the EC manual bit (0x40) clear.
    pub fn is_auto(&self) -> bool {
        self.mode == 0 && self.mode_byte & 0x40 == 0
    }

    /// EC temperatures within 1–115 °C (invalid readings are reported as `None`).
    pub fn valid_temps(&self) -> Option<(u32, u32)> {
        let ok = |t: u32| (1..=115).contains(&t);
        (ok(self.cpu_temp) && ok(self.gpu_temp)).then_some((self.cpu_temp, self.gpu_temp))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanCommand {
    Auto,
    Boost,
    /// Percent, 50–100 on v1/0.2.0.
    Manual { cpu: u8, gpu: u8 },
}

impl FanCommand {
    pub fn wire(&self) -> String {
        match self {
            Self::Auto => "auto".into(),
            Self::Boost => "boost".into(),
            Self::Manual { cpu, gpu } => format!("manual {cpu} {gpu}"),
        }
    }
}

pub trait FanBridge: Send + Sync {
    fn status(&self) -> Result<FanStatus>;
    fn control(&self, cmd: FanCommand) -> Result<()>;
    fn loaded(&self) -> bool;
    /// `MODULE_VERSION` (absent on 0.1.0).
    fn version(&self) -> Option<String>;
    fn srcversion(&self) -> Option<String>;
}

pub struct SysfsFan {
    fs: Sysfs,
}

impl SysfsFan {
    pub fn new(fs: Sysfs) -> Self {
        Self { fs }
    }
}

impl FanBridge for SysfsFan {
    fn status(&self) -> Result<FanStatus> {
        // `status` returns -ERANGE when the EC reports implausible values.
        FanStatus::parse(&self.fs.read(STATUS)?)
    }

    fn control(&self, cmd: FanCommand) -> Result<()> {
        if let FanCommand::Manual { cpu, gpu } = cmd {
            if !(50..=100).contains(&cpu) || !(50..=100).contains(&gpu) {
                return Err(HalError::Invalid(format!("manual {cpu} {gpu}: %50–100 olmalı")));
            }
        }
        self.fs.write(CONTROL, &cmd.wire())
    }

    fn loaded(&self) -> bool {
        self.fs.exists(MODULE)
    }

    fn version(&self) -> Option<String> {
        self.fs.read(format!("{MODULE}/version")).ok()
    }

    fn srcversion(&self) -> Option<String> {
        self.fs.read(format!("{MODULE}/srcversion")).ok()
    }
}

/// In-memory bridge for tests, modelling the 0.2.0 lease and command rules.
pub mod fake {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug)]
    pub struct State {
        pub status: FanStatus,
        pub commands: Vec<FanCommand>,
        pub fail_status: Option<i32>,
        pub suspended: bool,
    }

    pub struct FakeFan(pub Mutex<State>);

    impl Default for FakeFan {
        fn default() -> Self {
            Self(Mutex::new(State {
                status: FanStatus::parse(
                    "mode=0 cpu_target=100 gpu_target=100 cpu_rpm=3200 gpu_rpm=3230 cpu_temp=60 gpu_temp=55 \
                     mode_byte=160 cpu_pwm=106 gpu_pwm=106 lease_ms=0 error=0 thermal=0",
                )
                .unwrap(),
                commands: Vec::new(),
                fail_status: None,
                suspended: false,
            }))
        }
    }

    impl FakeFan {
        pub fn set_temps(&self, cpu: u32, gpu: u32) {
            let mut s = self.0.lock().unwrap();
            s.status.cpu_temp = cpu;
            s.status.gpu_temp = gpu;
            s.status.thermal = s.status.mode != 0 && (cpu >= 95 || gpu >= 87);
        }

        /// Simulates lease expiry in the kernel worker.
        pub fn expire_lease(&self) {
            let mut s = self.0.lock().unwrap();
            if s.status.mode != 0 {
                s.status.mode = 0;
                s.status.mode_byte &= !0x40;
                s.status.error = -110;
                s.status.lease_ms = 0;
            }
        }

        pub fn commands(&self) -> Vec<FanCommand> {
            self.0.lock().unwrap().commands.clone()
        }
    }

    impl FanBridge for FakeFan {
        fn status(&self) -> Result<FanStatus> {
            let s = self.0.lock().unwrap();
            match s.fail_status {
                Some(errno) => Err(HalError::io(STATUS, std::io::Error::from_raw_os_error(errno))),
                None => Ok(s.status),
            }
        }

        fn control(&self, cmd: FanCommand) -> Result<()> {
            let mut s = self.0.lock().unwrap();
            if s.suspended {
                return Err(HalError::Busy("uyku".into()));
            }
            s.commands.push(cmd);
            let st = &mut s.status;
            match cmd {
                FanCommand::Auto => {
                    st.mode = 0;
                    st.mode_byte &= !0x40;
                    st.lease_ms = 0;
                }
                FanCommand::Boost => {
                    st.mode = 2;
                    st.mode_byte |= 0x40;
                    st.cpu_pwm = 200;
                    st.gpu_pwm = 200;
                    st.lease_ms = 15000;
                }
                FanCommand::Manual { cpu, gpu } => {
                    if !(50..=100).contains(&cpu) || !(50..=100).contains(&gpu) {
                        return Err(HalError::Invalid("manual".into()));
                    }
                    st.mode = 1;
                    st.mode_byte |= 0x40;
                    st.cpu_target = cpu.into();
                    st.gpu_target = gpu.into();
                    let override_ = st.cpu_temp >= 95 || st.gpu_temp >= 87;
                    st.thermal = override_;
                    st.cpu_pwm = if override_ { 200 } else { u32::from(cpu) * 2 };
                    st.gpu_pwm = if override_ { 200 } else { u32::from(gpu) * 2 };
                    st.lease_ms = 15000;
                    st.error = 0;
                }
            }
            Ok(())
        }

        fn loaded(&self) -> bool {
            true
        }

        fn version(&self) -> Option<String> {
            Some("0.2.0".into())
        }

        fn srcversion(&self) -> Option<String> {
            Some("FAKE".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sysfs::fake::file;

    const LIVE: &str = "mode=0 cpu_target=100 gpu_target=100 cpu_rpm=3019 gpu_rpm=3084 cpu_temp=66 gpu_temp=60 \
                        mode_byte=128 cpu_pwm=95 gpu_pwm=95 lease_ms=0 error=0 thermal=0";

    #[test]
    fn parses_live_status_line() {
        let s = FanStatus::parse(LIVE).unwrap();
        assert!(s.is_auto());
        assert_eq!((s.cpu_rpm, s.gpu_pwm, s.valid_temps()), (3019, 95, Some((66, 60))));
        let s = FanStatus::parse(&LIVE.replace("mode_byte=128", "mode_byte=192").replace("mode=0", "mode=1")).unwrap();
        assert!(!s.is_auto());
        assert_eq!(FanStatus::parse(&LIVE.replace("error=0", "error=-110")).unwrap().error, -110);
    }

    #[test]
    fn rejects_changed_status_format() {
        assert!(FanStatus::parse(&format!("{LIVE} extra=1")).is_err());
        assert!(FanStatus::parse(&LIVE.replace(" thermal=0", "")).is_err());
        assert!(FanStatus::parse(&LIVE.replace("mode=0", "mode=x")).is_err());
        assert!(FanStatus::parse(&format!("{LIVE} mode=1")).is_err());
        assert!(FanStatus::parse(&LIVE.replace("cpu_temp=66", "cpu_temp=-1")).is_err());
    }

    #[test]
    fn invalid_temperatures_are_not_used() {
        let s = FanStatus::parse(&LIVE.replace("cpu_temp=66", "cpu_temp=0")).unwrap();
        assert_eq!(s.valid_temps(), None);
        let s = FanStatus::parse(&LIVE.replace("gpu_temp=60", "gpu_temp=116")).unwrap();
        assert_eq!(s.valid_temps(), None);
    }

    #[test]
    fn sysfs_bridge_reads_and_writes() {
        let dir = tempfile::tempdir().unwrap();
        file(dir.path(), STATUS, LIVE);
        file(dir.path(), CONTROL, "");
        file(dir.path(), "/sys/module/r9t_fan/version", "0.2.0\n");
        let fan = SysfsFan::new(Sysfs::new(dir.path()));
        assert_eq!(fan.status().unwrap().cpu_temp, 66);
        fan.control(FanCommand::Manual { cpu: 60, gpu: 70 }).unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join("sys/module/r9t_fan/parameters/control")).unwrap(), "manual 60 70");
        assert!(fan.control(FanCommand::Manual { cpu: 49, gpu: 70 }).is_err());
        assert_eq!(fan.version().as_deref(), Some("0.2.0"));
    }

    #[test]
    fn fake_models_lease_and_override() {
        let fan = fake::FakeFan::default();
        fan.control(FanCommand::Manual { cpu: 60, gpu: 60 }).unwrap();
        assert_eq!(fan.status().unwrap().cpu_pwm, 120);
        fan.expire_lease();
        let s = fan.status().unwrap();
        assert!(s.is_auto());
        assert_eq!(s.error, -110);
        fan.set_temps(96, 60);
        fan.control(FanCommand::Manual { cpu: 60, gpu: 60 }).unwrap();
        let s = fan.status().unwrap();
        assert!(s.thermal && s.cpu_pwm == 200);
    }
}
