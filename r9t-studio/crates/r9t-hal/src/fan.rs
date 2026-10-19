//! `r9t_fan` kernel bridge: `/sys/module/r9t_fan/parameters/{status,control}`.

use crate::{HalError, Result, Sysfs};

const STATUS: &str = "/sys/module/r9t_fan/parameters/status";
const CONTROL: &str = "/sys/module/r9t_fan/parameters/control";
const MODULE: &str = "/sys/module/r9t_fan";
const STATUS2: &str = "/sys/module/r9t_fan/parameters/status2";

/// `MODULE_VERSION`s whose `status`/`control` contract is known. 0.3.0 keeps the v1
/// `status` line and the 50–100 % commands by default, and adds `status2`, hwmon and
/// the low-duty parameters (`min_duty`, `allow_low_duty`). Anything else is refused.
pub const SUPPORTED_VERSIONS: [&str; 2] = ["0.2.0", "0.3.0"];

pub fn supported_version(version: Option<&str>) -> bool {
    version.is_some_and(|v| SUPPORTED_VERSIONS.contains(&v))
}

/// Refusal text when the bridge version is unknown.
pub fn version_refusal(version: Option<&str>) -> String {
    format!("r9t_fan {} gerekli, yüklü: {}", SUPPORTED_VERSIONS.join(" ya da "), version.unwrap_or("yok"))
}

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

/// Parsed 0.3.0 `status2`: the v1 fields plus cache and floor state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FanStatus2 {
    pub v1: FanStatus,
    pub sample_age_ms: u32,
    pub cache_ms: u32,
    pub allow_low_duty: bool,
    /// Lowest manual duty (%) the module accepts without `allow_low_duty`.
    pub min_duty: u32,
    /// Floors (%) currently applied, after the temperature ramp.
    pub cpu_floor: u32,
    pub gpu_floor: u32,
    /// In-kernel stall fallbacks since load.
    pub stalls: u32,
}

