<script>
  // Çift eksenli Canvas çizgi grafiği. Yalnız yeni veri geldiğinde (app.tick) ya da
  // boyut değişince çizer; sürekli animasyon yok (docs/08: boşta CPU).
  import { app, history, fmt } from './state.svelte.js';

  let { series, leftUnit = '', rightUnit = '', height = 190, windowS = 300, leftMin = null, rightMin = 0, title = '' } = $props();

  let canvas = $state();
  let width = $state(600);

  function range(keys, min) {
    let lo = Infinity, hi = -Infinity;
    for (const p of history.points) {
      for (const k of keys) {
        const v = p[k];
        if (v != null && Number.isFinite(v)) {
          if (v < lo) lo = v;
          if (v > hi) hi = v;
        }
      }
    }
    if (!Number.isFinite(lo)) return [0, 1];
    if (min != null) lo = Math.min(lo, min);
    const pad = Math.max((hi - lo) * 0.1, 1);
    return [min != null ? lo : Math.floor(lo - pad), Math.ceil(hi + pad)];
  }

  function draw() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(200, width), h = height;
    // Tuval yalnız boyut değişince yeniden ayrılır (her karede ayırmak pahalı).
    const cw = Math.round(w * dpr), ch = Math.round(h * dpr);
    if (canvas.width !== cw || canvas.height !== ch) {
      canvas.width = cw;
      canvas.height = ch;
    }
    const g = canvas.getContext('2d');
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, h);
    const padL = 44, padR = rightUnit ? 50 : 12, padT = 20, padB = 22;
    const pw = w - padL - padR, ph = h - padT - padB;
    const now = history.points.length ? history.points[history.points.length - 1].t : Date.now();
    const t0 = now - windowS * 1000;
    const left = series.filter((s) => s.axis !== 'r');
    const right = series.filter((s) => s.axis === 'r');
    const [l0, l1] = range(left.map((s) => s.key), leftMin);
    const [r0, r1] = right.length ? range(right.map((s) => s.key), rightMin) : [0, 1];
    const x = (t) => padL + ((t - t0) / (windowS * 1000)) * pw;
    const yl = (v) => padT + ph - ((v - l0) / (l1 - l0 || 1)) * ph;
    const yr = (v) => padT + ph - ((v - r0) / (r1 - r0 || 1)) * ph;

    g.font = '11px Inter, "Noto Sans", sans-serif';
    g.lineWidth = 1;
    g.strokeStyle = '#262c37';
    g.fillStyle = '#9aa4b5';
    for (let i = 0; i <= 4; i++) {
      const yy = padT + (ph * i) / 4;
      g.beginPath();
      g.moveTo(padL, yy + 0.5);
      g.lineTo(padL + pw, yy + 0.5);
      g.stroke();
      const lv = l1 - ((l1 - l0) * i) / 4;
      g.textAlign = 'right';
      g.fillText(fmt(lv, l1 - l0 < 8 ? 1 : 0), padL - 6, yy + 4);
      if (right.length) {
        const rv = r1 - ((r1 - r0) * i) / 4;
        g.textAlign = 'left';
        g.fillText(fmt(rv, r1 - r0 < 8 ? 1 : 0), padL + pw + 6, yy + 4);
      }
    }
    g.textAlign = 'left';
    g.fillText(leftUnit, 4, 11);
    if (rightUnit) {
      g.textAlign = 'right';
      g.fillText(rightUnit, w - 4, 11);
    }
    g.textAlign = 'center';
    for (let m = 0; m <= windowS / 60; m++) {
      const xx = x(now - m * 60000);
      if (xx < padL - 1) break;
      g.fillText(m === 0 ? 'şimdi' : `-${m} dk`, xx, h - 6);
    }
    for (const s of series) {
      const yf = s.axis === 'r' ? yr : yl;
      g.strokeStyle = s.color;
      g.lineWidth = s.axis === 'r' ? 1.2 : 1.8;
      g.setLineDash(s.dash ? [4, 3] : []);
      g.beginPath();
      // Eksik değer çizgiyi kesmez (yavaş örneklenen alanlar); yalnız `maxGapMs`'den
      // uzun boşluk keser (odak dışında örnekler 5 sn arayla gelir). Eksik değer asla
      // 0 olarak çizilmez.
      const maxGap = s.maxGapMs ?? 6500;
      let lastT = null;
      for (const p of history.points) {
        if (p.t < t0) continue;
        const v = p[s.key];
        if (v == null || !Number.isFinite(v)) continue;
        if (lastT != null && p.t - lastT <= maxGap) g.lineTo(x(p.t), yf(v));
        else g.moveTo(x(p.t), yf(v));
        lastT = p.t;
      }
      g.stroke();
    }
    g.setLineDash([]);
  }

  $effect(() => {
    void app.tick;
    void width;
    draw();
  });

  const last = (k) => {
    for (let i = history.points.length - 1; i >= 0; i--) {
      const v = history.points[i][k];
      if (v != null) return v;
    }
    return null;
  };
</script>

<div class="chart" bind:clientWidth={width}>
  <div class="legend">
    {#if title}<span class="title">{title}</span>{/if}
    {#each series as s (s.key)}
      <span class="item"><i style:background={s.color}></i>{s.label} <b class="num">{fmt((app.tick, last(s.key)), s.digits ?? 0)}</b></span>
    {/each}
  </div>
  <canvas bind:this={canvas} style:height="{height}px" aria-label={title || 'grafik'}></canvas>
</div>

<style>
  .chart { width: 100%; min-width: 0; }
  canvas { width: 100%; display: block; }
  .legend { display: flex; flex-wrap: wrap; gap: 4px 14px; font-size: 12px; color: var(--muted); margin-bottom: 4px; }
  .title { color: var(--text); font-weight: 600; margin-right: 6px; }
  .item { display: inline-flex; align-items: center; gap: 5px; }
  .item i { width: 10px; height: 3px; border-radius: 2px; display: inline-block; }
  .item b { color: var(--text); font-weight: 600; }
</style>
