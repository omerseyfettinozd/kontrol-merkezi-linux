<script>
  // Sürüklenebilir fan eğrisi. Çift tık: nokta ekle; sağ tık: nokta sil.
  // Modül tabanının (`floor`, r9t_fan min_duty) altı taralı gösterilir; canlı CPU/GPU EC
  // sıcaklığı dikey imleçtir.
  import { DUTY_FLOOR, T_MIN, T_MAX, dutyAt, validate } from './curve.js';

  let { points = $bindable(), interpolation = $bindable('linear'), cpuTemp = null, gpuTemp = null, disabled = false, floor = 50 } = $props();
  const shade = $derived(Math.max(DUTY_FLOOR, floor));

  const W = 640, H = 300;
  const pad = { l: 44, r: 16, t: 14, b: 34 };
  const X0 = 30, X1 = 100;
  const pw = W - pad.l - pad.r, ph = H - pad.t - pad.b;
  const sx = (t) => pad.l + ((t - X0) / (X1 - X0)) * pw;
  const sy = (d) => pad.t + ph - (d / 100) * ph;

  let svg = $state();
  let drag = $state(-1);

  const error = $derived(validate(points));
  const path = $derived.by(() => {
    if (error) return points.map(([t, d], i) => `${i ? 'L' : 'M'}${sx(t)},${sy(d)}`).join(' ');
    const out = [`M${sx(X0)},${sy(points[0][1])}`];
    for (let t = X0; t <= X1; t += 0.5) out.push(`L${sx(t)},${sy(dutyAt(points, interpolation, t))}`);
    return out.join(' ');
  });

  function toData(ev) {
    const r = svg.getBoundingClientRect();
    const px = ((ev.clientX - r.left) / r.width) * W;
    const py = ((ev.clientY - r.top) / r.height) * H;
    return [X0 + ((px - pad.l) / pw) * (X1 - X0), ((pad.t + ph - py) / ph) * 100];
  }

  function onDown(i, ev) {
    if (disabled) return;
    ev.preventDefault();
    drag = i;
    ev.currentTarget.setPointerCapture?.(ev.pointerId);
  }

  function onMove(ev) {
    if (drag < 0 || disabled) return;
    let [t, d] = toData(ev);
    const prev = points[drag - 1], next = points[drag + 1];
    t = Math.round(Math.min(next ? next[0] - 1 : T_MAX, Math.max(prev ? prev[0] + 1 : T_MIN, t)));
    d = Math.round(Math.min(next ? next[1] : 100, Math.max(prev ? prev[1] : DUTY_FLOOR, d)));
    if (drag === points.length - 1) d = Math.max(d, prev ? prev[1] : DUTY_FLOOR);
    points[drag] = [t, d];
  }

  function onUp() {
    drag = -1;
  }

  function onDouble(ev) {
    if (disabled || points.length >= 10) return;
    let [t, d] = toData(ev);
    t = Math.round(t);
    if (t < T_MIN || t > T_MAX || points.some((p) => p[0] === t)) return;
    const i = points.findIndex((p) => p[0] > t);
    const at = i < 0 ? points.length : i;
    const lo = points[at - 1]?.[1] ?? DUTY_FLOOR, hi = points[at]?.[1] ?? 100;
    d = Math.round(Math.min(hi, Math.max(lo, d)));
    points.splice(at, 0, [t, d]);
  }

  function onRemove(i, ev) {
    ev.preventDefault();
    if (disabled || points.length <= 2) return;
    points.splice(i, 1);
  }

  function nudge(i, ev) {
    if (disabled) return;
    const step = { ArrowUp: [0, 1], ArrowDown: [0, -1], ArrowLeft: [-1, 0], ArrowRight: [1, 0] }[ev.key];
    if (!step) return;
    ev.preventDefault();
    const [t, d] = points[i];
    const prev = points[i - 1], next = points[i + 1];
    const nt = Math.min(next ? next[0] - 1 : T_MAX, Math.max(prev ? prev[0] + 1 : T_MIN, t + step[0]));
    const nd = Math.min(next ? next[1] : 100, Math.max(prev ? prev[1] : DUTY_FLOOR, d + step[1]));
    points[i] = [nt, nd];
  }

  const cursor = (temp, color, label) =>
    temp == null ? null : { x: sx(Math.min(X1, Math.max(X0, temp))), color, label: `${label} ${Math.round(temp)} °C`, d: error ? null : dutyAt(points, interpolation, temp) };
  const cursors = $derived([cursor(cpuTemp, '#ff6b74', 'CPU'), cursor(gpuTemp, '#4ea1ff', 'GPU')].filter(Boolean));
</script>

