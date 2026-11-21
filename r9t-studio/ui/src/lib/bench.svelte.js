// 60 sn kararlılık testi ve A/B kıyas durumu. Bileşenin dışında tutulur: sekme
// değişince Profiller görünümü yeniden yaratılır, ölçüm ve saklanan sonuçlar
// bundan etkilenmemeli.
import { app, val, fmt, read, toast, POWER_NAMES } from './state.svelte.js';

export const METRICS = [
  ['cpu_c', 'CPU EC sıcaklığı', '°C'],
  ['gpu_c', 'GPU EC sıcaklığı', '°C'],
  ['cpu_w', 'CPU paket gücü', 'W'],
  ['gpu_w', 'NVIDIA gücü', 'W'],
  ['cpu_mhz', 'CPU frekansı', 'MHz'],
  ['cpu_rpm', 'CPU fanı', 'RPM'],
  ['gpu_rpm', 'GPU fanı', 'RPM'],
];
export const DURATION = 60;

class BenchState {
  run = $state(null);
  results = $state({ A: null, B: null });
  lastRun = $state(null);
}
export const bench = new BenchState();

function sample(s) {
  return {
    t: s.taken_at_ms,
    cpu_c: val(s.cpu.ec_temp_c),
    gpu_c: val(s.gpu.ec_temp_c),
    cpu_w: val(s.cpu.package_w),
    gpu_w: val(s.gpu.power_w),
    cpu_mhz: val(s.cpu.freq_mhz),
    cpu_rpm: val(s.fans.cpu_rpm),
    gpu_rpm: val(s.fans.gpu_rpm),
    guard: !!s.authority?.guard_active || !!val(s.fans.thermal_override),
  };
}

/** r9td'nin 1 Hz geçmişinden satırlar. Odak dışında UI 3–5 sn'de bir örnek
 *  aldığı için asıl seri budur; geçmişte olmayan MHz ve koruma bayrağı zamanca
 *  en yakın canlı örnekten alınır. Geçmiş yoksa canlı örneklerle yetinilir. */
async function rowsFromHistory(live, t0, t1) {
  let pts = [];
  try {
    const h = await read('history', { seconds: DURATION + 5 });
    pts = (h.points || []).filter((p) => p.t_ms >= t0 && p.t_ms <= t1);
  } catch {
    /* canlı örneklere düş */
  }
  if (pts.length < live.length) return live;
  const near = (t, ms) => {
    let best = null;
    for (const r of live) if (Math.abs(r.t - t) <= ms && (!best || Math.abs(r.t - t) < Math.abs(best.t - t))) best = r;
    return best;
  };
  return pts.map((p) => {
    const l = near(p.t_ms, 3000);
    return {
      t: p.t_ms,
      cpu_c: p.cpu_c,
      gpu_c: p.gpu_c,
      cpu_w: p.cpu_w,
      gpu_w: p.gpu_w,
      cpu_mhz: l?.cpu_mhz ?? null,
      cpu_rpm: p.cpu_rpm,
      gpu_rpm: p.gpu_rpm,
      // Alarmda Rust her örneği yollar; korumalı örnek ±1 sn içinde görünür.
      guard: !!near(p.t_ms, 1000)?.guard,
    };
  });
}

function summarize(rows, label, profile) {
  const m = {};
  for (const [k] of METRICS) {
    const v = rows.map((r) => r[k]).filter((x) => x != null && Number.isFinite(x));
    m[k] = { avg: v.length ? v.reduce((a, b) => a + b, 0) / v.length : null, peak: v.length ? Math.max(...v) : null, count: v.length };
  }
  // Plato: son 30 sn'de CPU ve GPU sıcaklığının aralığı ≤ 3 °C.
  const end = rows.length ? rows[rows.length - 1].t : 0;
  const tail = rows.filter((r) => end - r.t <= 30000);
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
  return { label, profile, at: new Date().toLocaleTimeString('tr-TR'), samples: rows.length, metrics: m, stable: notes.length === 0, notes };
}

export function startRun() {
  if (bench.run) return;
  const live = [];
  let seq = -1;
  const t0 = Date.now();
  const id = setInterval(async () => {
    const s = app.snap;
    if (s && s.sequence !== seq) {
      seq = s.sequence;
      live.push(sample(s));
    }
    const left = Math.max(0, DURATION - Math.floor((Date.now() - t0) / 1000));
    if (left > 0) {
      bench.run = { left, rows: live.length, id };
      return;
    }
    clearInterval(id);
    bench.run = { left: 0, rows: live.length, id: null, finishing: true };
    const profile = app.snap?.authority?.active_profile ?? POWER_NAMES[val(app.snap?.power?.profile)] ?? '—';
    const rows = await rowsFromHistory(live, t0, Date.now());
    bench.lastRun = summarize(rows, 'son', profile);
    bench.run = null;
    if (app.view !== 'profiles') toast('60 sn ölçüm bitti; sonuç Profiller sayfasında.');
  }, 500);
  bench.run = { left: DURATION, rows: 0, id };
}

export function stopRun() {
  if (bench.run?.id) clearInterval(bench.run.id);
  bench.run = null;
}
