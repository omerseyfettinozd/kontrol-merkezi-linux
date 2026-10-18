<script>
  import { app, val, fmt, canControl, control, session, refreshSession, every, toast } from '../state.svelte.js';

  const ctl = $derived(canControl());
  const s = $derived(app.snap);
  const sess = $derived(app.session);

  // ---------- Ekran ----------
  let bright = $state(null);
  $effect(() => {
    if (bright == null && sess?.brightness_pct != null) bright = sess.brightness_pct;
  });
  async function setHz(o, hz) {
    try {
      const r = await session('set_refresh_rate', { output: o.name, hz: Math.round(hz) });
      toast(r.message || `${o.name} ${Math.round(hz)} Hz`);
      refreshSession();
    } catch {
      /* bildirim gösterildi */
    }
  }
  async function setBright() {
    try {
      const r = await session('set_brightness', { percent: Number(bright) });
      toast(`Ekran parlaklığı %${r.brightness_pct}`);
    } catch {
      /* bildirim gösterildi */
    }
  }

  // ---------- Otomasyon (politika belgesi) ----------
  let doc = $state(null);
  let dirty = $state(false);
  $effect(() => {
    if (!dirty && app.policy?.document) doc = structuredClone($state.snapshot(app.policy.document));
  });
  const names = $derived(doc ? Object.keys(doc.profiles) : []);
  let newRule = $state({ executable: '', profile: '' });
  let steam = $state(null);
  let steamProfile = $state('');

  function touch() {
    dirty = true;
  }
  function addRule(executable, profile) {
    if (!executable || !profile) return;
    if (doc.automation.apps.some((r) => r.executable === executable)) {
      toast('Bu kural zaten var.', 'warn');
      return;
    }
    doc.automation.apps.push({ executable, profile });
    touch();
  }
  async function save() {
    const r = await control('configure', { document: $state.snapshot(doc) }, { key: 'configure' });
    if (r) dirty = false;
  }
  async function loadSteam() {
    try {
      steam = (await session('steam_games')).games;
    } catch {
      steam = null;
    }
  }

  // ---------- GPU Wake Inspector ----------
  let gpu = $state(null);
  let confirmPid = $state(null);
  $effect(() =>
    every(5000, async () => {
      try {
        gpu = await session('gpu_processes', {}, { notify: false });
      } catch (e) {
        gpu = { processes: [], note: `GPU süreçleri okunamadı: ${e?.message ?? e}` };
      }
    }),
  );
  async function kill(p) {
    confirmPid = null;
    try {
      const r = await session('kill_process', { pid: p.pid, start_time: p.start_time });
      toast(r.message || `${p.name} kapatıldı`);
      gpu = await session('gpu_processes', {}, { notify: false });
    } catch {
      /* bildirim gösterildi */
    }
  }
</script>

