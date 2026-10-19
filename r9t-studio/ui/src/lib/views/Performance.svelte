<script>
  import { app, val, fmt, cap, canControl, control, POWER_NAMES, EPP_NAMES, SCENARIO_NAMES } from '../state.svelte.js';
  import FanCurve from '../FanCurve.svelte';
  import Stat from '../Stat.svelte';
  import { validate } from '../curve.js';

  const s = $derived(app.snap);
  const ctl = $derived(canControl());
  const readings = $derived(app.caps?.probe?.readings);
  const limits = $derived(readings?.cpu?.limits);
  const eppChoices = $derived(readings?.cpu?.epp_choices ?? []);
  const presets = $derived(app.caps?.thermal_presets ?? []);
  const last = $derived(app.policy?.last);
  const onBattery = $derived(val(s?.power?.ac_online) === false);

  // Düzenleme durumları: sunucudan bir kez doldurulur, sonra kullanıcıya aittir.
  let cpuMhz = $state(null);
  let gpuMhz = $state(null);
  let manual = $state({ cpu: 60, gpu: 60 });
  // r9t_fan min_duty (probe): 25 after the low-duty sweep, 50 on 0.2.0.
  const floor = $derived(app.caps?.probe?.fan?.duty_floor ?? 50);
  let target = $state({ cpu: 75, gpu: 75 });
  let curve = $state([[45, 50], [60, 60], [75, 80], [85, 100]]);
  let interpolation = $state('linear');
  let seeded = false;

  $effect(() => {
    if (seeded || !s || !app.policy) return;
    seeded = true;
    cpuMhz = val(s.cpu.max_freq_mhz) ?? limits?.allowed?.[1] ?? 5000;
    gpuMhz = last?.gpu_max_mhz || val(s.gpu.max_clock_mhz) || 3090;
    const f = last?.fan;
    if (f?.mode === 'curve' && Array.isArray(f.points)) {
      curve = f.points.map((p) => [p[0], p[1]]);
      interpolation = f.interpolation ?? 'linear';
    }
    if (f?.mode === 'manual') manual = { cpu: f.cpu, gpu: f.gpu };
  });

  const fanMode = $derived(val(s?.fans?.mode));
  const reason = $derived(s?.authority?.reason ?? '');
  const targetActive = $derived(reason.includes('sıcaklık hedefi'));
  const curveError = $derived(validate(curve));
  const why = (c) => (c.ok ? '' : c.reason);
  const ctlTitle = $derived(ctl ? '' : 'r9td denetimde değil (gölge modu ya da bağlantı yok)');
</script>

