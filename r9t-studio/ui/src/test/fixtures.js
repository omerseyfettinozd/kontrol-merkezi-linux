// Snapshot kurucuları: alan şekli crates/r9t-protocol/src/snapshot.rs ile aynı.
// `nullSnap` her sensörü geçersiz (null) verir: değişmez 8'in arayüzdeki karşılığı.
const F = (value = null, age_ms = value == null ? null : 200) => ({ value, age_ms });

export function snap({ empty = false, seq = 1, t = Date.now() } = {}) {
  const v = (x) => F(empty ? null : x);
  return {
    taken_at_ms: t,
    sequence: seq,
    cpu: {
      ec_temp_c: v(72), tctl_c: v(75.5), ccd_temp_c: empty ? [] : [F(70), F(71)],
      package_w: v(45.2), apu_ppt_w: v(54), freq_mhz: v(4200), max_freq_mhz: v(5300), boost: v(true), epp: v('balance_performance'),
    },
    gpu: {
      runtime_status: v('active'), ec_temp_c: v(66), temp_c: v(65), power_w: v(38.5), enforced_limit_w: v(115),
      clock_graphics_mhz: v(1800), clock_memory_mhz: v(8000), max_clock_mhz: v(3090), util_gpu_pct: v(40), util_memory_pct: v(20),
      vram_used_mib: v(1200), vram_total_mib: v(8192), pstate: v(2), locked_clock_mhz: F(null),
    },
    fans: {
      mode: v('auto'), cpu_duty_pct: v(45), gpu_duty_pct: v(40), cpu_rpm: v(3000), gpu_rpm: v(2900), lease_ms: v(12000),
      kernel_error: v(0), thermal_override: v(false), mode_byte: v(0),
    },
    power: {
      ac_online: v(true), profile: v('balanced'),
      battery: { percent: v(80), status: v('Charging'), power_w: v(12.5), energy_wh: v(60), full_wh: v(75), design_wh: v(80), voltage_v: v(16.8) },
      usb_pd: [],
    },
    sensors: empty ? [] : [{ id: 'nvme0', chip: 'nvme', label: 'Composite', value_c: F(48) }],
    rgb_brightness: v(30),
    fn_lock: v(false),
    camera: v(true),
    authority: { state: 'control', reason: '', active_profile: null, guard_active: false, session: null },
  };
}

/** probe okuması başarısız: r9t-hal probe.rs `val()` düz "unavailable: ..." metni koyar. */
export const unavailableCaps = {
  mode: 'control',
  probe: {
    readings: { cpu: { limits: 'unavailable: permission', epp_choices: 'unavailable: permission' }, fan: { duty_floor: 'unavailable: x' } },
    capabilities: [],
  },
};

export const goodCaps = {
  mode: 'control',
  probe: {
    readings: {
      cpu: { limits: { allowed: [400, 5300], allowed_min_mhz: 400, allowed_max_mhz: 5300, max_mhz: 5300 }, epp_choices: ['performance', 'balance_performance', 'power'] },
      fan: { duty_floor: 25 },
    },
    capabilities: [{ id: 'fan_control', available: true }, { id: 'rgb', available: true }, { id: 'gpu_clocks', available: true }],
  },
};
