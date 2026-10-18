<script>
  import { invoke } from '@tauri-apps/api/core';
  import { app, val, fmt, canControl, control, toast, POWER_NAMES, EPP_NAMES } from '../state.svelte.js';

  const ctl = $derived(canControl());
  const doc = $derived(app.policy?.document);
  const names = $derived(doc ? Object.keys(doc.profiles) : []);

  // ---------- Karşılaştırma tablosu ----------
  const fanText = (f) => {
    if (!f) return '—';
    if (f.mode === 'manual') return `manuel %${f.cpu}/%${f.gpu}`;
    if (f.mode === 'curve') return `eğri (${f.points.length} nokta)`;
    if (f.mode === 'preset') return `preset: ${f.preset}`;
    return { auto: 'otomatik', boost: 'maksimum' }[f.mode] ?? f.mode;
  };
  const rgbBox = (c) => (c ? `rgb(${c.join(',')})` : null);
  const ROWS = [
    ['Güç profili', (p) => POWER_NAMES[p.power_profile] ?? '—'],
    ['CPU boost', (p) => (p.boost_enabled == null ? '—' : p.boost_enabled ? 'açık' : 'kapalı')],
    ['CPU tavanı', (p) => (p.cpu_max_mhz == null ? '—' : p.cpu_max_mhz === 0 ? 'sınırsız' : `${p.cpu_max_mhz} MHz`)],
    ['GPU tavanı', (p) => (p.gpu_max_mhz == null ? '—' : p.gpu_max_mhz === 0 ? 'sınırsız' : `${p.gpu_max_mhz} MHz`)],
    ['EPP', (p) => EPP_NAMES[p.cpu_epp] ?? '—'],
    ['Fan', (p) => fanText(p.fan)],
    ['Klavye', (p) => (p.rgb_map ? 'harita (126)' : p.rgb ? 'tek renk' : '—')],
    ['Parlaklık', (p) => (p.brightness == null ? '—' : `%${p.brightness}`)],
  ];

  async function saveDoc(next, msg) {
    const r = await control('configure', { document: next }, { key: 'configure', quiet: true });
    if (r) toast(msg);
  }
  function removeProfile(n) {
    const next = $state.snapshot(doc);
    delete next.profiles[n];
    for (const k of ['startup', 'ac', 'battery']) if (next.automation[k] === n) next.automation[k] = null;
    next.automation.apps = next.automation.apps.filter((r) => r.profile !== n);
    saveDoc(next, `"${n}" silindi (ona bağlı kurallar da kaldırıldı).`);
  }

  // Mevcut durumdan profil: geri okunabilen değerler anlık görüntüden, gerisi last.json'dan.
  let newName = $state('');
  function saveCurrent() {
    const n = newName.trim();
    if (!n || !doc) return;
    const s = app.snap, last = app.policy?.last ?? {};
    const settings = {
      power_profile: val(s?.power?.profile) ?? undefined,
      boost_enabled: val(s?.cpu?.boost) ?? undefined,
      cpu_max_mhz: val(s?.cpu?.max_freq_mhz) ?? undefined,
      cpu_epp: val(s?.cpu?.epp) ?? undefined,
      gpu_max_mhz: last.gpu_max_mhz ?? undefined,
      fan: last.fan ?? { mode: 'auto' },
      ...(last.rgb_map ? { rgb_map: last.rgb_map } : last.rgb ? { rgb: last.rgb } : {}),
      ...(last.brightness != null ? { brightness: last.brightness } : {}),
    };
    const next = $state.snapshot(doc);
    next.profiles[n] = JSON.parse(JSON.stringify(settings));
    saveDoc(next, `"${n}" kaydedildi.`);
    newName = '';
  }

  // ---------- JSON içe/dışa aktarma ----------
  async function exportDoc() {
    try {
      const path = await invoke('export_file', { name: 'r9t-profiller.json', content: JSON.stringify($state.snapshot(doc), null, 2) });
      toast(`Dışa aktarıldı: ${path}`);
    } catch (e) {
      toast(String(e), 'bad');
    }
  }
  async function importFile(e) {
    const file = e.currentTarget.files?.[0];
    e.currentTarget.value = '';
    if (!file || !doc) return;
    let data;
    try {
      data = JSON.parse(await file.text());
    } catch {
      toast('Dosya JSON değil.', 'bad');
      return;
    }
    const incoming = data?.profiles && typeof data.profiles === 'object' ? data.profiles : null;
    if (!incoming || Array.isArray(incoming)) {
      toast('Dosyada "profiles" nesnesi yok (r9t-profiller.json biçimi bekleniyor).', 'bad');
      return;
    }
    const next = $state.snapshot(doc);
    const added = [];
    for (const [n, p] of Object.entries(incoming)) {
      let name = n;
      for (let i = 2; next.profiles[name]; i++) name = `${n} (${i})`;
      next.profiles[name] = p;
      added.push(name);
    }
    // r9td belgeyi bütünüyle doğrular; geçersiz profil varsa hiçbir şey yazılmaz.
    saveDoc(next, `${added.length} profil içe aktarıldı: ${added.join(', ')}`);
  }

  // ---------- 60 sn kararlılık testi ve A/B kıyas ----------
  const METRICS = [
    ['cpu_c', 'CPU EC sıcaklığı', '°C'],
    ['gpu_c', 'GPU EC sıcaklığı', '°C'],
    ['cpu_w', 'CPU paket gücü', 'W'],
    ['gpu_w', 'NVIDIA gücü', 'W'],
    ['cpu_mhz', 'CPU frekansı', 'MHz'],
    ['cpu_rpm', 'CPU fanı', 'RPM'],
    ['gpu_rpm', 'GPU fanı', 'RPM'],
  ];
  const DURATION = 60;
  let run = $state(null);
  let results = $state({ A: null, B: null });
  let lastRun = $state(null);

  function sample(s) {
    return {
      cpu_c: val(s.cpu.ec_temp_c),
      gpu_c: val(s.gpu.ec_temp_c),
      cpu_w: val(s.cpu.package_w),
      gpu_w: val(s.gpu.power_w),
      cpu_mhz: val(s.cpu.freq_mhz),
      cpu_rpm: val(s.fans.cpu_rpm),
      gpu_rpm: val(s.fans.gpu_rpm),
      guard: !!s.authority.guard_active || !!val(s.fans.thermal_override),
    };
  }

  function summarize(rows, label) {
    const m = {};
    for (const [k] of METRICS) {
      const v = rows.map((r) => r[k]).filter((x) => x != null && Number.isFinite(x));
      m[k] = { avg: v.length ? v.reduce((a, b) => a + b, 0) / v.length : null, peak: v.length ? Math.max(...v) : null, count: v.length };
    }
    // Plato: son 30 sn'de CPU ve GPU sıcaklığının aralığı ≤ 3 °C.
    const tail = rows.slice(-30);
    const spread = (k) => {
      const v = tail.map((r) => r[k]).filter((x) => x != null);
      return v.length ? Math.max(...v) - Math.min(...v) : null;
    };
    const missing = rows.filter((r) => r.cpu_c == null || r.gpu_c == null).length;
    const guard = rows.filter((r) => r.guard).length;
    const sc = spread('cpu_c'), sg = spread('gpu_c');
    const notes = [];
    if (guard) notes.push(`${guard} örnekte termal koruma etkindi.`);
    if (missing > rows.length * 0.1) notes.push(`${missing} örnekte sıcaklık eksikti.`);
    if (sc == null || sg == null || sc > 3 || sg > 3) notes.push(`Sıcaklık henüz oturmadı (son 30 sn aralığı CPU ${fmt(sc, 1)} / GPU ${fmt(sg, 1)} °C).`);
    return {
      label,
      profile: app.snap?.authority?.active_profile ?? POWER_NAMES[val(app.snap?.power?.profile)] ?? '—',
      at: new Date().toLocaleTimeString('tr-TR'),
      samples: rows.length,
      metrics: m,
      stable: notes.length === 0,
      notes,
    };
  }

  function startRun() {
    if (run) return;
    const rows = [];
    let seq = -1;
    run = { left: DURATION, rows: 0 };
    const t0 = Date.now();
    const id = setInterval(() => {
      const s = app.snap;
      if (s && s.sequence !== seq) {
        seq = s.sequence;
        rows.push(sample(s));
      }
      const left = Math.max(0, DURATION - Math.floor((Date.now() - t0) / 1000));
      run = { left, rows: rows.length, id };
      if (left === 0) {
        clearInterval(id);
        run = null;
        lastRun = summarize(rows, 'son');
      }
    }, 500);
    run.id = id;
  }
  function stopRun() {
    if (run?.id) clearInterval(run.id);
    run = null;
  }
  $effect(() => () => stopRun());

  const diff = (a, b) => (a == null || b == null ? null : b - a);
  const abWarnings = $derived.by(() => {
    const { A, B } = results;
    if (!A || !B) return [];
    const w = ['İş yükü kontrol edilmez ve FPS ölçülmez; sonuçlar yalnız bilgi amaçlıdır.'];
    if (Math.max(A.samples, B.samples) / Math.max(1, Math.min(A.samples, B.samples)) > 1.5) w.push(`A (${A.samples}) ve B (${B.samples}) örnek sayıları çok farklı.`);
    if (!A.stable || !B.stable) w.push('Pencerelerden en az biri kararlı değil; farklar yanıltıcı olabilir.');
    return w;
  });

  async function exportCsv() {
    const { A, B } = results;
    const lines = [['metrik', 'birim', 'A_ort', 'B_ort', 'fark_ort', 'A_tepe', 'B_tepe', 'fark_tepe'].join(',')];
    const f = (v) => (v == null ? '' : v.toFixed(2));
    for (const [k, t, u] of METRICS) {
      const a = A.metrics[k], b = B.metrics[k];
      lines.push([t, u, f(a.avg), f(b.avg), f(diff(a.avg, b.avg)), f(a.peak), f(b.peak), f(diff(a.peak, b.peak))].join(','));
    }
    lines.push('', `A,${A.profile},${A.samples} örnek`, `B,${B.profile},${B.samples} örnek`, '', `not,"${abWarnings[0] ?? ''}"`);
    try {
      toast(`CSV: ${await invoke('export_file', { name: 'r9t-kiyas.csv', content: lines.join('\n') })}`);
    } catch (e) {
      toast(String(e), 'bad');
    }
  }