impl FanStatus2 {
    pub const EXTRA: [&'static str; 7] = ["sample_age_ms", "cache_ms", "allow_low_duty", "min_duty", "cpu_floor", "gpu_floor", "stalls"];

    pub fn parse(line: &str) -> Result<Self> {
        let (mut v1, mut extra) = (Vec::new(), std::collections::BTreeMap::new());
        for pair in line.split_whitespace() {
            let (k, v) = pair.split_once('=').ok_or_else(|| HalError::Invalid(format!("status2: {pair}")))?;
            if Self::EXTRA.contains(&k) {
                let v: u32 = v.parse().map_err(|_| HalError::Invalid(format!("status2: {pair}")))?;
                if extra.insert(k, v).is_some() {
                    return Err(HalError::Invalid(format!("status2: yinelenen {k}")));
                }
            } else {
                v1.push(pair);
            }
        }
        if extra.len() != Self::EXTRA.len() {
            return Err(HalError::Invalid(format!("status2 alan kümesi beklenmiyor: {line}")));
        }
        let g = |k: &str| extra[k];
        Ok(Self {
            v1: FanStatus::parse(&v1.join(" "))?,
            sample_age_ms: g("sample_age_ms"),
            cache_ms: g("cache_ms"),
            allow_low_duty: g("allow_low_duty") != 0,
            min_duty: g("min_duty"),
            cpu_floor: g("cpu_floor"),
            gpu_floor: g("gpu_floor"),
            stalls: g("stalls"),
        })
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
    /// Lowest manual duty the loaded module accepts: `min_duty` on 0.3.0, else 50.
    fn duty_floor(&self) -> u8 {
        50
    }
}

/// `min_duty` parameter text → floor; anything unexpected keeps the 0.2.0 floor.
pub fn parse_floor(text: Option<&str>) -> u8 {
    text.and_then(|t| t.trim().parse::<u8>().ok()).filter(|d| (r9t_protocol::profile::DUTY_FLOOR..=50).contains(d)).unwrap_or(50)
}

pub struct SysfsFan {
    fs: Sysfs,
}

impl SysfsFan {
    pub fn new(fs: Sysfs) -> Self {
        Self { fs }
    }

    /// 0.3.0 only.
    pub fn status2(&self) -> Result<FanStatus2> {
        FanStatus2::parse(&self.fs.read(STATUS2)?)
    }

    /// Low-duty sweep only (`r9tctl fan-sweep`): `allow_low_duty` must be on and the
    /// module enforces 20–100 %, its temperature floor and stall detection. r9td never
    /// calls this; its commands go through [`FanBridge::control`] (50–100 %).
    pub fn control_low(&self, cpu: u8, gpu: u8) -> Result<()> {
        if !(20..=100).contains(&cpu) || !(20..=100).contains(&gpu) {
            return Err(HalError::Invalid(format!("manual {cpu} {gpu}: %20–100 olmalı")));
        }
        self.fs.write(CONTROL, &FanCommand::Manual { cpu, gpu }.wire())
    }

    pub fn set_allow_low_duty(&self, on: bool) -> Result<()> {
        self.fs.write(format!("{MODULE}/parameters/allow_low_duty"), if on { "1" } else { "0" })
    }
}

impl FanBridge for SysfsFan {
    fn status(&self) -> Result<FanStatus> {
        // `status` returns -ERANGE when the EC reports implausible values.
        FanStatus::parse(&self.fs.read(STATUS)?)
    }

    fn control(&self, cmd: FanCommand) -> Result<()> {
        if let FanCommand::Manual { cpu, gpu } = cmd {
            let floor = self.duty_floor();
            if !(floor..=100).contains(&cpu) || !(floor..=100).contains(&gpu) {
                return Err(HalError::Invalid(format!("manual {cpu} {gpu}: %{floor}–100 olmalı")));
            }
        }
        self.fs.write(CONTROL, &cmd.wire())
    }

    fn duty_floor(&self) -> u8 {
        parse_floor(self.fs.read(format!("{MODULE}/parameters/min_duty")).ok().as_deref())
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
        pub min_duty: u8,
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
                min_duty: 50,
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
            let floor = s.min_duty;
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
                    if !(floor..=100).contains(&cpu) || !(floor..=100).contains(&gpu) {
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

        fn duty_floor(&self) -> u8 {
            self.0.lock().unwrap().min_duty
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
    fn parses_status2_and_rejects_unknown_versions() {
        let line = format!("{LIVE} sample_age_ms=412 cache_ms=1000 allow_low_duty=0 min_duty=50 cpu_floor=50 gpu_floor=50 stalls=0");
        let s = FanStatus2::parse(&line).unwrap();
        assert_eq!((s.v1.cpu_rpm, s.sample_age_ms, s.min_duty, s.allow_low_duty), (3019, 412, 50, false));
        assert!(FanStatus2::parse(LIVE).is_err(), "v1 line alone is not status2");
        assert!(FanStatus2::parse(&format!("{line} stalls=1")).is_err());
        assert!(FanStatus::parse(&line).is_err(), "status keeps its strict v1 key set");
        assert!(supported_version(Some("0.2.0")) && supported_version(Some("0.3.0")));
        assert!(!supported_version(Some("0.1.0")) && !supported_version(Some("0.4.0")) && !supported_version(None));
        assert_eq!(version_refusal(None), "r9t_fan 0.2.0 ya da 0.3.0 gerekli, yüklü: yok");
    }

    #[test]
    fn floor_follows_min_duty() {
        let dir = tempfile::tempdir().unwrap();
        file(dir.path(), CONTROL, "");
        let fan = SysfsFan::new(Sysfs::new(dir.path()));
        assert_eq!(fan.duty_floor(), 50, "0.2.0: no min_duty parameter");
        assert!(fan.control(FanCommand::Manual { cpu: 30, gpu: 60 }).is_err());
        file(dir.path(), "/sys/module/r9t_fan/parameters/min_duty", "25\n");
        assert_eq!(fan.duty_floor(), 25);
        fan.control(FanCommand::Manual { cpu: 25, gpu: 60 }).unwrap();
        assert!(fan.control(FanCommand::Manual { cpu: 24, gpu: 60 }).is_err());
        assert_eq!((parse_floor(Some("20")), parse_floor(Some("51")), parse_floor(Some("x")), parse_floor(None)), (50, 50, 50, 50));
    }

    #[test]
    fn low_duty_control_is_separate_and_bounded() {
        let dir = tempfile::tempdir().unwrap();
        file(dir.path(), CONTROL, "");
        file(dir.path(), "/sys/module/r9t_fan/parameters/allow_low_duty", "N\n");
        let fan = SysfsFan::new(Sysfs::new(dir.path()));
        assert!(fan.control(FanCommand::Manual { cpu: 30, gpu: 60 }).is_err(), "r9td path stays 50–100");
        fan.set_allow_low_duty(true).unwrap();
        fan.control_low(30, 60).unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join("sys/module/r9t_fan/parameters/control")).unwrap(), "manual 30 60");
        assert!(fan.control_low(19, 60).is_err());
        assert_eq!(std::fs::read_to_string(dir.path().join("sys/module/r9t_fan/parameters/allow_low_duty")).unwrap(), "1");
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
