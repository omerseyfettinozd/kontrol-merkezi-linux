//! Live state written by the sampler and read by RPC handlers. Every value keeps the
//! instant it was sampled so a snapshot can report per-field ages.

use std::time::Instant;

use r9t_protocol::snapshot::{
    AuthoritySnapshot, AuthorityState, BatterySnapshot, CpuSnapshot, FanMode, FanSnapshot, Field, GpuSnapshot,
    PowerSnapshot, SensorReading, Snapshot, UsbPdSnapshot,
};

#[derive(Debug, Clone)]
pub struct Timed<T> {
    pub value: Option<T>,
    pub at: Option<Instant>,
}

impl<T> Default for Timed<T> {
    fn default() -> Self {
        Self { value: None, at: None }
    }
}

impl<T: Clone> Timed<T> {
    /// Records a sample attempt; a failed read stores `None` (never a fake 0).
    pub fn set(&mut self, value: Option<T>, now: Instant) {
        self.value = value;
        self.at = Some(now);
    }

    pub fn field(&self, now: Instant) -> Field<T> {
        Field {
            value: self.value.clone(),
            age_ms: self.at.map(|t| now.saturating_duration_since(t).as_millis() as u64),
        }
    }

    /// Value if sampled within `max_ms`.
    pub fn fresh(&self, now: Instant, max_ms: u64) -> Option<T> {
        match (&self.value, self.at) {
            (Some(v), Some(t)) if now.saturating_duration_since(t).as_millis() as u64 <= max_ms => Some(v.clone()),
            _ => None,
        }
    }
}

/// USB-C PD source: name, online, voltage V, current A.
pub type UsbPdTimed = (String, Timed<bool>, Timed<f32>, Timed<f32>);
/// hwmon sensor: id, chip, label, °C.
pub type SensorTimed = (String, String, String, Timed<f32>);

#[derive(Debug, Default, Clone)]
pub struct Live {
    pub cpu_ec_c: Timed<f32>,
    pub gpu_ec_c: Timed<f32>,
    pub tctl_c: Timed<f32>,
    pub ccd_c: Vec<Timed<f32>>,
    pub package_w: Timed<f32>,
    pub apu_ppt_w: Timed<f32>,
    pub cpu_freq_mhz: Timed<u32>,
    pub cpu_max_mhz: Timed<u32>,
    pub boost: Timed<bool>,
    pub epp: Timed<String>,

    pub gpu_runtime: Timed<String>,
    pub gpu_temp_c: Timed<f32>,
    pub gpu_power_w: Timed<f32>,
    pub gpu_limit_w: Timed<f32>,
    pub gpu_clock_mhz: Timed<u32>,
    pub gpu_mem_clock_mhz: Timed<u32>,
    pub gpu_max_clock_mhz: Timed<u32>,
    pub gpu_util: Timed<u8>,
    pub gpu_mem_util: Timed<u8>,
    pub vram_used: Timed<u32>,
    pub vram_total: Timed<u32>,
    pub pstate: Timed<u8>,
    pub gpu_locked_mhz: Timed<u32>,

    pub fan_mode: Timed<FanMode>,
    pub cpu_duty: Timed<u8>,
    pub gpu_duty: Timed<u8>,
    pub cpu_rpm: Timed<u32>,
    pub gpu_rpm: Timed<u32>,
    pub lease_ms: Timed<u32>,
    pub kernel_error: Timed<i32>,
    pub thermal_override: Timed<bool>,
    pub mode_byte: Timed<u8>,

    pub ac_online: Timed<bool>,
    pub profile: Timed<String>,
    pub bat_percent: Timed<u8>,
    pub bat_status: Timed<String>,
    pub bat_w: Timed<f32>,
    pub bat_wh: Timed<f32>,
    pub bat_full_wh: Timed<f32>,
    pub bat_design_wh: Timed<f32>,
    pub bat_v: Timed<f32>,
    pub usb_pd: Vec<UsbPdTimed>,

    pub sensors: Vec<SensorTimed>,
    pub rgb_brightness: Timed<u8>,
    pub fn_lock: Timed<bool>,

    pub sequence: u64,
    pub authority: Option<AuthoritySnapshot>,
}