<div class="wrap">
  <svg
    bind:this={svg}
    viewBox="0 0 {W} {H}"
    role="application"
    aria-label="Fan eğrisi düzenleyici"
    class:disabled
    onpointermove={onMove}
    onpointerup={onUp}
    onpointerleave={onUp}
    ondblclick={onDouble}
  >
    <defs>
      <pattern id="floor" width="8" height="8" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
        <rect width="8" height="8" fill="#151920" />
        <line x1="0" y1="0" x2="0" y2="8" stroke="#2b313d" stroke-width="3" />
      </pattern>
    </defs>
    <rect x={pad.l} y={sy(shade)} width={pw} height={sy(0) - sy(shade)} fill="url(#floor)" />
    <text x={pad.l + 8} y={sy(shade) + 18} class="floor">{shade > DUTY_FLOOR ? `Modül tabanı %${shade} — altı %${shade}'e çekilir` : `Taban %${shade} — bu bölgeye inilmez`}</text>
    <rect x={sx(T_MIN)} y={pad.t} width={sx(T_MAX) - sx(T_MIN)} height={sy(DUTY_FLOOR) - pad.t} fill="#ffffff05" />
    {#each [0, 25, 50, 75, 100] as d}
      <line x1={pad.l} x2={pad.l + pw} y1={sy(d)} y2={sy(d)} class="grid" />
      <text x={pad.l - 6} y={sy(d) + 4} class="axis" text-anchor="end">%{d}</text>
    {/each}
    {#each [30, 40, 50, 60, 70, 80, 90, 100] as t}
      <line x1={sx(t)} x2={sx(t)} y1={pad.t} y2={pad.t + ph} class="grid" />
      <text x={sx(t)} y={H - 12} class="axis" text-anchor="middle">{t} °C</text>
    {/each}
    <line x1={sx(90)} x2={sx(90)} y1={pad.t} y2={pad.t + ph} class="limit" />
    {#each cursors as c (c.label)}
      <line x1={c.x} x2={c.x} y1={pad.t} y2={pad.t + ph} stroke={c.color} stroke-dasharray="4 3" stroke-width="1.5" />
      {#if c.d != null}<circle cx={c.x} cy={sy(c.d)} r="4" fill={c.color} />{/if}
      <text x={c.x + 5} y={c.color === '#ff6b74' ? pad.t + 12 : pad.t + 26} fill={c.color} class="cur">{c.label}{c.d != null ? ` → %${c.d}` : ''}</text>
    {/each}
    <path d={path} class="curve" class:invalid={!!error} />
    {#each points as [t, d], i}
      <circle
        cx={sx(t)}
        cy={sy(d)}
        r={drag === i ? 9 : 7}
        class="pt"
        class:active={drag === i}
        role="slider"
        tabindex={disabled ? -1 : 0}
        aria-label="Nokta {i + 1}: {t} °C, %{d}"
        aria-valuenow={d}
        onpointerdown={(e) => onDown(i, e)}
        oncontextmenu={(e) => onRemove(i, e)}
        onkeydown={(e) => nudge(i, e)}
      />
      {#if drag === i}<text x={sx(t)} y={sy(d) - 14} text-anchor="middle" class="tip">{t} °C · %{d}</text>{/if}
    {/each}
  </svg>
  <div class="spread foot">
    <small>Çift tık: nokta ekle · sağ tık: sil · ok tuşları: ince ayar · 2–10 nokta, en geç 90 °C'de %100</small>
    {#if error}<span class="badge bad">{error}</span>{:else}<span class="badge ok">Eğri geçerli</span>{/if}
  </div>
</div>

<style>
  .wrap { width: 100%; }
  svg { width: 100%; height: auto; display: block; touch-action: none; user-select: none; background: var(--panel-2); border-radius: 8px; border: 1px solid var(--border); }
  svg.disabled .pt { cursor: not-allowed; opacity: 0.6; }
  .grid { stroke: #262c37; stroke-width: 1; }
  .limit { stroke: #e3b34155; stroke-width: 1; stroke-dasharray: 2 4; }
  .axis { fill: #9aa4b5; font-size: 11px; }
  .floor { fill: #8b94a5; font-size: 11px; }
  .cur { font-size: 11px; font-weight: 600; }
  .tip { fill: #e8ebf1; font-size: 12px; font-weight: 600; }
  .curve { fill: none; stroke: #e63946; stroke-width: 2.5; }
  .curve.invalid { stroke: #e3b341; stroke-dasharray: 6 4; }
  .pt { fill: #0e1015; stroke: #ff6b74; stroke-width: 2.5; cursor: grab; }
  .pt.active { fill: #ff6b74; cursor: grabbing; }
  .pt:focus-visible { outline: none; stroke: #4ea1ff; }
  .foot { margin-top: 6px; }
</style>
