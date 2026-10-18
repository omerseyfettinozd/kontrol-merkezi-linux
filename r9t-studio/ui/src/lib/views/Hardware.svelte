<script>
  import { invoke } from '@tauri-apps/api/core';
  import { app, val, fmt, cap, canControl, control, session, refreshSession, read, every, toast } from '../state.svelte.js';
  import { compareSleep } from '../sleep.js';

  const ctl = $derived(canControl());
  const s = $derived(app.snap);
  const sess = $derived(app.session);

  // ---------- Ses ----------
  let vol = $state(null);
  let mic = $state(null);
  $effect(() => {
    if (vol == null && Array.isArray(sess?.volume)) vol = sess.volume[0];
    if (mic == null && Array.isArray(sess?.mic)) mic = sess.mic[0];
  });
  async function sessionSet(method, params, done) {
    try {
      const r = await session(method, params);
      if (done) toast(done(r));
      refreshSession();
    } catch {
      /* bildirim gösterildi */
    }
  }

  // ---------- s2idle ----------
  let now = $state(null);
  let base = $state(null);
  $effect(() =>
    every(10000, async () => {
      now = await invoke('sleep_snapshot');
    }),
  );
  $effect(() => {
    invoke('sleep_baseline').then((b) => (base = b));
  });
  async function saveBaseline() {
    const snap = await invoke('sleep_snapshot');
    base = await invoke('sleep_baseline', { save: snap });
    toast('Uyku öncesi ölçüm kaydedildi. Uyuyup uyandıktan sonra bu sayfaya dönün.');
  }
  const result = $derived(base && now ? compareSleep(base, now) : null);

  // ---------- Süreçler ----------
  let procs = $state(null);
  $effect(() =>
    every(2500, async () => {
      procs = await invoke('processes', { limit: 12 });
    }),
  );
  const mb = (b) => (b == null ? '—' : b >= 1 << 30 ? `${fmt(b / (1 << 30), 2)} GB` : `${fmt(b / (1 << 20), 0)} MB`);

  // ---------- Olaylar ----------
  let events = $state([]);
  $effect(() =>
    every(10000, async () => {
      try {
        events = (await read('events', { limit: 15 })).events ?? [];
      } catch {
        events = [];
      }
    }),
  );
  const time = (ms) => (ms ? new Date(ms).toLocaleTimeString('tr-TR') : '—');
</script>

