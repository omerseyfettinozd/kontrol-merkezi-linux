//! NVIDIA dGPU via NVML loaded at runtime (`libnvidia-ml.so.1`).
//!
//! Gate: `runtime_status` is read from sysfs before *every* NVML call, including
//! init. When it is not `active`, NVML is not touched and the counter does not move
//! (Faz 2 acceptance e).
//!
//! `NVML_INIT_FLAG_NO_ATTACH` cannot be used: with driver 615 no device handle is
//! returned afterwards (`NOT_FOUND`). Instead the session is held open only while a
//! display is connected to the dGPU (it cannot runtime-suspend then anyway, reads
//! cost ~0.01 ms); otherwise every access is init → read → shutdown (~13 ms) so an
//! open NVML session never keeps an idle dGPU awake.
//!
//! Preflight: an `nvmlInit` that fails early in boot (error 999) keeps failing for the
//! life of the process; only a new process recovers (measured 2026-10-21 on two boots).
//! With a preflight set, the first in-process init waits until a child process manages
//! one.

use std::ffi::{c_char, c_int, c_uint, c_ulonglong, c_void, CString};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::{HalError, Result, Sysfs};

pub const PCI_BUS_ID: &str = "0000:01:00.0";
const NVML_SUCCESS: c_int = 0;
const NVML_ERROR_INSUFFICIENT_SIZE: c_int = 7;
const CLOCK_GRAPHICS: c_int = 0;
const CLOCK_MEM: c_int = 2;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GpuSample {
    pub temp_c: Option<u32>,
    pub power_w: Option<f32>,
    pub enforced_limit_w: Option<f32>,
    pub clock_graphics_mhz: Option<u32>,
    pub clock_memory_mhz: Option<u32>,
    pub max_clock_mhz: Option<u32>,
    pub util_gpu_pct: Option<u8>,
    pub util_memory_pct: Option<u8>,
    pub vram_used_mib: Option<u32>,
    pub vram_total_mib: Option<u32>,
    pub pstate: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessKind {
    Graphics,
    Compute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuProcess {
    pub pid: u32,
    pub kind: ProcessKind,
    pub used_mib: Option<u64>,
}

pub trait Gpu: Send + Sync {
    /// sysfs `power/runtime_status` (`active`, `suspended`, ...).
    fn runtime_status(&self) -> Result<String>;
    fn sample(&self) -> Result<GpuSample>;
    /// Temperature, power and utilisation only (3 NVML calls, ~0.3 ms CPU).
    fn sample_fast(&self) -> Result<GpuSample>;
    fn processes(&self) -> Result<Vec<GpuProcess>>;
    fn set_locked_clocks(&self, max_mhz: u32) -> Result<()>;
    fn reset_locked_clocks(&self) -> Result<()>;
    /// NVML entry points invoked so far (for the no-wake guarantee).
    fn nvml_calls(&self) -> u64;
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Utilization {
    gpu: c_uint,
    memory: c_uint,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Memory {
    total: c_ulonglong,
    free: c_ulonglong,
    used: c_ulonglong,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct ProcessInfo {
    pid: c_uint,
    used_gpu_memory: c_ulonglong,
    gpu_instance_id: c_uint,
    compute_instance_id: c_uint,
}

type Device = *mut c_void;

struct Api {
    _lib: libloading::Library,
    init: unsafe extern "C" fn(c_uint) -> c_int,
    shutdown: unsafe extern "C" fn() -> c_int,
    handle_by_pci: unsafe extern "C" fn(*const c_char, *mut Device) -> c_int,
    temperature: unsafe extern "C" fn(Device, c_int, *mut c_uint) -> c_int,
    power_usage: unsafe extern "C" fn(Device, *mut c_uint) -> c_int,
    enforced_limit: unsafe extern "C" fn(Device, *mut c_uint) -> c_int,
    clock_info: unsafe extern "C" fn(Device, c_int, *mut c_uint) -> c_int,
    max_clock_info: unsafe extern "C" fn(Device, c_int, *mut c_uint) -> c_int,
    utilization: unsafe extern "C" fn(Device, *mut Utilization) -> c_int,
    memory: unsafe extern "C" fn(Device, *mut Memory) -> c_int,
    pstate: unsafe extern "C" fn(Device, *mut c_int) -> c_int,
    set_locked: unsafe extern "C" fn(Device, c_uint, c_uint) -> c_int,
    reset_locked: unsafe extern "C" fn(Device) -> c_int,
    graphics_procs: unsafe extern "C" fn(Device, *mut c_uint, *mut ProcessInfo) -> c_int,
    compute_procs: unsafe extern "C" fn(Device, *mut c_uint, *mut ProcessInfo) -> c_int,
}

// SAFETY: NVML is thread-safe per NVIDIA's documentation; the raw handle is only
// used while holding `Nvml::state`'s mutex.
unsafe impl Send for Api {}

impl Api {
    fn load() -> Result<Self> {
        // SAFETY: loading the vendor library runs its constructors; this is the same
        // library the NVIDIA driver stack loads in every process using NVML.
        let lib = unsafe { libloading::Library::new("libnvidia-ml.so.1") }
            .map_err(|e| HalError::Unsupported(format!("libnvidia-ml.so.1: {e}")))?;
        macro_rules! sym {
            ($name:literal) => {
                // SAFETY: the declared signature matches the NVML header for this symbol.
                *unsafe { lib.get($name) }.map_err(|e| HalError::Unsupported(format!("NVML sembolü: {e}")))?
            };
        }
        Ok(Self {
            init: sym!(b"nvmlInitWithFlags\0"),
            shutdown: sym!(b"nvmlShutdown\0"),
            handle_by_pci: sym!(b"nvmlDeviceGetHandleByPciBusId_v2\0"),
            temperature: sym!(b"nvmlDeviceGetTemperature\0"),
            power_usage: sym!(b"nvmlDeviceGetPowerUsage\0"),
            enforced_limit: sym!(b"nvmlDeviceGetEnforcedPowerLimit\0"),
            clock_info: sym!(b"nvmlDeviceGetClockInfo\0"),
            max_clock_info: sym!(b"nvmlDeviceGetMaxClockInfo\0"),
            utilization: sym!(b"nvmlDeviceGetUtilizationRates\0"),
            memory: sym!(b"nvmlDeviceGetMemoryInfo\0"),
            pstate: sym!(b"nvmlDeviceGetPerformanceState\0"),
            set_locked: sym!(b"nvmlDeviceSetGpuLockedClocks\0"),
            reset_locked: sym!(b"nvmlDeviceResetGpuLockedClocks\0"),
            graphics_procs: sym!(b"nvmlDeviceGetGraphicsRunningProcesses_v3\0"),
            compute_procs: sym!(b"nvmlDeviceGetComputeRunningProcesses_v3\0"),
            _lib: lib,
        })
    }
}

struct State {
    api: Option<Api>,
    device: Device,
    preflight_ok: bool,
    last_preflight: Option<Instant>,
}

/// Out-of-process NVML check; `true` once a child process initialized NVML.
pub type Preflight = Box<dyn Fn() -> bool + Send + Sync>;

const PREFLIGHT_EVERY: Duration = Duration::from_secs(10);

// SAFETY: the NVML device handle is an opaque token valid process-wide; it is only
// dereferenced by NVML itself and only used while holding the `Nvml::state` mutex.
unsafe impl Send for State {}

pub struct Nvml {
    fs: Sysfs,
    state: Mutex<State>,
    calls: AtomicU64,
    preflight: Option<Preflight>,
}

/// True when a connected DRM connector belongs to the dGPU (e.g. `card1-eDP-1`).
pub fn display_on_dgpu(fs: &Sysfs) -> bool {
    fs.list("/sys/class/drm").iter().filter(|n| n.starts_with("card") && n.contains('-')).any(|n| {
        fs.read(format!("/sys/class/drm/{n}/status")).is_ok_and(|s| s == "connected")
            && fs
                .resolve(format!("/sys/class/drm/{n}/device/device"))
                .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
                .is_some_and(|f| f == PCI_BUS_ID)
    })
}

impl Nvml {
    pub fn new(fs: Sysfs) -> Self {
        Self {
            fs,
            state: Mutex::new(State { api: None, device: std::ptr::null_mut(), preflight_ok: false, last_preflight: None }),
            calls: AtomicU64::new(0),
            preflight: None,
        }
    }

    pub fn with_preflight(mut self, preflight: Preflight) -> Self {
        self.preflight = Some(preflight);
        self
    }

    /// Whether the NVML session stays open between samples (see module docs).
    pub fn holds_session(&self) -> bool {
        display_on_dgpu(&self.fs)
    }

    fn check(&self, code: c_int, what: &str) -> Result<()> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if code == NVML_SUCCESS { Ok(()) } else { Err(HalError::Verify(format!("NVML {what}: hata {code}"))) }
    }

    /// Runs `f` with an initialized device, only while the dGPU is active.
    fn with_device<T>(&self, f: impl FnOnce(&Api, Device) -> Result<T>) -> Result<T> {
        let status = self.runtime_status()?;
        if status != "active" {
            return Err(HalError::GpuNotActive(status));
        }
        let mut st = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if st.api.is_none() {
            if let (false, Some(preflight)) = (st.preflight_ok, &self.preflight) {
                if st.last_preflight.is_some_and(|t| t.elapsed() < PREFLIGHT_EVERY) {
                    return Err(HalError::Verify("NVML henüz hazır değil".into()));
                }
                st.last_preflight = Some(Instant::now());
                if !preflight() {
                    return Err(HalError::Verify("NVML henüz hazır değil (alt süreç denemesi başarısız)".into()));
                }
                st.preflight_ok = true;
            }
            let api = Api::load()?;
            // SAFETY: plain C call with a flags integer.
            self.check(unsafe { (api.init)(0) }, "init")?;
            let bus = CString::new(format!("0000{PCI_BUS_ID}")).expect("no NUL");
            let mut dev: Device = std::ptr::null_mut();
            // SAFETY: `bus` is NUL-terminated and `dev` is a valid out-pointer.
            if let Err(e) = self.check(unsafe { (api.handle_by_pci)(bus.as_ptr(), &mut dev) }, "handle") {
                // SAFETY: balanced with the successful init above.
                unsafe { (api.shutdown)() };
                return Err(e);
            }
            st.device = dev;
            st.api = Some(api);
        }
        let api = st.api.as_ref().expect("initialized above");
        let result = f(api, st.device);
        if !self.holds_session() {
            // SAFETY: balanced with init; the handle is discarded with the session.
            unsafe { (api.shutdown)() };
            self.calls.fetch_add(1, Ordering::Relaxed);
            st.api = None;
            st.device = std::ptr::null_mut();
        }
        result
    }

    fn read_u32(&self, f: unsafe extern "C" fn(Device, *mut c_uint) -> c_int, dev: Device, what: &str) -> Option<u32> {
        let mut v: c_uint = 0;
        // SAFETY: `dev` came from nvmlDeviceGetHandleByPciBusId_v2; `v` is a valid out-pointer.
        self.check(unsafe { f(dev, &mut v) }, what).ok().map(|_| v)
    }

    fn procs(&self, api: &Api, dev: Device, kind: ProcessKind) -> Result<Vec<GpuProcess>> {
        let f = match kind {
            ProcessKind::Graphics => api.graphics_procs,
            ProcessKind::Compute => api.compute_procs,
        };
        let mut count: c_uint = 64;
        let mut buf = vec![ProcessInfo::default(); count as usize];
        // SAFETY: `buf` has room for `count` entries, as NVML requires.
        let mut rc = unsafe { f(dev, &mut count, buf.as_mut_ptr()) };
        if rc == NVML_ERROR_INSUFFICIENT_SIZE {
            buf.resize(count as usize + 16, ProcessInfo::default());
            count = buf.len() as c_uint;
            // SAFETY: as above, with the larger buffer.
            rc = unsafe { f(dev, &mut count, buf.as_mut_ptr()) };
        }
        self.check(rc, "processes")?;
        Ok(buf[..count as usize]
            .iter()
            .map(|p| GpuProcess {
                pid: p.pid,
                kind,
                // NVML reports "not available" as all-ones.
                used_mib: (p.used_gpu_memory != u64::MAX).then_some(p.used_gpu_memory / (1024 * 1024)),
            })
            .collect())
    }
}

impl Gpu for Nvml {
    fn runtime_status(&self) -> Result<String> {
        self.fs.read(format!("/sys/bus/pci/devices/{PCI_BUS_ID}/power/runtime_status"))
    }

    fn sample(&self) -> Result<GpuSample> {
        self.with_device(|api, dev| {
            let mut s = GpuSample::default();
            let mut t: c_uint = 0;
            // SAFETY: valid device and out-pointer; 0 = NVML_TEMPERATURE_GPU.
            s.temp_c = self.check(unsafe { (api.temperature)(dev, 0, &mut t) }, "temp").ok().map(|_| t);
            s.power_w = self.read_u32(api.power_usage, dev, "power").map(|mw| mw as f32 / 1000.0);
            s.enforced_limit_w = self.read_u32(api.enforced_limit, dev, "limit").map(|mw| mw as f32 / 1000.0);
            for (ty, slot) in [(CLOCK_GRAPHICS, &mut s.clock_graphics_mhz), (CLOCK_MEM, &mut s.clock_memory_mhz)] {
                let mut c: c_uint = 0;
                // SAFETY: valid device, clock type and out-pointer.
                *slot = self.check(unsafe { (api.clock_info)(dev, ty, &mut c) }, "clock").ok().map(|_| c);
            }
            let mut c: c_uint = 0;
            // SAFETY: as above.
            s.max_clock_mhz = self.check(unsafe { (api.max_clock_info)(dev, CLOCK_GRAPHICS, &mut c) }, "maxclock").ok().map(|_| c);
            let mut u = Utilization::default();
            // SAFETY: `u` matches nvmlUtilization_t.
            if self.check(unsafe { (api.utilization)(dev, &mut u) }, "util").is_ok() {
                s.util_gpu_pct = u8::try_from(u.gpu).ok();
                s.util_memory_pct = u8::try_from(u.memory).ok();
            }
            let mut m = Memory::default();
            // SAFETY: `m` matches nvmlMemory_t (v1).
            if self.check(unsafe { (api.memory)(dev, &mut m) }, "memory").is_ok() {
                s.vram_used_mib = u32::try_from(m.used / (1024 * 1024)).ok();
                s.vram_total_mib = u32::try_from(m.total / (1024 * 1024)).ok();
            }
            let mut p: c_int = 0;
            // SAFETY: valid device and out-pointer.
            s.pstate = self.check(unsafe { (api.pstate)(dev, &mut p) }, "pstate").ok().and_then(|_| u8::try_from(p).ok()).filter(|p| *p <= 15);
            Ok(s)
        })
    }

    fn sample_fast(&self) -> Result<GpuSample> {
        self.with_device(|api, dev| {
            let mut s = GpuSample::default();
            let mut t: c_uint = 0;
            // SAFETY: valid device and out-pointer; 0 = NVML_TEMPERATURE_GPU.
            s.temp_c = self.check(unsafe { (api.temperature)(dev, 0, &mut t) }, "temp").ok().map(|_| t);
            s.power_w = self.read_u32(api.power_usage, dev, "power").map(|mw| mw as f32 / 1000.0);
            let mut u = Utilization::default();
            // SAFETY: `u` matches nvmlUtilization_t.
            if self.check(unsafe { (api.utilization)(dev, &mut u) }, "util").is_ok() {
                s.util_gpu_pct = u8::try_from(u.gpu).ok();
                s.util_memory_pct = u8::try_from(u.memory).ok();
            }
            Ok(s)
        })
    }

    fn processes(&self) -> Result<Vec<GpuProcess>> {
        self.with_device(|api, dev| {
            let mut all = self.procs(api, dev, ProcessKind::Graphics)?;
            all.extend(self.procs(api, dev, ProcessKind::Compute)?);
            Ok(all)
        })
    }

    fn set_locked_clocks(&self, max_mhz: u32) -> Result<()> {
        if !(600..=3090).contains(&max_mhz) {
            return Err(HalError::Invalid(format!("GPU sınırı 600–3090 MHz olmalı, {max_mhz} verildi")));
        }
        // SAFETY: valid device; min 0 lets the driver pick its floor.
        self.with_device(|api, dev| self.check(unsafe { (api.set_locked)(dev, 0, max_mhz) }, "set locked clocks"))
    }

    fn reset_locked_clocks(&self) -> Result<()> {
        // SAFETY: valid device.
        self.with_device(|api, dev| self.check(unsafe { (api.reset_locked)(dev) }, "reset locked clocks"))
    }

    fn nvml_calls(&self) -> u64 {
        self.calls.load(Ordering::Relaxed)
    }
}

pub mod fake {
    use super::*;

    pub struct FakeGpu {
        pub status: Mutex<String>,
        pub sample: GpuSample,
        pub locked: Mutex<Option<u32>>,
        pub calls: AtomicU64,
    }

    impl Default for FakeGpu {
        fn default() -> Self {
            Self {
                status: Mutex::new("active".into()),
                sample: GpuSample { temp_c: Some(60), power_w: Some(20.0), enforced_limit_w: Some(105.0), ..Default::default() },
                locked: Mutex::new(None),
                calls: AtomicU64::new(0),
            }
        }
    }

    impl FakeGpu {
        fn gate(&self) -> Result<()> {
            let s = self.status.lock().unwrap().clone();
            if s != "active" {
                return Err(HalError::GpuNotActive(s));
            }
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    impl Gpu for FakeGpu {
        fn runtime_status(&self) -> Result<String> {
            Ok(self.status.lock().unwrap().clone())
        }
        fn sample(&self) -> Result<GpuSample> {
            self.gate()?;
            Ok(self.sample.clone())
        }
        fn sample_fast(&self) -> Result<GpuSample> {
            self.gate()?;
            Ok(GpuSample { temp_c: self.sample.temp_c, power_w: self.sample.power_w, ..Default::default() })
        }
        fn processes(&self) -> Result<Vec<GpuProcess>> {
            self.gate()?;
            Ok(vec![])
        }
        fn set_locked_clocks(&self, max_mhz: u32) -> Result<()> {
            self.gate()?;
            *self.locked.lock().unwrap() = Some(max_mhz);
            Ok(())
        }
        fn reset_locked_clocks(&self) -> Result<()> {
            self.gate()?;
            *self.locked.lock().unwrap() = None;
            Ok(())
        }
        fn nvml_calls(&self) -> u64 {
            self.calls.load(Ordering::Relaxed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sysfs::fake::file;

    #[test]
    fn suspended_gpu_never_touches_nvml() {
        let dir = tempfile::tempdir().unwrap();
        file(dir.path(), &format!("/sys/bus/pci/devices/{PCI_BUS_ID}/power/runtime_status"), "suspended");
        let n = Nvml::new(Sysfs::new(dir.path()));
        assert!(matches!(n.sample(), Err(HalError::GpuNotActive(s)) if s == "suspended"));
        assert!(matches!(n.processes(), Err(HalError::GpuNotActive(_))));
        assert!(matches!(n.set_locked_clocks(2000), Err(HalError::GpuNotActive(_)) | Err(HalError::Invalid(_))));
        assert_eq!(n.nvml_calls(), 0);
        assert!(n.state.lock().unwrap().api.is_none(), "library not even loaded");
    }

    #[test]
    fn failed_preflight_keeps_nvml_out_of_process_and_is_rate_limited() {
        let dir = tempfile::tempdir().unwrap();
        file(dir.path(), &format!("/sys/bus/pci/devices/{PCI_BUS_ID}/power/runtime_status"), "active");
        let runs = std::sync::Arc::new(AtomicU64::new(0));
        let r = runs.clone();
        let n = Nvml::new(Sysfs::new(dir.path())).with_preflight(Box::new(move || {
            r.fetch_add(1, Ordering::Relaxed);
            false
        }));
        assert!(n.sample_fast().is_err());
        assert!(n.sample_fast().is_err());
        assert_eq!(runs.load(Ordering::Relaxed), 1, "second call inside PREFLIGHT_EVERY");
        assert!(n.state.lock().unwrap().api.is_none(), "library not loaded in-process");
    }

    #[test]
    fn missing_runtime_status_is_an_error_not_a_wake() {
        let dir = tempfile::tempdir().unwrap();
        let n = Nvml::new(Sysfs::new(dir.path()));
        assert!(matches!(n.sample(), Err(HalError::NotFound(_))));
        assert_eq!(n.nvml_calls(), 0);
    }

    #[test]
    fn locked_clock_range() {
        let g = fake::FakeGpu::default();
        assert!(Nvml::new(Sysfs::new("/nonexistent")).set_locked_clocks(5000).is_err());
        g.set_locked_clocks(2475).unwrap();
        assert_eq!(*g.locked.lock().unwrap(), Some(2475));
        *g.status.lock().unwrap() = "suspended".into();
        let before = g.nvml_calls();
        assert!(g.sample().is_err());
        assert_eq!(g.nvml_calls(), before);
    }

    #[test]
    fn display_detection_follows_connector_owner() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        let card = |n: u32, pci: &str| {
            file(r, &format!("/sys/devices/pci0000:00/{pci}/drm/card{n}/.keep"), "");
            crate::sysfs::fake::link(r, &format!("/sys/class/drm/card{n}/device"), &r.join(format!("sys/devices/pci0000:00/{pci}")).display().to_string());
        };
        card(0, "0000:07:00.0");
        card(1, PCI_BUS_ID);
        let conn = |name: &str, card: u32, status: &str| {
            file(r, &format!("/sys/class/drm/{name}/status"), status);
            crate::sysfs::fake::link(r, &format!("/sys/class/drm/{name}/device"), &r.join(format!("sys/class/drm/card{card}")).display().to_string());
        };
        conn("card0-DP-2", 0, "connected");
        conn("card1-eDP-1", 1, "disconnected");
        let fs = Sysfs::new(r);
        assert!(!display_on_dgpu(&fs), "iGPU display only: sample-and-close");
        file(r, "/sys/class/drm/card1-eDP-1/status", "connected");
        assert!(display_on_dgpu(&fs));
    }

    #[test]
    fn process_info_layout_matches_nvml_v2() {
        assert_eq!(std::mem::size_of::<ProcessInfo>(), 24);
        assert_eq!(std::mem::size_of::<Memory>(), 24);
    }
}
