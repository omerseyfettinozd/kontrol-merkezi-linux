<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { app, val, canControl, control, session, cap, toast, read } from '../state.svelte.js';

  const ROWS = 6, COLS = 21, N = ROWS * COLS;
  const ctl = $derived(canControl() && cap('rgb').ok);
  const max = $derived(app.caps?.probe?.readings?.rgb?.max ?? 50);
  const level = $derived(val(app.snap?.rgb_brightness));

  let keymap = $state(null);
  let frame = $state(Array.from({ length: N }, () => '#ffffff'));
  let color = $state('#e63946');
  let brightness = $state(40);
  let painting = false;
  let seeded = false;
  let breathPeriod = $state(3000);
  let waveSpeed = $state(5);

  const hex = ([r, g, b]) => '#' + [r, g, b].map((v) => v.toString(16).padStart(2, '0')).join('');
  const rgb = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));

  onMount(async () => {
    try {
      keymap = await invoke('keymap');
    } catch {
      keymap = null;
    }
  });

  // Izgara klavyenin gerçek renkleriyle (sysfs geri okuması) başlar; okunamazsa son uygulanan harita.
  $effect(() => {
    if (seeded || !app.connected) return;
    seeded = true;
    read('rgb_state')
      .then((r) => {
        if (r.colors?.length === N) frame = r.colors.map(hex);
        if (r.brightness != null && r.max) brightness = Math.round((r.brightness / r.max) * 100);
      })
      .catch(() => {
        const last = app.policy?.last;
        if (last?.rgb_map?.length === N) frame = last.rgb_map.map(hex);
        else if (last?.rgb) frame = Array.from({ length: N }, () => hex(last.rgb));
        if (last?.brightness != null) brightness = last.brightness;
      });
  });

  const label = (i) => keymap?.keys?.[i]?.label ?? '';
  const calibrated = $derived(keymap?.verified === true);

  function paint(i) {
    frame[i] = color;
  }
  // Boyama kap üzerinden: işaretçi yakalanır, iki olay arasındaki tuşlar ızgarada
  // doğru boyunca doldurulur (hızlı sürüklemede boşluk kalmaz).
  let lastKey = -1;
  const keyAt = (e) => {
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest?.('[data-i]');
    return el ? Number(el.dataset.i) : -1;
  };
  function line(a, b) {
    let [r0, c0] = [Math.floor(a / COLS), a % COLS];
    const [r1, c1] = [Math.floor(b / COLS), b % COLS];
    const n = Math.max(Math.abs(r1 - r0), Math.abs(c1 - c0));
    for (let k = 0; k <= n; k++) {
      const r = Math.round(r0 + ((r1 - r0) * k) / (n || 1));
      const c = Math.round(c0 + ((c1 - c0) * k) / (n || 1));
      paint(r * COLS + c);
    }
  }
  function down(e) {
    const i = keyAt(e);
    if (i < 0) return;
    e.preventDefault();
    if (e.button === 2) {
      color = frame[i];
      return;
    }
    painting = true;
    lastKey = i;
    paint(i);
  }
  function move(e) {
    if (!painting) return;
    const i = keyAt(e);
    if (i < 0 || i === lastKey) return;
    line(lastKey, i);
    lastKey = i;
  }
  function up(e) {
    if (painting) move(e);
    painting = false;
    lastKey = -1;
  }
  // Bırakma ızgara dışında olsa da boyama biter (yakalama kullanılmaz: kaçan bir
  // pointerup diğer düğmeleri kilitlemesin).

  function fill(pred) {
    for (let i = 0; i < N; i++) if (pred(Math.floor(i / COLS), i % COLS)) frame[i] = color;
  }
  const GROUPS = [
    { name: 'Tümü', f: () => true },
    ...Array.from({ length: ROWS }, (_, r) => ({ name: `Satır ${r + 1}`, f: (row) => row === r })),
    { name: 'Sütun 1–8', f: (_, c) => c < 8 },
    { name: 'Sütun 9–16', f: (_, c) => c >= 8 && c < 16 },
    { name: 'Sütun 17–21', f: (_, c) => c >= 16 },
  ];

  const apply = () => control('rgb_map', { colors: frame.map(rgb), brightness: Number(brightness) }, { key: 'rgb' });

  async function audio(on) {
    if (on) {
      const r = await control('rgb_effect', { effect: { kind: 'audio_reactive' } }, { key: 'effect', quiet: true });
      if (!r) return;
      try {
        await session('audio_reactive', { enabled: true });
        toast('Ses tepkili aydınlatma açıldı (varsayılan çıkışın monitörü).');
      } catch {
        /* bildirim gösterildi */
      }
    } else {
      try {
        await session('audio_reactive', { enabled: false }, { notify: false });
      } catch {
        /* oturum yoksa zaten kapalı */
      }
      apply();
    }
  }
</script>

<svelte:window onpointerup={up} onblur={() => (painting = false)} />

