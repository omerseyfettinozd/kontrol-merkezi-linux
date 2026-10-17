//! Hardware abstraction for the GameGaraj Slayer R9T.
//!
//! Every backend reads through [`Sysfs`], whose root defaults to `/` and can be
//! redirected with `R9T_SYSFS_ROOT` so tests run against a fake tree. Devices that
//! need ioctls (`/dev/tuxedo_io`) and NVML are behind traits with fakes.

pub mod cpufreq;
pub mod fan;
pub mod gpu;
pub mod hwmon;
pub mod power;
pub mod probe;
pub mod rapl;
pub mod rgb;
pub mod sysfs;
pub mod tuxedo;

pub use sysfs::Sysfs;

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HalError {
    #[error("izin yok: {0}")]
    Permission(PathBuf),
    #[error("bulunamadı: {0}")]
    NotFound(PathBuf),
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("geçersiz değer: {0}")]
    Invalid(String),
    #[error("doğrulanamadı: {0}")]
    Verify(String),
    /// dGPU is runtime-suspended; NVML was not touched.
    #[error("dGPU etkin değil (runtime_status={0}); NVML çağrılmadı")]
    GpuNotActive(String),
    #[error("desteklenmiyor: {0}")]
    Unsupported(String),
    #[error("meşgul: {0}")]
    Busy(String),
}

impl HalError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        let path = path.into();
        match source.kind() {
            std::io::ErrorKind::PermissionDenied => Self::Permission(path),
            std::io::ErrorKind::NotFound => Self::NotFound(path),
            _ => match source.raw_os_error() {
                Some(nix::libc::EBUSY) => Self::Busy(path.display().to_string()),
                _ => Self::Io { path, source },
            },
        }
    }

    pub fn is_permission(&self) -> bool {
        matches!(self, Self::Permission(_))
    }
}

pub type Result<T> = std::result::Result<T, HalError>;