impl Live {
    pub fn snapshot(&self, now: Instant, unix_ms: u64) -> Snapshot {
        Snapshot {
            taken_at_ms: unix_ms,
            sequence: self.sequence,
            cpu: CpuSnapshot {
                ec_temp_c: self.cpu_ec_c.field(now),
                tctl_c: self.tctl_c.field(now),
                ccd_temp_c: self.ccd_c.iter().map(|t| t.field(now)).collect(),
                package_w: self.package_w.field(now),
                apu_ppt_w: self.apu_ppt_w.field(now),
                freq_mhz: self.cpu_freq_mhz.field(now),
                max_freq_mhz: self.cpu_max_mhz.field(now),
                boost: self.boost.field(now),
                epp: self.epp.field(now),
            },
            gpu: GpuSnapshot {
                runtime_status: self.gpu_runtime.field(now),
                ec_temp_c: self.gpu_ec_c.field(now),
                temp_c: self.gpu_temp_c.field(now),
                power_w: self.gpu_power_w.field(now),
                enforced_limit_w: self.gpu_limit_w.field(now),
                clock_graphics_mhz: self.gpu_clock_mhz.field(now),
                clock_memory_mhz: self.gpu_mem_clock_mhz.field(now),
                max_clock_mhz: self.gpu_max_clock_mhz.field(now),
                util_gpu_pct: self.gpu_util.field(now),
                util_memory_pct: self.gpu_mem_util.field(now),
                vram_used_mib: self.vram_used.field(now),
                vram_total_mib: self.vram_total.field(now),
                pstate: self.pstate.field(now),
                locked_clock_mhz: self.gpu_locked_mhz.field(now),
            },
            fans: FanSnapshot {
                mode: self.fan_mode.field(now),
                cpu_duty_pct: self.cpu_duty.field(now),
                gpu_duty_pct: self.gpu_duty.field(now),
                cpu_rpm: self.cpu_rpm.field(now),
                gpu_rpm: self.gpu_rpm.field(now),
                lease_ms: self.lease_ms.field(now),
                kernel_error: self.kernel_error.field(now),
                thermal_override: self.thermal_override.field(now),
                mode_byte: self.mode_byte.field(now),
            },
            power: PowerSnapshot {
                ac_online: self.ac_online.field(now),
                profile: self.profile.field(now),
                battery: BatterySnapshot {
                    percent: self.bat_percent.field(now),
                    status: self.bat_status.field(now),
                    power_w: self.bat_w.field(now),
                    energy_wh: self.bat_wh.field(now),
                    full_wh: self.bat_full_wh.field(now),
                    design_wh: self.bat_design_wh.field(now),
                    voltage_v: self.bat_v.field(now),
                },
                usb_pd: self
                    .usb_pd
                    .iter()
                    .map(|(n, o, v, c)| UsbPdSnapshot { name: n.clone(), online: o.field(now), voltage_v: v.field(now), current_a: c.field(now) })
                    .collect(),
            },
            sensors: self
                .sensors
                .iter()
                .map(|(id, chip, label, v)| SensorReading { id: id.clone(), chip: chip.clone(), label: label.clone(), value_c: v.field(now) })
                .collect(),
            rgb_brightness: self.rgb_brightness.field(now),
            fn_lock: self.fn_lock.field(now),
            authority: self.authority.clone().unwrap_or(AuthoritySnapshot {
                state: AuthorityState::Shadow,
                reason: "salt okunur gölge modu".into(),
                active_profile: None,
                guard_active: false,
                session: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn ages_and_failed_reads() {
        let t0 = Instant::now();
        let mut live = Live::default();
        live.cpu_ec_c.set(Some(70.0), t0);
        live.gpu_ec_c.set(None, t0);
        let s = live.snapshot(t0 + Duration::from_millis(1500), 0);
        assert_eq!(s.cpu.ec_temp_c, Field { value: Some(70.0), age_ms: Some(1500) });
        assert_eq!(s.gpu.ec_temp_c, Field { value: None, age_ms: Some(1500) });
        assert_eq!(s.cpu.tctl_c, Field { value: None, age_ms: None });
        assert_eq!(live.cpu_ec_c.fresh(t0 + Duration::from_secs(6), 5000), None);
    }
}
