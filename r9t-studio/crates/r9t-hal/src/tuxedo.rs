//! `/dev/tuxedo_io` fast EC path. Typestate enforces safety invariant 9: no ioctl
//! other than the hardware check is reachable before [`Unverified::verify`] succeeds.
//! (tuxedo-drivers 4.24.0 can oops if `uw_feats` is used before `R_HWCHECK_UW`.)

use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::PathBuf;

use crate::{HalError, Result, Sysfs};

const GROUP_HWCHECK: u8 = 0xEC;
const GROUP_READ: u8 = 0xEF;
const GROUP_WRITE: u8 = 0xF0;
const NR_HWCHECK_UW: u8 = 0x06;
const NR_INTERFACE: u8 = 0x00;
const NR_MODEL_ID: u8 = 0x01;
const NR_CPU_DUTY: u8 = 0x10;
const NR_GPU_DUTY: u8 = 0x11;
const NR_CPU_TEMP: u8 = 0x12;
const NR_GPU_TEMP: u8 = 0x13;
const NR_MODE: u8 = 0x14;
const NR_FAN_AUTO: u8 = 0x14;
pub const EXPECTED_MODEL_ID: i32 = 0x1A;

/// `_IOR(group, nr, void*)` as used by tuxedo_io (size = pointer size).
pub const fn ior(group: u8, nr: u8) -> u64 {
    (2u64 << 30) | ((std::mem::size_of::<usize>() as u64) << 16) | ((group as u64) << 8) | nr as u64
}

/// `_IO(group, nr)`.
pub const fn io(group: u8, nr: u8) -> u64 {
    ((group as u64) << 8) | nr as u64
}

/// Raw device access; real implementation issues ioctls, fakes record calls.
pub trait IoctlDevice: Send {
    fn read_int(&mut self, request: u64) -> std::io::Result<i32>;
    fn read_bytes(&mut self, request: u64, buf: &mut [u8]) -> std::io::Result<()>;
    fn call(&mut self, request: u64) -> std::io::Result<()>;
}

pub struct RealDevice {
    file: File,
}

impl RealDevice {
    pub fn open(fs: &Sysfs, writable: bool) -> Result<Self> {
        let path: PathBuf = fs.path("/dev/tuxedo_io");
        let file = std::fs::OpenOptions::new().read(true).write(writable).open(&path).map_err(|e| HalError::io(path, e))?;
        Ok(Self { file })
    }

    fn ioctl(&self, request: u64, arg: *mut u8) -> std::io::Result<()> {
        // SAFETY: `arg` points to a buffer of at least 4 bytes (or 32 for the
        // interface string) owned by the caller for the duration of the call; the
        // driver copies at most that much (copy_to_user of u32 / fixed string).
        let rc = unsafe { nix::libc::ioctl(self.file.as_raw_fd(), request as _, arg) };
        if rc < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
    }
}

impl IoctlDevice for RealDevice {
    fn read_int(&mut self, request: u64) -> std::io::Result<i32> {
        let mut v = [0u8; 8];
        self.ioctl(request, v.as_mut_ptr())?;
        Ok(i32::from_le_bytes([v[0], v[1], v[2], v[3]]))
    }

    fn read_bytes(&mut self, request: u64, buf: &mut [u8]) -> std::io::Result<()> {
        self.ioctl(request, buf.as_mut_ptr())
    }

    fn call(&mut self, request: u64) -> std::io::Result<()> {
        self.ioctl(request, std::ptr::null_mut())
    }
}

/// Opened but not yet hardware-checked. Only [`Unverified::verify`] is available.
pub struct Unverified<D: IoctlDevice>(D);

/// Hardware check, model id and interface confirmed.
pub struct Verified<D: IoctlDevice> {
    dev: D,
    pub model_id: i32,
    pub interface: String,
}

impl<D: IoctlDevice> Unverified<D> {
    pub fn new(dev: D) -> Self {
        Self(dev)
    }

    /// `R_HWCHECK_UW` must be 1, then model id 0x1A and an interface containing `uniwill_wmi`.
    pub fn verify(mut self) -> Result<Verified<D>> {
        let hw = self.0.read_int(ior(GROUP_HWCHECK, NR_HWCHECK_UW)).map_err(|e| HalError::io("/dev/tuxedo_io", e))?;
        if hw != 1 {
            return Err(HalError::Verify(format!("R_HWCHECK_UW={hw}; Uniwill arayüzü hazır değil")));
        }
        let model_id = self.0.read_int(ior(GROUP_READ, NR_MODEL_ID)).map_err(|e| HalError::io("/dev/tuxedo_io", e))?;
        if model_id != EXPECTED_MODEL_ID {
            return Err(HalError::Verify(format!("model kimliği 0x{model_id:02x}, 0x1a bekleniyor")));
        }
        let mut buf = [0u8; 32];
        self.0.read_bytes(ior(GROUP_READ, NR_INTERFACE), &mut buf).map_err(|e| HalError::io("/dev/tuxedo_io", e))?;
        let interface = String::from_utf8_lossy(buf.split(|&b| b == 0).next().unwrap_or_default()).into_owned();
        if !interface.contains("uniwill_wmi") {
            return Err(HalError::Verify(format!("EC arayüzü {interface:?}, uniwill_wmi bekleniyor")));
        }
        Ok(Verified { dev: self.0, model_id, interface })
    }
}

/// One fast-path sample: four EC reads (~57 ms each) plus the mode byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EcSample {
    /// `None` when the EC returned 0 or > 115 (reads are unchecked in the driver).
    pub cpu_temp: Option<u8>,
    pub gpu_temp: Option<u8>,
    /// Raw duty 0–200.
    pub cpu_duty: Option<u8>,
    pub gpu_duty: Option<u8>,
    pub mode: Option<u8>,
}

