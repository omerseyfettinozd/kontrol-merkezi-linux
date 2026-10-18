<script>
  import { app, val, fmt, EPP_NAMES } from '../state.svelte.js';
  import Stat from '../Stat.svelte';
  import Chart from '../Chart.svelte';

  const s = $derived(app.snap);
  const sensors = $derived(s?.sensors ?? []);
  // DDR5 ve NVMe: hwmon indeksine değil, sürücü + aygıt kimliğine göre (CLAUDE.md).
  const ddr = $derived(sensors.filter((x) => x.chip === 'spd5118'));
  const nvme = $derived.by(() => {
    const byDev = new Map();
    for (const x of sensors.filter((x) => x.chip === 'nvme')) {
      const dev = x.id.split(':').slice(1, -1).join(':');
      const e = byDev.get(dev) ?? { dev, composite: null, max: null };
      if (x.label === 'Composite') e.composite = x.value_c;
      const v = val(x.value_c);
      if (v != null && (e.max == null || v > e.max)) e.max = v;
      byDev.set(dev, e);
    }
    return [...byDev.values()];
  });
  const bat = $derived(s?.power?.battery);
  const health = $derived.by(() => {
    const f = val(bat?.full_wh), d = val(bat?.design_wh);
    return f != null && d ? (f / d) * 100 : null;
  });
  const vram = $derived.by(() => {
    const u = val(s?.gpu?.vram_used_mib), t = val(s?.gpu?.vram_total_mib);
    return u != null && t ? `${fmt(u / 1024, 1)} / ${fmt(t / 1024, 1)} GB` : null;
  });
</script>

{#if !s}
  <div class="note info">Anlık görüntü bekleniyor…</div>
{:else}
  <div class="grid cols-auto">
    <section class="card">
      <h3>CPU <span class="badge">8940HX</span></h3>
      <div class="grid two">
        <Stat label="EC sıcaklığı" field={s.cpu.ec_temp_c} unit="°C" big />
        <Stat label="Paket gücü" field={s.cpu.package_w} unit="W" digits={1} big />
        <Stat label="Tctl (gösterim)" field={s.cpu.tctl_c} unit="°C" digits={1} />
        <Stat label="Frekans (ort.)" field={s.cpu.freq_mhz} unit="MHz" />
        <Stat label="Tavan" value={val(s.cpu.max_freq_mhz)} unit="MHz" />
        <Stat label="Boost / EPP" value={`${val(s.cpu.boost) == null ? '—' : val(s.cpu.boost) ? 'açık' : 'kapalı'} · ${EPP_NAMES[val(s.cpu.epp)] ?? val(s.cpu.epp) ?? '—'}`} />
      </div>
    </section>
    <section class="card">
      <h3>GPU <span class="badge">RTX 5070</span> <span class="badge {val(s.gpu.runtime_status) === 'active' ? 'ok' : 'info'}">{val(s.gpu.runtime_status) ?? '—'}</span></h3>
      <div class="grid two">
        <Stat label="EC sıcaklığı" field={s.gpu.ec_temp_c} unit="°C" big />
        <Stat label="Güç" field={s.gpu.power_w} unit="W" digits={1} big />
        <Stat label="Çekirdek / bellek" value={val(s.gpu.clock_graphics_mhz) == null ? null : `${fmt(val(s.gpu.clock_graphics_mhz))} / ${fmt(val(s.gpu.clock_memory_mhz))}`} unit="MHz" />
        <Stat label="Kullanım" field={s.gpu.util_gpu_pct} unit="%" />
        <Stat label="VRAM" value={vram} />
        <Stat label="P-state · limit" value={val(s.gpu.pstate) == null ? null : `P${val(s.gpu.pstate)} · ${fmt(val(s.gpu.enforced_limit_w))} W`} />
      </div>
    </section>
    <section class="card">
      <h3>Fanlar <span class="badge {val(s.fans.mode) === 'auto' ? 'ok' : 'warn'}">{val(s.fans.mode) ?? '—'}</span>{#if val(s.fans.thermal_override)}<span class="badge bad">kernel koruması</span>{/if}</h3>
      <div class="grid two">
        <Stat label="CPU fanı" field={s.fans.cpu_rpm} unit="RPM" stale={40000} big />
        <Stat label="GPU fanı" field={s.fans.gpu_rpm} unit="RPM" stale={40000} big />
        <Stat label="CPU görev" field={s.fans.cpu_duty_pct} unit="%" stale={40000} />
        <Stat label="GPU görev" field={s.fans.gpu_duty_pct} unit="%" stale={40000} />
      </div>
    </section>
    <section class="card">
      <h3>Pil <span class="badge {val(s.power.ac_online) ? 'ok' : 'warn'}">{val(s.power.ac_online) ? 'prizde' : 'pilde'}</span></h3>
      <div class="grid two">
        <Stat label="Doluluk" field={s.power.battery.percent} unit="%" big />
        <Stat label="Güç" field={s.power.battery.power_w} unit="W" digits={1} big />
        <Stat label="Durum" field={s.power.battery.status} />
        <Stat label="Enerji" value={val(bat.energy_wh) == null ? null : `${fmt(val(bat.energy_wh), 1)} / ${fmt(val(bat.full_wh), 1)}`} unit="Wh" />
        <Stat label="Sağlık (dolu/tasarım)" value={health} unit="%" digits={1} />
        <Stat label="Gerilim" field={s.power.battery.voltage_v} unit="V" digits={2} />
      </div>
    </section>
    <section class="card">
      <h3>DDR5</h3>
      <div class="grid two">
        {#each ddr as d, i (d.id)}
          <Stat label="Modül {i + 1}" field={d.value_c} unit="°C" digits={1} />
        {:else}
          <small>spd5118 algılayıcısı yok</small>
        {/each}
      </div>
    </section>
    <section class="card">
      <h3>NVMe</h3>
      <div class="grid two">
        {#each nvme as n, i (n.dev)}
          <Stat label="Disk {i + 1} ({n.dev})" field={n.composite} unit="°C" digits={1} hint="Composite; en sıcak algılayıcı {fmt(n.max, 1)} °C" />
        {:else}
          <small>NVMe algılayıcısı yok</small>
        {/each}
      </div>
    </section>
  </div>

  <div class="grid cols-2 charts">
    <section class="card">
      <Chart
        title="Sıcaklık ve güç (5 dk)"
        leftUnit="°C"
        rightUnit="W"
        series={[
          { key: 'cpu_c', label: 'CPU °C', color: '#ff6b74' },
          { key: 'gpu_c', label: 'GPU °C', color: '#4ea1ff' },
          { key: 'cpu_w', label: 'CPU W', color: '#f0883e', axis: 'r', digits: 1 },
          { key: 'gpu_w', label: 'GPU W', color: '#39c5cf', axis: 'r', digits: 1 },
        ]}
      />
    </section>
    <section class="card">
      <Chart
        title="Fanlar ve pil (5 dk)"
        leftUnit="RPM"
        rightUnit="W"
        leftMin={0}
        series={[
          { key: 'cpu_rpm', label: 'CPU RPM', color: '#b083f0', maxGapMs: 35000 },
          { key: 'gpu_rpm', label: 'GPU RPM', color: '#3fb950', maxGapMs: 35000 },
          { key: 'battery_w', label: 'Pil W', color: '#e3b341', axis: 'r', dash: true, digits: 1 },
        ]}
      />
    </section>
  </div>
{/if}

<style>
  .two { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px 14px; }
  .charts { margin-top: var(--gap); }
</style>
