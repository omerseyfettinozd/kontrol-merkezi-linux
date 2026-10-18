<script>
  import { onMount } from 'svelte';
  import { app, start, val, fmt, isShadow, canControl, control, session, refreshSession, POWER_NAMES, toast } from './lib/state.svelte.js';
  import Overview from './lib/views/Overview.svelte';
  import Performance from './lib/views/Performance.svelte';
  import Keyboard from './lib/views/Keyboard.svelte';
  import Display from './lib/views/Display.svelte';
  import Hardware from './lib/views/Hardware.svelte';
  import Profiles from './lib/views/Profiles.svelte';

  const VIEWS = [
    { id: 'overview', name: 'Genel Bakış', icon: 'M3 13h8V3H3zm0 8h8v-6H3zm10 0h8V11h-8zm0-18v6h8V3z', comp: Overview },
    { id: 'performance', name: 'Performans & Termal', icon: 'M12 2a10 10 0 1 0 10 10h-2a8 8 0 1 1-8-8zm1 3v6.6l4.2 2.5-1 1.7L11 12.7V5z', comp: Performance },
    { id: 'keyboard', name: 'Klavye & RGB', icon: 'M2 6h20v12H2zm2 2v2h2V8zm3 0v2h2V8zm3 0v2h2V8zm3 0v2h2V8zm3 0v2h2V8zM4 11v2h2v-2zm3 0v2h2v-2zm3 0v2h2v-2zm3 0v2h2v-2zm3 0v2h2v-2zM6 14v2h12v-2z', comp: Keyboard },
    { id: 'display', name: 'Ekran, GPU & Otomasyon', icon: 'M3 4h18v12H3zm2 2v8h14V6zm3 12h8v2H8z', comp: Display },
    { id: 'hardware', name: 'Donanım, Ses & Tanılama', icon: 'M9 2h6v3h3v3h3v6h-3v3h-3v3H9v-3H6v-3H3V8h3V5h3zm-1 6v8h8V8z', comp: Hardware },
    { id: 'profiles', name: 'Profiller & Kıyaslama', icon: 'M4 4h16v4H4zm0 6h16v4H4zm0 6h16v4H4z', comp: Profiles },
  ];

  const current = $derived(VIEWS.find((v) => v.id === app.view) ?? VIEWS[0]);
  const s = $derived(app.snap);
  const edp = $derived(app.session?.outputs?.find((o) => o.name.startsWith('eDP')) ?? app.session?.outputs?.[0]);
  const boostOn = $derived(val(s?.fans?.mode) === 'boost');

  onMount(() => {
    start();
    const id = setInterval(() => {
      if (!document.hidden) refreshSession();
    }, 5000);
    return () => clearInterval(id);
  });

  async function setHz(hz) {
    if (!edp) return;
    try {
      const r = await session('set_refresh_rate', { output: edp.name, hz: Math.round(hz) });
      toast(r.message || `${edp.name}: ${Math.round(hz)} Hz`);
      refreshSession();
    } catch {
      /* bildirim gösterildi */
    }
  }

  const tempClass = (c) => (c == null ? '' : c >= 90 ? 'hot' : c >= 80 ? 'warm' : '');
</script>