<div class="grid cols-2">
  <section class="card">
    <h3>Ekran</h3>
    {#if !sess}
      <div class="note">r9t-session çalışmıyor; Hz ve parlaklık oturum ajanı üzerinden değişir (<code>systemctl --user start r9t-session</code>).</div>
    {:else}
      {#each sess.outputs as o (o.name)}
        <div class="spread out">
          <div><b>{o.name}</b> <small class="num">{o.width}×{o.height} · {fmt(o.refresh_hz, 1)} Hz</small></div>
          <div class="row tight">
            {#each o.available_hz as hz (hz)}
              <button class="small" class:active={Math.round(o.refresh_hz) === Math.round(hz)} onclick={() => setHz(o, hz)}>{Math.round(hz)} Hz</button>
            {/each}
          </div>
        </div>
      {/each}
      <small>Hz listesi çalışma anında derleyiciden okunur. Hz politikası: {sess.hz_policy ?? '—'}</small>
      <div class="sep"></div>
      {#if bright != null}
        <label class="row">Parlaklık (nvidia_0)
          <input type="range" min="1" max="100" bind:value={bright} style="max-width:260px" />
          <b class="num">%{bright}</b>
          <button class="small" onclick={setBright}>Uygula</button>
        </label>
      {:else}<small class="faint">Ekran parlaklığı okunamadı.</small>{/if}
    {/if}
  </section>

  <section class="card">
    <h3>GPU Wake Inspector <span class="badge {val(s?.gpu?.runtime_status) === 'active' ? 'warn' : 'ok'}">{val(s?.gpu?.runtime_status) ?? '—'}</span></h3>
    <p class="note info">Dahili ekran (eDP-1) dGPU'ya bağlı: dGPU bu yapılandırmada D3cold'a giremez. Aşağıdaki süreçler dGPU'yu ayrıca meşgul eder.</p>
    <div class="kv" style="margin:10px 0">
      <span>P-state</span><span>{val(s?.gpu?.pstate) == null ? '—' : `P${val(s.gpu.pstate)}`}</span>
      <span>Kullanım</span><span>{fmt(val(s?.gpu?.util_gpu_pct), 0, '%')}</span>
      <span>Güç</span><span>{fmt(val(s?.gpu?.power_w), 1, 'W')}</span>
    </div>
    {#if gpu?.processes?.length}
      <div class="scroll-x">
        <table>
          <thead><tr><th>Tür</th><th>Süreç</th><th class="r">PID</th><th class="r">VRAM</th><th></th></tr></thead>
          <tbody>
            {#each gpu.processes as p (`${p.pid}-${p.kind}`)}
              <tr>
                <td><span class="badge {p.kind === 'G' ? 'info' : 'warn'}">{p.kind}</span></td>
                <td>{p.name ?? '?'}</td>
                <td class="r num">{p.pid}</td>
                <td class="r num">{p.used_mib == null ? '—' : `${p.used_mib} MiB`}</td>
                <td class="r">
                  {#if !p.killable}<small class="faint">korunur</small>
                  {:else if confirmPid === p.pid}
                    <button class="small danger" onclick={() => kill(p)}>Evet, kapat</button>
                    <button class="small" onclick={() => (confirmPid = null)}>Vazgeç</button>
                  {:else}<button class="small" onclick={() => (confirmPid = p.pid)}>Kapat…</button>{/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <small>{gpu ? (gpu.note ?? gpu.error ?? 'dGPU kullanan süreç yok') : 'r9t-session gerekli'}</small>
    {/if}
  </section>
</div>

<section class="card auto">
  <div class="spread">
    <h3>Otomasyon {#if app.policy?.paused}<span class="badge warn">duraklatıldı</span>{:else}<span class="badge ok">etkin</span>{/if}</h3>
    <div class="row tight">
      {#if app.policy?.paused}
        <button disabled={!ctl} onclick={() => control('automation_resume')}>Sürdür</button>
      {:else}
        <button disabled={!ctl} onclick={() => control('automation_pause')}>Duraklat</button>
      {/if}
      <button class="primary" disabled={!ctl || !dirty} onclick={save}>Kaydet</button>
      {#if dirty}<button onclick={() => (dirty = false)}>Değişiklikleri at</button>{/if}
    </div>
  </div>
  {#if !doc}
    <small>Politika okunamadı.</small>
  {:else}
    <div class="grid cols-auto" style="margin-top:10px">
      {#each [['startup', 'Açılışta'], ['ac', 'Prize takılınca'], ['battery', 'Pile geçince']] as [k, t] (k)}
        <label class="stack lbl">{t}
          <select value={doc.automation[k] ?? ''} onchange={(e) => { doc.automation[k] = e.currentTarget.value || null; touch(); }}>
            <option value="">— yok —</option>
            {#each names as n (n)}<option value={n}>{n}</option>{/each}
          </select>
        </label>
      {/each}
      <label class="stack lbl">Klavye ışığı boşta kapanma (sn, 0 = kapalı)
        <input type="number" min="0" max="3600" value={doc.automation.lighting_timeout} onchange={(e) => { doc.automation.lighting_timeout = Math.max(0, Math.min(3600, Number(e.currentTarget.value) || 0)); touch(); }} />
      </label>
      <label class="row tight lbl"><input type="checkbox" checked={doc.automation.low_hz_on_battery} onchange={(e) => { doc.automation.low_hz_on_battery = e.currentTarget.checked; touch(); }} /> Pilde düşük Hz</label>
    </div>

    <h3 style="margin-top:14px">Uygulama ve oyun kuralları</h3>
    <div class="scroll-x">
      <table>
        <thead><tr><th>Eşleşme</th><th>Profil</th><th></th></tr></thead>
        <tbody>
          {#each doc.automation.apps as r, i (r.executable)}
            <tr>
              <td><code>{r.executable}</code></td>
              <td>{r.profile}{#if !names.includes(r.profile)} <span class="badge bad">profil yok</span>{/if}</td>
              <td class="r"><button class="small" onclick={() => { doc.automation.apps.splice(i, 1); touch(); }}>Sil</button></td>
            </tr>
          {:else}
            <tr><td colspan="3"><small>Kural yok.</small></td></tr>
          {/each}
        </tbody>
      </table>
    </div>
    <div class="row" style="margin-top:8px">
      <input placeholder="/usr/bin/oyun ya da class:steam_app_*" bind:value={newRule.executable} style="flex:1;min-width:220px" />
      <select bind:value={newRule.profile}>
        <option value="">profil seç</option>
        {#each names as n (n)}<option value={n}>{n}</option>{/each}
      </select>
      <button onclick={() => { addRule(newRule.executable.trim(), newRule.profile); newRule.executable = ''; }} disabled={!newRule.executable.trim() || !newRule.profile}>Kural ekle</button>
    </div>

    <div class="spread" style="margin-top:14px">
      <h3>Steam oyunları</h3>
      <div class="row tight">
        <select bind:value={steamProfile}>
          <option value="">profil seç</option>
          {#each names as n (n)}<option value={n}>{n}</option>{/each}
        </select>
        <button onclick={loadSteam} disabled={!sess}>Listeyi oku</button>
      </div>
    </div>
    {#if steam}
      <div class="steam">
        {#each steam as g (g.appid)}
          {@const rule = `class:${g.class}`}
          <div class="game">
            <span title={g.dir ?? ''}>{g.name}</span>
            {#if doc.automation.apps.some((r) => r.executable === rule)}
              <span class="badge ok">kural var</span>
            {:else}
              <button class="small" disabled={!steamProfile} title={steamProfile ? '' : 'Önce profil seçin'} onclick={() => addRule(rule, steamProfile)}>+ {steamProfile || 'profil'}</button>
            {/if}
          </div>
        {:else}
          <small>Kurulu Steam oyunu bulunamadı.</small>
        {/each}
      </div>
    {/if}
  {/if}
</section>

<style>
  .out { padding: 6px 0; }
  .auto { margin-top: var(--gap); }
  .lbl { color: var(--muted); font-size: 12px; gap: 4px; }
  .steam { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 6px; margin-top: 8px; }
  .game { display: flex; justify-content: space-between; align-items: center; gap: 8px; padding: 6px 10px; border: 1px solid var(--border); border-radius: 8px; background: var(--panel-2); }
  .game span:first-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
