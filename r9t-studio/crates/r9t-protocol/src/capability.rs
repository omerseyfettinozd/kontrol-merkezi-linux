//! Capability probing results. A feature not proven on hardware is never `available`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityId {
    FanRead,
    FanControl,
    EcFastPath,
    CpuLimit,
    CpuEpp,
    CpuBoost,
    PowerProfile,
    GpuTelemetry,
    GpuLockedClocks,
    PackagePower,
    Rgb,
    RgbBuffered,
    FnLock,
    Camera,
    Battery,
    ChargeLimit,
    CpuPpt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    pub id: CapabilityId,
    pub available: bool,
    /// Why it is unavailable, or what limits it (empty when fully available).
    pub reason: String,
}

impl Capability {
    pub fn yes(id: CapabilityId) -> Self {
        Self { id, available: true, reason: String::new() }
    }

    pub fn no(id: CapabilityId, reason: impl Into<String>) -> Self {
        Self { id, available: false, reason: reason.into() }
    }
}