impl<D: IoctlDevice> Verified<D> {
    fn read(&mut self, nr: u8) -> Result<i32> {
        self.dev.read_int(ior(GROUP_READ, nr)).map_err(|e| HalError::io("/dev/tuxedo_io", e))
    }

    pub fn temps(&mut self) -> Result<(Option<u8>, Option<u8>)> {
        let valid = |v: i32| (1..=115).contains(&v).then_some(v as u8);
        Ok((valid(self.read(NR_CPU_TEMP)?), valid(self.read(NR_GPU_TEMP)?)))
    }

    pub fn duties(&mut self) -> Result<(Option<u8>, Option<u8>)> {
        let valid = |v: i32| (0..=200).contains(&v).then_some(v as u8);
        Ok((valid(self.read(NR_CPU_DUTY)?), valid(self.read(NR_GPU_DUTY)?)))
    }

    pub fn mode(&mut self) -> Result<Option<u8>> {
        let v = self.read(NR_MODE)?;
        Ok((0..=255).contains(&v).then_some(v as u8))
    }

    pub fn sample(&mut self) -> Result<EcSample> {
        let (cpu_temp, gpu_temp) = self.temps()?;
        let (cpu_duty, gpu_duty) = self.duties()?;
        Ok(EcSample { cpu_temp, gpu_temp, cpu_duty, gpu_duty, mode: self.mode()? })
    }

    /// Emergency only: `W_UW_FANAUTO` (0xF014). Needs a writable descriptor.
    pub fn emergency_fan_auto(&mut self) -> Result<()> {
        self.dev.call(io(GROUP_WRITE, NR_FAN_AUTO)).map_err(|e| HalError::io("/dev/tuxedo_io", e))
    }
}

pub mod fake {
    use super::*;
    use std::collections::HashMap;

    /// Records every request; values keyed by ioctl request number.
    #[derive(Default)]
    pub struct FakeDevice {
        pub values: HashMap<u64, i32>,
        pub interface: String,
        pub calls: Vec<u64>,
    }

    impl FakeDevice {
        pub fn r9t() -> Self {
            let mut v = HashMap::new();
            v.insert(ior(GROUP_HWCHECK, NR_HWCHECK_UW), 1);
            v.insert(ior(GROUP_READ, NR_MODEL_ID), EXPECTED_MODEL_ID);
            v.insert(ior(GROUP_READ, NR_CPU_TEMP), 66);
            v.insert(ior(GROUP_READ, NR_GPU_TEMP), 60);
            v.insert(ior(GROUP_READ, NR_CPU_DUTY), 106);
            v.insert(ior(GROUP_READ, NR_GPU_DUTY), 106);
            v.insert(ior(GROUP_READ, NR_MODE), 160);
            Self { values: v, interface: "uniwill_wmi".into(), calls: Vec::new() }
        }
    }

    impl IoctlDevice for FakeDevice {
        fn read_int(&mut self, request: u64) -> std::io::Result<i32> {
            self.calls.push(request);
            self.values.get(&request).copied().ok_or_else(|| std::io::Error::from_raw_os_error(nix::libc::EINVAL))
        }

        fn read_bytes(&mut self, request: u64, buf: &mut [u8]) -> std::io::Result<()> {
            self.calls.push(request);
            let b = self.interface.as_bytes();
            let n = b.len().min(buf.len());
            buf[..n].copy_from_slice(&b[..n]);
            Ok(())
        }

        fn call(&mut self, request: u64) -> std::io::Result<()> {
            self.calls.push(request);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeDevice;
    use super::*;

    #[test]
    fn ioctl_numbers_match_driver() {
        assert_eq!(ior(0xEC, 0x06), 0x8008EC06);
        assert_eq!(ior(0xEF, 0x10), 0x8008EF10);
        assert_eq!(ior(0xEF, 0x13), 0x8008EF13);
        assert_eq!(io(0xF0, 0x14), 0xF014);
    }

    #[test]
    fn hwcheck_is_always_the_first_call() {
        let v = Unverified::new(FakeDevice::r9t()).verify().unwrap();
        assert_eq!(v.dev.calls[0], 0x8008EC06);
        assert_eq!(v.model_id, 0x1A);
    }

    #[test]
    fn failed_hwcheck_stops_before_any_other_ioctl() {
        let mut d = FakeDevice::r9t();
        d.values.insert(ior(GROUP_HWCHECK, NR_HWCHECK_UW), 0);
        // Keep a handle on the call log through a raw pointer-free path: verify consumes the device,
        // so re-run with a device whose model read would fail if reached.
        d.values.remove(&ior(GROUP_READ, NR_MODEL_ID));
        let err = Unverified::new(d).verify().err().unwrap();
        assert!(matches!(err, HalError::Verify(m) if m.contains("HWCHECK")));
    }

    #[test]
    fn wrong_model_or_interface_is_refused() {
        let mut d = FakeDevice::r9t();
        d.values.insert(ior(GROUP_READ, NR_MODEL_ID), 0x13);
        assert!(Unverified::new(d).verify().is_err());
        let mut d = FakeDevice::r9t();
        d.interface = "clevo_acpi".into();
        assert!(Unverified::new(d).verify().is_err());
    }

    #[test]
    fn invalid_ec_values_become_none() {
        let mut d = FakeDevice::r9t();
        d.values.insert(ior(GROUP_READ, NR_CPU_TEMP), 0);
        d.values.insert(ior(GROUP_READ, NR_GPU_TEMP), 116);
        let mut v = Unverified::new(d).verify().unwrap();
        let s = v.sample().unwrap();
        assert_eq!((s.cpu_temp, s.gpu_temp, s.cpu_duty, s.mode), (None, None, Some(106), Some(160)));
    }
}