<div class="grid cols-2">
  <section class="card wide">
    <div class="spread">
      <h3>Klavye haritası <span class="badge">{N} LED · 6×21</span>
        {#if calibrated}<span class="badge ok">TR-Q kalibre</span>{:else}<span class="badge warn">TR-Q haritası kalibre edilmedi</span>{/if}
      </h3>
      <div class="row tight">
        <input type="color" bind:value={color} aria-label="Fırça rengi" />
        {#each ['#ffffff', '#e63946', '#f0883e', '#e3b341', '#3fb950', '#39c5cf', '#4ea1ff', '#b083f0', '#000000'] as c (c)}
          <button class="swatch" style:background={c} aria-label="Renk {c}" onclick={() => (color = c)} class:sel={color === c}></button>
        {/each}
      </div>
    </div>
    <div
      class="kbd"
      role="group"
      aria-label="Klavye LED ızgarası"
      oncontextmenu={(e) => e.preventDefault()}
      onpointerdown={down}
      onpointermove={move}
      onpointercancel={up}
    >
      {#each frame as c, i (i)}
        <button
          class="key"
          style:background={c}
          style:color={rgb(c).reduce((a, b) => a + b, 0) > 380 ? '#111' : '#eee'}
          data-i={i}
          aria-label="LED {i} (satır {Math.floor(i / COLS) + 1}, sütun {(i % COLS) + 1}) {label(i)}"
          onclick={() => paint(i)}>{label(i)}</button>
      {/each}
    </div>
    <div class="row tight groups">
      <small>Hızlı gruplar:</small>
      {#each GROUPS as g (g.name)}<button class="small" onclick={() => fill(g.f)}>{g.name}</button>{/each}
    </div>
    <div class="row">
      <button class="primary" disabled={!ctl} onclick={apply}>Haritayı uygula</button>
      <button disabled={!ctl} onclick={() => control('rgb', { rgb: rgb(color), brightness: Number(brightness) }, { key: 'rgb' })}>Tek renk: tümü</button>
      <small>Sol tık/sürükle: boya · sağ tık: renk al. {calibrated ? '' : 'Tuş etiketleri kalibrasyondan sonra görünür (r9tctl keymap-calibrate).'}</small>
    </div>
  </section>

  <section class="card">
    <h3>Global parlaklık <small>tüm LED'ler (0–{max})</small></h3>
    <div class="row"><input type="range" min="0" max="100" bind:value={brightness} disabled={!ctl} /><b class="num">%{brightness}</b></div>
    <div class="row">
      <button disabled={!ctl} onclick={() => control('rgb_brightness', { brightness: Number(brightness) }, { key: 'bright' })}>Parlaklığı uygula</button>
      <small>Şu an: {level ?? '—'} / {max}</small>
    </div>
  </section>

  <section class="card">
    <h3>Efektler <small>pilde, boşta ve kapak kapalıyken durur</small></h3>
    <div class="stack">
      <div class="row">
        <button disabled={!ctl} onclick={apply}>Statik (harita)</button>
        <button disabled={!ctl} onclick={() => control('rgb_effect', { effect: { kind: 'thermal_gradient' } }, { key: 'effect' })}>Termal gradyan</button>
      </div>
      <div class="row">
        <button disabled={!ctl} onclick={() => control('rgb_effect', { effect: { kind: 'breathing', rgb: rgb(color), period_ms: Number(breathPeriod) } }, { key: 'effect' })}>Nefes</button>
        <input type="range" min="1000" max="8000" step="250" bind:value={breathPeriod} style="max-width:200px" aria-label="Nefes periyodu" />
        <small class="num">{(breathPeriod / 1000).toFixed(2)} sn · fırça rengi</small>
      </div>
      <div class="row">
        <button disabled={!ctl} onclick={() => control('rgb_effect', { effect: { kind: 'wave', speed: Number(waveSpeed) } }, { key: 'effect' })}>Dalga</button>
        <input type="range" min="1" max="10" bind:value={waveSpeed} style="max-width:200px" aria-label="Dalga hızı" />
        <small class="num">hız {waveSpeed}</small>
      </div>
      <div class="row">
        <button disabled={!ctl || !app.session} onclick={() => audio(true)}>Ses tepkili: aç</button>
        <button disabled={!app.session?.audio_reactive} onclick={() => audio(false)}>kapat</button>
        <small>{app.session ? (app.session.audio_reactive ? 'PipeWire yakalama açık' : 'r9t-session hazır') : 'r9t-session gerekli'}</small>
      </div>
    </div>
  </section>
</div>

<style>
  h3 small { font-weight: 400; color: var(--muted); font-size: 12px; }
  .wide { grid-column: 1 / -1; }
  .kbd { display: grid; grid-template-columns: repeat(21, minmax(0, 1fr)); gap: 4px; margin: 12px 0; user-select: none; touch-action: none; }
  .key { aspect-ratio: 1.15; padding: 0; border-radius: 5px; border: 1px solid #00000066; font-size: 10px; font-weight: 600; overflow: hidden; min-width: 0; }
  .key:hover:not(:disabled) { border-color: #fff; }
  .swatch { width: 24px; height: 24px; padding: 0; border-radius: 6px; }
  .swatch.sel { outline: 2px solid #fff; outline-offset: 1px; }
  .groups { margin-bottom: 10px; }
</style>