<div class="shell">
  <nav class="side" aria-label="Modüller">
    <div class="brand">
      <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true"><circle cx="12" cy="12" r="10" fill="none" stroke="#e63946" stroke-width="2.4" /><circle cx="12" cy="12" r="3" fill="#e8ebf1" /></svg>
      <div><b>R9T Studio</b><small>{app.snap ? 'SLAYER R9T' : ''}</small></div>
    </div>
    {#each VIEWS as v (v.id)}
      <button class="nav" class:sel={app.view === v.id} onclick={() => (app.view = v.id)} aria-current={app.view === v.id ? 'page' : undefined}>
        <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path d={v.icon} fill="currentColor" /></svg>
        <span>{v.name}</span>
      </button>
    {/each}
    <div class="grow"></div>
    <div class="conn">
      <div class="row tight"><i class="dot" class:on={app.connected}></i> r9td {app.connected ? (isShadow() ? 'gölge (salt okunur)' : 'denetimde') : 'bağlı değil'}</div>
      <div class="row tight"><i class="dot" class:on={!!app.session}></i> r9t-session {app.session ? 'çalışıyor' : 'yok'}</div>
      {#if app.hello}<small class="faint">r9td {app.hello.version} · protokol {app.hello.protocol_version}</small>{/if}
    </div>
  </nav>

  <div class="main">
    <header class="strip">
      <div class="chip">
        <span class="label">Profil</span>
        <b>{POWER_NAMES[val(s?.power?.profile)] ?? '—'}</b>
        {#if s?.authority?.active_profile}<small>{s.authority.active_profile}</small>{/if}
      </div>
      <div class="chip">
        <span class="label">{val(s?.power?.ac_online) ? 'Priz' : 'Pil'}</span>
        <b class="num">%{fmt(val(s?.power?.battery?.percent))}</b>
        <small class="num">{fmt(val(s?.power?.battery?.power_w), 1, 'W')}</small>
      </div>
      <div class="chip {tempClass(val(s?.cpu?.ec_temp_c))}">
        <span class="label">CPU</span>
        <b class="num">{fmt(val(s?.cpu?.ec_temp_c), 0, '°C')}</b>
        <small class="num">{fmt(val(s?.cpu?.package_w), 1, 'W')}</small>
      </div>
      <div class="chip {tempClass(val(s?.gpu?.ec_temp_c))}">
        <span class="label">GPU</span>
        <b class="num">{fmt(val(s?.gpu?.ec_temp_c), 0, '°C')}</b>
        <small class="num">{fmt(val(s?.gpu?.power_w), 1, 'W')}</small>
      </div>
      <div class="chip">
        <span class="label">Fan</span>
        <b class="num">{fmt(val(s?.fans?.cpu_rpm))} / {fmt(val(s?.fans?.gpu_rpm))}</b>
        <small>{val(s?.fans?.mode) ?? '—'}</small>
      </div>
      <div class="grow"></div>
      <button
        class:active={boostOn}
        disabled={!canControl()}
        title={canControl() ? 'Fanları %100’e al / otomatiğe döndür' : 'r9td denetimde değil'}
        onclick={() => (boostOn ? control('auto') : control('fan_boost'))}>{boostOn ? 'Max Fan: açık' : 'Max Fan'}</button>
      <div class="hz" title={edp ? `${edp.name} yenileme hızı` : 'r9t-session gerekli'}>
        {#if edp}
          <span class="label">Hz</span>
          {#each edp.available_hz as hz (hz)}
            <button class="small" class:active={Math.round(edp.refresh_hz) === Math.round(hz)} onclick={() => setHz(hz)}>{Math.round(hz)}</button>
          {/each}
        {:else}
          <small class="faint">Hz: r9t-session yok</small>
        {/if}
      </div>
    </header>

    {#if !app.connected}
      <div class="note bad banner">r9td'ye bağlanılamıyor: {app.daemonError ?? 'bekleniyor…'} — 2 sn'de bir yeniden denenir.</div>
    {:else if isShadow()}
      <div class="note info banner">r9td salt okunur gölge modunda: veriler canlıdır, ayar değiştiren düğmeler devre dışıdır. Denetim için r9td etkin olmalıdır (<code>r9t-switch --trial</code>).</div>
    {:else if s?.authority?.guard_active}
      <div class="note bad banner">Termal koruma etkin: fanlar %100 (CPU ≥ 90 °C ya da GPU ≥ 84 °C). Sıcaklık düşünce bırakılır.</div>
    {/if}

    <main class="content">
      <div class="title spread"><h2>{current.name}</h2>{#if s}<small class="faint num">örnek #{s.sequence}</small>{/if}</div>
      {#key current.id}
        <current.comp />
      {/key}
    </main>
  </div>

  <div class="toasts" aria-live="polite">
    {#each app.toasts as t (t.id)}
      <div class="toast {t.kind}">{t.text}</div>
    {/each}
  </div>
</div>

<style>
  .shell { display: grid; grid-template-columns: 216px minmax(0, 1fr); height: 100vh; }
  .side { background: #12151b; border-right: 1px solid var(--border); padding: 14px 10px; display: flex; flex-direction: column; gap: 4px; min-height: 0; }
  .brand { display: flex; align-items: center; gap: 10px; padding: 4px 8px 14px; }
  .brand b { display: block; font-size: 15px; }
  .brand small { display: block; font-size: 11px; color: var(--faint); }
  .nav { display: flex; align-items: center; gap: 10px; text-align: left; background: transparent; border: 1px solid transparent; padding: 9px 10px; color: var(--muted); white-space: normal; }
  .nav:hover:not(:disabled) { background: #1a1e27; border-color: transparent; color: var(--text); }
  .nav.sel { background: #21151a; border-color: #4a1f26; color: #fff; }
  .nav svg { flex: none; }
  .grow { flex: 1; }
  .conn { font-size: 12px; color: var(--muted); display: flex; flex-direction: column; gap: 4px; padding: 8px; border-top: 1px solid var(--border); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--danger); display: inline-block; }
  .dot.on { background: var(--green); }

  .main { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .strip { display: flex; align-items: center; gap: 6px; padding: 8px 14px; border-bottom: 1px solid var(--border); background: #12151b; flex-wrap: wrap; }
  .chip { display: flex; align-items: baseline; gap: 5px; padding: 4px 8px; white-space: nowrap; border: 1px solid var(--border); border-radius: 8px; background: var(--panel); }
  .chip b { font-size: 14px; }
  .chip small { font-size: 12px; }
  .chip.warm b { color: var(--yellow); }
  .chip.hot b { color: var(--danger); }
  .hz { display: flex; gap: 4px; align-items: center; }
  .banner { margin: 10px 18px 0; }
  .content { flex: 1; overflow-y: auto; padding: 14px 18px 28px; min-width: 0; }
  .title { margin-bottom: 12px; }

  .toasts { position: fixed; right: 16px; bottom: 16px; display: flex; flex-direction: column; gap: 8px; max-width: 460px; z-index: 10; }
  .toast { background: #1c2230; border: 1px solid var(--border); border-left: 4px solid var(--green); padding: 9px 12px; border-radius: 8px; box-shadow: 0 6px 20px #0008; font-size: 13px; }
  .toast.warn { border-left-color: var(--yellow); }
  .toast.bad { border-left-color: var(--danger); }
</style>