{#if !s}
  <div class="note info">Anlık görüntü bekleniyor…</div>
{:else}
  <div class="grid cols-2">
    <section class="card">
      <h3>Senaryolar <small>tek atomik sunucu komutu</small></h3>
      <div class="row">
        {#each app.caps?.scenarios ?? ['quiet', 'balanced', 'game', 'turbo'] as sc (sc)}
          <button
            class:active={s.authority.active_profile === `senaryo:${sc}`}
            disabled={!ctl || (sc === 'turbo' && onBattery)}
            title={sc === 'turbo' && onBattery ? 'Turbo yalnız prizde' : ctlTitle}
            onclick={() => control('scenario', { name: sc }, { key: 'scenario' })}>{SCENARIO_NAMES[sc] ?? sc}</button>
        {/each}
      </div>
      <div class="sep"></div>
      <h3>Güç profili <small>power-profiles-daemon (yalnız EPP'yi değiştirir)</small></h3>
      <div class="row">
        {#each ['power-saver', 'balanced', 'performance'] as p (p)}
          <button class:active={val(s.power.profile) === p} disabled={!ctl || !cap('power_profile').ok} title={why(cap('power_profile')) || ctlTitle} onclick={() => control('power', { profile: p }, { key: 'power' })}>{POWER_NAMES[p]}</button>
        {/each}
      </div>
      <div class="row" style="margin-top:10px">
        <label class="row tight">
          <input type="checkbox" checked={val(s.cpu.boost) === true} disabled={!ctl || !cap('cpu_boost').ok} onchange={(e) => control('boost', { enabled: e.currentTarget.checked })} />
          CPU boost
        </label>
        <label class="row tight">EPP
          <select value={val(s.cpu.epp) ?? ''} disabled={!ctl || !cap('cpu_epp').ok} onchange={(e) => control('profile', { settings: { cpu_epp: e.currentTarget.value } }, { key: 'epp' })}>
            {#each eppChoices as e (e)}<option value={e}>{EPP_NAMES[e] ?? e}</option>{/each}
          </select>
        </label>
      </div>
    </section>

    <section class="card">
      <h3>Ölçülmüş presetler</h3>
      <div class="presets">
        {#each presets as p (p.key)}
          <div class="preset" class:on={s.authority.active_profile === p.key}>
            <div class="spread"><b>{p.name}</b><button class="small" disabled={!ctl} title={ctlTitle} onclick={() => control('thermal_preset', { preset: p.key }, { key: 'preset' })}>Uygula</button></div>
            <small>{p.description}</small>
            {#if p.settings}
              <small class="faint num">{POWER_NAMES[p.settings.power_profile] ?? ''} · boost {p.settings.boost_enabled ? 'açık' : 'kapalı'} · CPU {p.settings.cpu_max_mhz ? `${p.settings.cpu_max_mhz} MHz` : 'sınırsız'} · GPU {p.settings.gpu_max_mhz ? `${p.settings.gpu_max_mhz} MHz` : 'sınırsız'}</small>
            {/if}
          </div>
        {:else}
          <small>Preset listesi r9td'den alınamadı.</small>
        {/each}
      </div>
    </section>

    <section class="card">
      <h3>CPU tavanı <span class="badge">{limits ? `${limits.allowed[0]}–${limits.allowed[1]} MHz` : '—'}</span></h3>
      {#if cpuMhz != null && limits}
        <div class="row"><input type="range" min={limits.allowed[0]} max={limits.allowed[1]} step="50" bind:value={cpuMhz} disabled={!ctl} /><b class="num w80">{cpuMhz} MHz</b></div>
        <div class="row">
          <button class="primary" disabled={!ctl || !cap('cpu_limit').ok} onclick={() => control('profile', { settings: { cpu_max_mhz: Number(cpuMhz) } }, { key: 'cpu' })}>Uygula</button>
          <button disabled={!ctl} onclick={() => control('profile', { settings: { cpu_max_mhz: 0 } }, { key: 'cpu' })}>Sınırı kaldır</button>
          <small>Şu an: {fmt(val(s.cpu.max_freq_mhz))} MHz {val(s.cpu.boost) === false ? '(boost kapalıyken sürücü taban tavana kırpar)' : ''}</small>
        </div>
      {/if}
      <div class="sep"></div>
      <h3>GPU çekirdek tavanı <span class="badge {cap('gpu_locked_clocks').ok ? 'info' : 'bad'}">{cap('gpu_locked_clocks').ok ? 'NVML kilitli saat' : 'yok'}</span></h3>
      {#if gpuMhz != null}
        <div class="row"><input type="range" min="300" max={val(s.gpu.max_clock_mhz) ?? 3090} step="15" bind:value={gpuMhz} disabled={!ctl || !cap('gpu_locked_clocks').ok} /><b class="num w80">{gpuMhz} MHz</b></div>
        <div class="row">
          <button class="primary" disabled={!ctl || !cap('gpu_locked_clocks').ok} onclick={() => control('profile', { settings: { gpu_max_mhz: Number(gpuMhz) } }, { key: 'gpu' })}>Uygula</button>
          <button disabled={!ctl || !cap('gpu_locked_clocks').ok} onclick={() => control('profile', { settings: { gpu_max_mhz: 0 } }, { key: 'gpu' })}>Sıfırla</button>
          <small>{cap('gpu_locked_clocks').reason}</small>
        </div>
      {/if}
    </section>

    <section class="card">
      <h3>Güç sınırları <span class="badge">salt okunur</span></h3>
      <div class="grid two">
        <Stat label="APU paket PPT (amdgpu)" field={s.cpu.apu_ppt_w} unit="W" digits={1} hint="amdgpu power1_input: APU paket PPT, iGPU watt'ı değil" />
        <Stat label="CPU paket (RAPL)" field={s.cpu.package_w} unit="W" digits={1} />
        <Stat label="GPU uygulanan limit" field={s.gpu.enforced_limit_w} unit="W" />
        <Stat label="GPU güç" field={s.gpu.power_w} unit="W" digits={1} />
      </div>
      <p class="note" style="margin-top:10px">Ryzen PPT ayarı: {cap('cpu_ppt').ok ? 'mevcut' : `desteklenmiyor — ${cap('cpu_ppt').reason}`}. GPU güç limiti NVML'de salt okunurdur (5–115 W).</p>
    </section>
  </div>

  <section class="card fan">
    <div class="spread">
      <h3>Fan denetimi <span class="badge {fanMode === 'auto' ? 'ok' : 'warn'}">{fanMode ?? '—'}</span> <small>{reason}</small></h3>
      <div class="row tight">
        <button class:active={fanMode === 'auto' && !targetActive} disabled={!ctl} onclick={() => control('auto')}>Otomatik (EC)</button>
        <button class:active={fanMode === 'boost'} disabled={!ctl} onclick={() => control('fan_boost')}>Maksimum</button>
        {#each app.caps?.fan_presets?.filter((k) => k !== 'maximum') ?? [] as k (k)}
          <button disabled={!ctl} onclick={() => control('fan_preset', { preset: k }, { key: 'fan_preset' })}>Eğri: {k === 'balanced' ? 'dengeli' : k === 'cool' ? 'serin' : k}</button>
        {/each}
      </div>
    </div>
    <div class="fan-grid">
      <div>
        <FanCurve bind:points={curve} bind:interpolation cpuTemp={val(s.cpu.ec_temp_c)} gpuTemp={val(s.gpu.ec_temp_c)} disabled={!ctl} {floor} />
        <div class="row" style="margin-top:8px">
          <label class="row tight">Enterpolasyon
            <select bind:value={interpolation}>
              <option value="linear">Doğrusal</option>
              <option value="monotone_cubic">Monoton kübik (spline)</option>
            </select>
          </label>
          <button class="primary" disabled={!ctl || !!curveError} onclick={() => control('curve', { points: curve, interpolation }, { key: 'curve' })}>Eğriyi uygula</button>
          <small>Eğri her iki fana CPU/GPU EC sıcaklığıyla ayrı ayrı uygulanır; koruma eşikleri her zaman geçerlidir.</small>
        </div>
      </div>
      <div class="stack">
        <div class="card inner">
          <h3>Manuel</h3>
          <label class="slider">CPU fanı <input type="range" min={floor} max="100" bind:value={manual.cpu} disabled={!ctl} /><b class="num">%{manual.cpu}</b></label>
          <label class="slider">GPU fanı <input type="range" min={floor} max="100" bind:value={manual.gpu} disabled={!ctl} /><b class="num">%{manual.gpu}</b></label>
          <button disabled={!ctl} onclick={() => control('manual', { cpu: Number(manual.cpu), gpu: Number(manual.gpu) }, { key: 'manual' })}>Manuel uygula</button>
          <small>Taban %{floor} (ölçülmüş: %25 ≈ 1600, %50 ≈ 3200 RPM). Sıcaklık yükselince modül tabanı %50'ye çeker.</small>
        </div>
        <div class="card inner">
          <h3>Sıcaklık hedefi (PID) {#if targetActive}<span class="badge warn">etkin</span>{/if}</h3>
          <label class="slider">CPU <input type="range" min="60" max="85" bind:value={target.cpu} disabled={!ctl} /><b class="num">{target.cpu} °C</b></label>
          <label class="slider">GPU <input type="range" min="60" max="80" bind:value={target.gpu} disabled={!ctl} /><b class="num">{target.gpu} °C</b></label>
          <div class="row tight">
            <button disabled={!ctl} onclick={() => control('temperature_target', { cpu: Number(target.cpu), gpu: Number(target.gpu) }, { key: 'target' })}>Hedefi başlat</button>
            <button disabled={!ctl || !targetActive} onclick={() => control('temperature_target_stop', {}, { key: 'target' })}>Durdur</button>
          </div>
          <small>Fan ve CPU tavanı birlikte ayarlanır; durdurunca tavan geri yüklenir.</small>
        </div>
      </div>
    </div>
  </section>
{/if}

<style>
  h3 small { font-weight: 400; color: var(--muted); font-size: 12px; }
  .presets { display: flex; flex-direction: column; gap: 8px; max-height: 330px; overflow-y: auto; padding-right: 4px; }
  .preset { border: 1px solid var(--border); border-radius: 8px; padding: 8px 10px; display: flex; flex-direction: column; gap: 3px; background: var(--panel-2); }
  .preset.on { border-color: var(--accent); }
  .two { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
  .w80 { min-width: 86px; text-align: right; }
  .fan { margin-top: var(--gap); }
  .fan-grid { display: grid; grid-template-columns: minmax(0, 1.7fr) minmax(250px, 1fr); gap: var(--gap); margin-top: 10px; }
  .inner { background: var(--panel-2); display: flex; flex-direction: column; gap: 8px; }
  .slider { display: grid; grid-template-columns: 70px 1fr 56px; align-items: center; gap: 8px; color: var(--muted); }
  .slider b { color: var(--text); text-align: right; }
  @media (max-width: 1150px) { .fan-grid { grid-template-columns: minmax(0, 1fr); } }
</style>
