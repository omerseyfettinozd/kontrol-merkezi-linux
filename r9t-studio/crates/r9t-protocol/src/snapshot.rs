//! Cached hardware state. Every field carries its own age; an unreadable or
//! invalid source is `value: null`, never a fake 0 °C (safety invariant 8).

use serde::{Deserialize, Serialize};

/// A sampled value and how old it was when the snapshot was served.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field<T> {
    pub value: Option<T>,
    /// Milliseconds since the sample was taken; `None` when never sampled.
    pub age_ms: Option<u64>,
}

impl<T> Default for Field<T> {
    fn default() -> Self {
        Self { value: None, age_ms: None }
    }
}

impl<T> Field<T> {
    pub fn fresh(value: T) -> Self {
        Self { value: Some(value), age_ms: Some(0) }
    }

    pub fn missing() -> Self {
        Self::default()
    }

    /// Value only if it is younger than `max_age_ms`.
    pub fn fresh_value(&self, max_age_ms: u64) -> Option<&T> {
        match (&self.value, self.age_ms) {
            (Some(v), Some(age)) if age <= max_age_ms => Some(v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanMode {
    Auto,
    Manual,
    Boost,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FanSnapshot {
    pub mode: Field<FanMode>,
    /// Duty in percent (EC raw 0–200 halved).
    pub cpu_duty_pct: Field<u8>,
    pub gpu_duty_pct: Field<u8>,
    pub cpu_rpm: Field<u32>,
    pub gpu_rpm: Field<u32>,
    pub lease_ms: Field<u32>,
    /// Last kernel error (e.g. -110 lease expiry), 0 = none.
    pub kernel_error: Field<i32>,
    pub thermal_override: Field<bool>,
    pub mode_byte: Field<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CpuSnapshot {
    /// EC temperature, used for every thermal decision.
    pub ec_temp_c: Field<f32>,
    /// k10temp Tctl, display only.
    pub tctl_c: Field<f32>,
    pub ccd_temp_c: Vec<Field<f32>>,
    /// RAPL package-0 energy delta.
    pub package_w: Field<f32>,
    /// amdgpu `power1_input`: APU package PPT, not iGPU power.
    pub apu_ppt_w: Field<f32>,
    pub freq_mhz: Field<u32>,
    pub max_freq_mhz: Field<u32>,
    pub boost: Field<bool>,
    pub epp: Field<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GpuSnapshot {
    /// PCI runtime PM status; NVML is only touched while this is "active".
    pub runtime_status: Field<String>,
    pub ec_temp_c: Field<f32>,
    pub temp_c: Field<f32>,
    pub power_w: Field<f32>,
    pub enforced_limit_w: Field<f32>,
    pub clock_graphics_mhz: Field<u32>,
    pub clock_memory_mhz: Field<u32>,
    pub max_clock_mhz: Field<u32>,
    pub util_gpu_pct: Field<u8>,
    pub util_memory_pct: Field<u8>,
    pub vram_used_mib: Field<u32>,
    pub vram_total_mib: Field<u32>,
    pub pstate: Field<u8>,
    /// Locked-clock ceiling requested by r9td; no independent read-back exists.
    pub locked_clock_mhz: Field<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatterySnapshot {
    pub percent: Field<u8>,
    pub status: Field<String>,
    /// Positive while charging, negative while discharging.
    pub power_w: Field<f32>,
    pub energy_wh: Field<f32>,
    pub full_wh: Field<f32>,
    pub design_wh: Field<f32>,
    pub voltage_v: Field<f32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsbPdSnapshot {
    pub name: String,
    pub online: Field<bool>,
    pub voltage_v: Field<f32>,
    pub current_a: Field<f32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerSnapshot {
    pub ac_online: Field<bool>,
    pub profile: Field<String>,
    pub battery: BatterySnapshot,
    pub usb_pd: Vec<UsbPdSnapshot>,
}

/// One labelled temperature from hwmon, matched by chip name + label + device,
/// never by hwmon index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensorReading {
    /// Stable id, e.g. `nvme:0000:02:00.0:Composite`.
    pub id: String,
    pub chip: String,
    pub label: String,
    pub value_c: Field<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityState {
    /// Read-only shadow; the legacy service owns the hardware.
    Shadow,
    /// r9td is the single writer.
    Control,
    /// Control path refused (e.g. r9t_fan is not 0.2.0, legacy socket answered).
    Refused,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoritySnapshot {
    pub state: AuthorityState,
    pub reason: String,
    pub active_profile: Option<String>,
    pub guard_active: bool,
    /// Registered r9t-session, e.g. `hyprland:3`.
    pub session: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    /// Unix time in milliseconds when the snapshot was assembled.
    pub taken_at_ms: u64,
    pub sequence: u64,
    pub cpu: CpuSnapshot,
    pub gpu: GpuSnapshot,
    pub fans: FanSnapshot,
    pub power: PowerSnapshot,
    pub sensors: Vec<SensorReading>,
    pub rgb_brightness: Field<u8>,
    pub fn_lock: Field<bool>,
    /// Internal camera USB authorisation (`true` = enabled). Absent from older daemons.
    #[serde(default)]
    pub camera: Field<bool>,
    pub authority: AuthoritySnapshot,
}

/// Compact sample kept in the 300 s ring buffer and sent by `history`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryPoint {
    pub t_ms: u64,
    pub cpu_c: Option<f32>,
    pub gpu_c: Option<f32>,
    pub cpu_w: Option<f32>,
    pub gpu_w: Option<f32>,
    pub cpu_rpm: Option<u32>,
    pub gpu_rpm: Option<u32>,
    pub battery_w: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_values_serialize_as_null_not_zero() {
        let f: Field<f32> = Field::missing();
        assert_eq!(serde_json::to_string(&f).unwrap(), r#"{"value":null,"age_ms":null}"#);
        let cpu = CpuSnapshot::default();
        let json = serde_json::to_value(&cpu).unwrap();
        assert!(json["ec_temp_c"]["value"].is_null());
    }

    #[test]
    fn snapshot_without_camera_from_older_daemon_parses() {
        let snap = Snapshot {
            taken_at_ms: 1,
            sequence: 2,
            cpu: CpuSnapshot::default(),
            gpu: GpuSnapshot::default(),
            fans: FanSnapshot::default(),
            power: PowerSnapshot::default(),
            sensors: vec![],
            rgb_brightness: Field::missing(),
            fn_lock: Field::missing(),
            camera: Field { value: Some(true), age_ms: Some(0) },
            authority: AuthoritySnapshot {
                state: AuthorityState::Shadow,
                reason: String::new(),
                active_profile: None,
                guard_active: false,
                session: None,
            },
        };
        let mut json = serde_json::to_value(&snap).unwrap();
        assert_eq!(json["camera"]["value"], true);
        json.as_object_mut().unwrap().remove("camera");
        let old: Snapshot = serde_json::from_value(json).unwrap();
        assert_eq!(old.camera, Field::missing());
    }

    #[test]
    fn stale_values_are_not_fresh() {
        let f = Field { value: Some(70.0f32), age_ms: Some(6000) };
        assert!(f.fresh_value(5000).is_none());
        assert_eq!(f.fresh_value(6000), Some(&70.0));
    }
}
