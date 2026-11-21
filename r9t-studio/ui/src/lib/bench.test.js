// 60 sn ölçüm: odak dışında UI 5 sn'de bir görüntü alsa da sonuç r9td'nin 1 Hz
// geçmişinden gelmeli; sekme değişimi (bileşen yok olsa da) ölçümü bozmamalı.
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const state = vi.hoisted(() => ({ history: null, toasts: [] }));
vi.mock('./state.svelte.js', () => ({
  app: { snap: null, view: 'overview' },
  val: (f) => (f && f.value !== undefined ? f.value : null),
  fmt: (x, d = 0) => (x == null ? '—' : x.toFixed(d)),
  POWER_NAMES: { balanced: 'Dengeli' },
  toast: (t) => state.toasts.push(t),
  read: async () => {
    if (state.history instanceof Error) throw state.history;
    return state.history;
  },
}));

const { app } = await import('./state.svelte.js');
const { bench, startRun, stopRun, DURATION } = await import('./bench.svelte.js');

const mk = (seq, guard = false) => ({
  sequence: seq,
  taken_at_ms: Date.now(),
  cpu: { ec_temp_c: { value: 80 }, package_w: { value: 50 }, freq_mhz: { value: 4200 } },
  gpu: { ec_temp_c: { value: 70 }, power_w: { value: 60 } },
  fans: { cpu_rpm: { value: 4000 }, gpu_rpm: { value: 4100 }, thermal_override: { value: false } },
  authority: { guard_active: guard },
  power: { profile: { value: 'balanced' } },
});

function historyFor(t0, n, temp = () => 80) {
  return { points: Array.from({ length: n }, (_, i) => ({ t_ms: t0 + i * 1000, cpu_c: temp(i), gpu_c: 70, cpu_w: 50, gpu_w: 60, cpu_rpm: 4000, gpu_rpm: 4100 })) };
}

/** Odak dışı: 5 sn'de bir yeni görüntü; süre dolana kadar saati ilerletir. */
async function runFor(seconds, { guardAt = -1 } = {}) {
  let seq = 0;
  app.snap = mk(seq);
  startRun();
  for (let s = 0; s < seconds * 2; s++) {
    if (s % 10 === 0) app.snap = mk(++seq, seq === guardAt);
    await vi.advanceTimersByTimeAsync(500);
  }
  await vi.advanceTimersByTimeAsync(1000);
}

beforeEach(() => {
  vi.useFakeTimers();
  bench.run = null;
  bench.lastRun = null;
  bench.results = { A: null, B: null };
  state.toasts.length = 0;
});
afterEach(() => {
  stopRun();
  vi.useRealTimers();
});

describe('bench', () => {
  it('odak dışında da 1 Hz geçmişten ~60 örnek toplar', async () => {
    const t0 = Date.now();
    state.history = historyFor(t0, DURATION + 1);
    await runFor(DURATION, { guardAt: 3 });
    const r = bench.lastRun;
    expect(r).not.toBeNull();
    expect(r.samples).toBeGreaterThanOrEqual(55);
    expect(r.metrics.cpu_c.avg).toBe(80);
    // MHz geçmişte yok; en yakın canlı örnekten tamamlanır.
    expect(r.metrics.cpu_mhz.count).toBeGreaterThan(0);
    expect(r.notes.some((n) => n.includes('termal koruma'))).toBe(true);
    expect(bench.run).toBeNull();
    expect(state.toasts).toHaveLength(1);
  });

  it('geçmiş okunamazsa canlı örneklere düşer', async () => {
    state.history = new Error('bağlantı yok');
    await runFor(DURATION);
    expect(bench.lastRun.samples).toBeGreaterThanOrEqual(5);
    expect(bench.lastRun.samples).toBeLessThan(20);
  });

  it('plato son 30 sn zaman penceresiyle ölçülür', async () => {
    const t0 = Date.now();
    // İlk 30 sn ısınma (60→89 °C), son 30 sn sabit 90 °C: kararlı sayılmalı.
    state.history = historyFor(t0, DURATION + 1, (i) => (i < 30 ? 60 + i : 90));
    await runFor(DURATION);
    expect(bench.lastRun.notes.some((n) => n.includes('oturmadı'))).toBe(false);
  });

  it('iptal sonuç üretmez; A/B sonuçları korunur', async () => {
    bench.results.A = { label: 'A', samples: 60 };
    state.history = historyFor(Date.now(), 10);
    app.snap = mk(1);
    startRun();
    await vi.advanceTimersByTimeAsync(5000);
    stopRun();
    await vi.advanceTimersByTimeAsync(70000);
    expect(bench.run).toBeNull();
    expect(bench.lastRun).toBeNull();
    expect(bench.results.A.samples).toBe(60);
  });
});