</script>

<section class="card">
  <div class="spread">
    <h3>Kayıtlı profiller <span class="badge">{names.length}</span></h3>
    <div class="row tight">
      <button disabled={!doc} onclick={exportDoc}>JSON dışa aktar</button>
      <label class="filebtn" class:disabled={!ctl}>JSON içe aktar<input type="file" accept=".json,application/json" disabled={!ctl} onchange={importFile} /></label>
    </div>
  </div>
  {#if !doc}
    <small>Politika okunamadı.</small>
  {:else if !names.length}
    <small>Henüz profil yok. Aşağıdan mevcut durumu kaydedin ya da eski ayarları <code>r9tctl migrate</code> ile aktarın.</small>
  {:else}
    <div class="scroll-x">
      <table class="cmp">
        <thead>
          <tr>
            <th></th>
            {#each names as n (n)}<th class:on={app.snap?.authority?.active_profile === n}>{n}</th>{/each}
          </tr>
        </thead>
        <tbody>
          {#each ROWS as [t, f] (t)}
            <tr>
              <th>{t}</th>
              {#each names as n (n)}
                <td>
                  {#if t === 'Klavye' && doc.profiles[n].rgb}<i class="sw" style:background={rgbBox(doc.profiles[n].rgb)}></i>{/if}
                  {f(doc.profiles[n])}
                </td>
              {/each}
            </tr>
          {/each}
          <tr>
            <th></th>
            {#each names as n (n)}
              <td>
                <div class="row tight">
                  <button class="small primary" disabled={!ctl} onclick={() => control('apply_profile', { name: n }, { key: 'apply' })}>Uygula</button>
                  <button class="small danger" disabled={!ctl} onclick={() => removeProfile(n)}>Sil</button>
                </div>
              </td>
            {/each}
          </tr>
        </tbody>
      </table>
    </div>
  {/if}
  <div class="row" style="margin-top:10px">
    <input placeholder="Yeni profil adı" bind:value={newName} maxlength="48" style="width:220px" />
    <button disabled={!ctl || !newName.trim() || !doc} onclick={saveCurrent}>Mevcut durumu kaydet</button>
    <small>Güç profili, boost, CPU tavanı ve EPP canlı değerden; fan, GPU tavanı ve klavye son uygulanandan alınır.</small>
  </div>
</section>

<section class="card test">
  <div class="spread">
    <h3>60 sn kararlılık testi</h3>
    <div class="row tight">
      {#if run}
        <span class="badge warn num">{run.left} sn · {run.rows} örnek</span>
        <button onclick={stopRun}>İptal</button>
      {:else}
        <button class="primary" disabled={!app.connected} onclick={startRun}>Ölçümü başlat</button>
      {/if}
    </div>
  </div>
  <p class="muted">Seçili profil ve iş yükü sırasında 60 sn boyunca sıcaklık, güç, frekans ve fanları örnekler. İş yükünü kendiniz başlatın (oyun, derleme…); test donanıma yazmaz.</p>
  {#if lastRun}
    <div class="last">
      <div class="spread">
        <b>Son ölçüm: {lastRun.profile} · {lastRun.at} · {lastRun.samples} örnek</b>
        <div class="row tight">
          <span class="badge {lastRun.stable ? 'ok' : 'warn'}">{lastRun.stable ? 'kararlı' : 'kararsız'}</span>
          <button class="small" onclick={() => (results.A = { ...lastRun, label: 'A' })}>A olarak sakla</button>
          <button class="small" onclick={() => (results.B = { ...lastRun, label: 'B' })}>B olarak sakla</button>
        </div>
      </div>
      {#each lastRun.notes as n (n)}<div class="note" style="margin-top:6px">{n}</div>{/each}
    </div>
  {/if}

  {#if results.A || results.B}
    <div class="scroll-x" style="margin-top:12px">
      <table>
        <thead>
          <tr>
            <th>Metrik</th>
            <th class="r">A ort. {results.A ? `(${results.A.profile})` : ''}</th>
            <th class="r">B ort. {results.B ? `(${results.B.profile})` : ''}</th>
            <th class="r">Fark</th>
            <th class="r">A tepe</th>
            <th class="r">B tepe</th>
            <th class="r">Fark</th>
          </tr>
        </thead>
        <tbody>
          {#each METRICS as [k, t, u] (k)}
            {@const a = results.A?.metrics[k]}
            {@const b = results.B?.metrics[k]}
            {@const d = diff(a?.avg, b?.avg)}
            {@const dp = diff(a?.peak, b?.peak)}
            <tr>
              <td>{t} <small class="faint">{u}</small></td>
              <td class="r">{fmt(a?.avg, 1)}</td>
              <td class="r">{fmt(b?.avg, 1)}</td>
              <td class="r" class:up={d > 0} class:down={d < 0}>{d == null ? '—' : `${d > 0 ? '+' : ''}${fmt(d, 1)}`}</td>
              <td class="r">{fmt(a?.peak, 1)}</td>
              <td class="r">{fmt(b?.peak, 1)}</td>
              <td class="r" class:up={dp > 0} class:down={dp < 0}>{dp == null ? '—' : `${dp > 0 ? '+' : ''}${fmt(dp, 1)}`}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <div class="stack" style="margin-top:8px">
      {#each abWarnings as w (w)}<div class="note">{w}</div>{/each}
    </div>
    <div class="row" style="margin-top:8px">
      <button disabled={!results.A || !results.B} onclick={exportCsv}>CSV dışa aktar</button>
      <button onclick={() => (results = { A: null, B: null })}>Temizle</button>
    </div>
  {/if}
</section>

<style>
  .cmp th { white-space: nowrap; }
  .cmp thead th.on { color: var(--accent-2); }
  .cmp td { white-space: nowrap; }
  .sw { display: inline-block; width: 12px; height: 12px; border-radius: 3px; margin-right: 6px; vertical-align: -1px; border: 1px solid #0006; }
  .filebtn { position: relative; overflow: hidden; display: inline-block; padding: 6px 12px; border: 1px solid var(--border); border-radius: 8px; background: var(--panel-2); cursor: pointer; }
  .filebtn:hover { border-color: #3b4352; }
  .filebtn.disabled { opacity: 0.45; cursor: not-allowed; }
  .filebtn input { position: absolute; inset: 0; opacity: 0; cursor: pointer; }
  .test { margin-top: var(--gap); }
  .last { margin-top: 10px; padding: 10px; border: 1px solid var(--border); border-radius: 8px; background: var(--panel-2); }
  .up { color: var(--orange); }
  .down { color: var(--green); }
</style>