<div class="grid cols-2">
  <section class="card">
    <h3>Klavye ve kamera</h3>
    <div class="spread line">
      <span>Fn-Lock <small>(tuxedo_keyboard)</small></span>
      <div class="row tight">
        <span class="badge {val(s?.fn_lock) ? 'warn' : ''}">{val(s?.fn_lock) == null ? '—' : val(s.fn_lock) ? 'açık' : 'kapalı'}</span>
        <button class="small" disabled={!ctl || !cap('fn_lock').ok} onclick={() => control('fn_lock', { enabled: !val(s?.fn_lock) }, { key: 'fn' })}>{val(s?.fn_lock) ? 'Kapat' : 'Aç'}</button>
      </div>
    </div>
    <div class="spread line">
      <span>Kamera (USB yetkilendirme)</span>
      <div class="row tight">
        <button class="small" disabled={!ctl || !cap('camera').ok} onclick={() => control('camera', { enabled: true }, { key: 'cam' })}>Aç</button>
        <button class="small" disabled={!ctl || !cap('camera').ok} onclick={() => control('camera', { enabled: false }, { key: 'cam' })}>Kapat</button>
      </div>
    </div>
    {#if cap('charge_limit').ok}
      <div class="spread line"><span>Şarj eşiği</span><small>yetenek mevcut</small></div>
    {:else}
      <small class="faint">Şarj eşiği gösterilmiyor: {cap('charge_limit').reason}</small>
    {/if}
    <div class="sep"></div>
    <h3>USB-C güç (UCSI)</h3>
    {#each s?.power?.usb_pd ?? [] as p (p.name)}
      <div class="spread line">
        <code>{p.name.replace('ucsi-source-psy-', '')}</code>
        <span class="num">{val(p.online) ? `${fmt(val(p.voltage_v), 1)} V · ${fmt(val(p.current_a), 2)} A · ${fmt((val(p.voltage_v) ?? 0) * (val(p.current_a) ?? 0), 1)} W` : 'bağlı değil'}</span>
      </div>
    {:else}
      <small>UCSI güç kaynağı yok.</small>
    {/each}
  </section>

  <section class="card">
    <h3>Ses ve kablosuz <small>r9t-session · PipeWire / rfkill</small></h3>
    {#if !sess}
      <div class="note">r9t-session çalışmıyor.</div>
    {:else}
      {#if vol != null}
        <label class="slider">Hoparlör <input type="range" min="0" max="100" bind:value={vol} /><b class="num">%{vol}</b>
          <button class="small" onclick={() => sessionSet('set_volume', { percent: Number(vol) }, (r) => `Ses %${r.volume}`)}>Uygula</button></label>
      {:else}<small class="faint">Hoparlör seviyesi okunamadı.</small>{/if}
      {#if mic != null}
        <label class="slider">Mikrofon <input type="range" min="0" max="100" bind:value={mic} /><b class="num">%{mic}</b>
          <button class="small" onclick={() => sessionSet('set_mic_volume', { percent: Number(mic) }, (r) => `Mikrofon %${r.mic}`)}>Uygula</button></label>
      {:else}<small class="faint">Mikrofon seviyesi okunamadı.</small>{/if}
      {#if sess.volume?.[1]}<small class="faint">Hoparlör sessizde</small>{/if}
      <div class="sep"></div>
      {#each [['wifi', 'Wi-Fi'], ['bluetooth', 'Bluetooth']] as [k, t] (k)}
        <div class="spread line">
          <span>{t}</span>
          <div class="row tight">
            <span class="badge {sess[k] ? 'ok' : ''}">{sess[k] == null ? 'yok' : sess[k] ? 'açık' : 'kapalı'}</span>
            <button class="small" disabled={sess[k] == null} onclick={() => sessionSet(k, { enabled: !sess[k] }, () => `${t} ${sess[k] ? 'kapatıldı' : 'açıldı'}`)}>{sess[k] ? 'Kapat' : 'Aç'}</button>
          </div>
        </div>
      {/each}
      <div class="spread line">
        <span>Uçak modu</span>
        <div class="row tight">
          <button class="small" onclick={() => sessionSet('airplane', { enabled: true }, () => 'Uçak modu açık')}>Aç</button>
          <button class="small" onclick={() => sessionSet('airplane', { enabled: false }, () => 'Uçak modu kapalı')}>Kapat</button>
        </div>
      </div>
    {/if}
  </section>

  <section class="card">
    <h3>s2idle analizi <span class="badge">{now?.mem_sleep ?? '—'}</span></h3>
    <div class="kv">
      <span>Başarılı / hatalı uyku</span><span>{now?.stats?.success ?? '—'} / {now?.stats?.fail ?? '—'}</span>
      <span>Son donanım uykusu (s0i3)</span><span>{now?.stats?.last_hw_sleep_us == null ? '—' : `${fmt(now.stats.last_hw_sleep_us / 1e6, 1)} sn`}</span>
      <span>Toplam donanım uykusu</span><span>{now?.stats?.total_hw_sleep_us == null ? '—' : `${fmt(now.stats.total_hw_sleep_us / 1e6, 1)} sn`}</span>
      <span>Son hata</span><span>{now?.stats?.last_failed_dev ?? 'yok'} {now?.stats?.last_failed_step ?? ''}</span>
      <span>Pil</span><span>{now?.battery?.energy_uwh == null ? '—' : `${fmt(now.battery.energy_uwh / 1e6, 2)} Wh · ${now.battery.status ?? ''}`}</span>
    </div>
    <div class="row" style="margin:10px 0">
      <button onclick={saveBaseline}>Uyku öncesi ölçümü kaydet</button>
      {#if base?.time}<small>kayıt: {new Date(base.time * 1000).toLocaleString('tr-TR')}</small>{/if}
    </div>
    {#if result}
      <div class="kv">
        <span>Süre</span><span>{result.duration_s == null ? '—' : `${fmt(result.duration_s / 3600, 2)} sa`}</span>
        <span>Uyku sayısı artışı</span><span>{result.suspend_delta ?? '—'}</span>
        <span>Donanım uykusu oranı</span><span>{result.hw_ratio == null ? '—' : `%${fmt(result.hw_ratio * 100, 1)}`}</span>
        <span>Enerji kaybı</span><span>{result.loss_w == null ? '—' : `${fmt(result.loss_w, 2)} W (${fmt(result.loss_pct_h, 2)} %/sa)`}</span>
      </div>
      <div class="stack" style="margin-top:8px">
        {#each result.warnings as w (w)}<div class="note {result.valid ? 'info' : ''}">{w}</div>{/each}
      </div>
    {/if}
  </section>

  <section class="card">
    <h3>Süreç izleyici <small>CPU (1 çekirdek = %100) · {procs?.total ?? '—'} süreç</small></h3>
    <div class="scroll-x">
      <table>
        <thead><tr><th>Süreç</th><th class="r">PID</th><th class="r">CPU</th><th class="r">Bellek</th></tr></thead>
        <tbody>
          {#each procs?.processes ?? [] as p (p.pid)}
            <tr><td>{p.name}</td><td class="r num">{p.pid}</td><td class="r num">{fmt(p.cpu_pct, 1)} %</td><td class="r num">{mb(p.rss)}</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
    <small class="faint">Yalnız süreç adı, CPU süresi ve RSS okunur; komut satırı okunmaz.</small>
  </section>

  <section class="card">
    <h3>Yetenekler <small>r9td yoklaması</small></h3>
    <div class="scroll-x">
      <table>
        <tbody>
          {#each app.caps?.probe?.capabilities ?? [] as c (c.id)}
            <tr><td><code>{c.id}</code></td><td><span class="badge {c.available ? 'ok' : 'bad'}">{c.available ? 'var' : 'yok'}</span></td><td><small>{c.reason}</small></td></tr>
          {/each}
        </tbody>
      </table>
    </div>
    {#if app.caps?.probe?.device}
      <small class="faint">{app.caps.probe.device.vendor} {app.caps.probe.device.product} · BIOS {app.caps.probe.device.bios} · {app.caps.probe.device.kernel}</small>
    {/if}
  </section>

  <section class="card">
    <h3>Denetim günlüğü <small>son 15</small></h3>
    <div class="scroll-x">
      <table>
        <tbody>
          {#each events as e, i (i)}
            <tr>
              <td class="num"><small>{time(e.t_ms ?? e.time_ms)}</small></td>
              <td><code>{e.method}</code></td>
              <td><span class="badge {e.ok ? 'ok' : 'bad'}">{e.ok ? 'tamam' : 'hata'}</span></td>
              <td><small>{e.message ?? ''}</small></td>
            </tr>
          {:else}
            <tr><td><small>Olay yok.</small></td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>
</div>

<style>
  h3 small { font-weight: 400; color: var(--muted); font-size: 12px; }
  .line { padding: 5px 0; }
  .slider { display: grid; grid-template-columns: 84px 1fr 52px auto; align-items: center; gap: 8px; color: var(--muted); margin-bottom: 6px; }
  .slider b { color: var(--text); text-align: right; }
</style>
