// dGPU belleği dolunca (ekran dGPU'da: yeni pencereler boş açılır) üst bant uyarır,
// Ekran sekmesi belleği tutan süreçleri adına göre toplayıp en büyükten sıralar.
import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import { tick } from 'svelte';
import { snap, goodCaps } from './fixtures.js';

const PROCS = [
  { pid: 10, kind: 'G', name: 'Hyprland', used_mib: 255, killable: false },
  { pid: 11, kind: 'C', name: 'pc-eye', used_mib: 514, killable: true, start_time: 1 },
  { pid: 12, kind: 'C', name: 'pc-eye', used_mib: 774, killable: true, start_time: 1 },
  { pid: 13, kind: 'C', name: 'pc-eye', used_mib: 514, killable: true, start_time: 1 },
  { pid: 14, kind: 'C', name: 'mpvpaper', used_mib: 777, killable: true, start_time: 1 },
  { pid: 14, kind: 'G', name: 'mpvpaper', used_mib: 777, killable: true, start_time: 1 },
];
let vramUsed = 7596;
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd, args = {}) => {
    if (cmd === 'daemon_call') {
      if (args.method === 'status' || args.method === 'hello') return args.method === 'status' ? full() : { protocol_version: 1 };
      if (args.method === 'history') return { window_s: 300, points: [] };
      if (args.method === 'capabilities') return goodCaps;
      throw { code: -32601, kind: 'rpc', message: `test: ${args.method}` };
    }
    if (cmd === 'session_call') {
      if (args.method === 'gpu_processes') return { runtime_status: 'active', processes: PROCS };
      throw { code: -1, kind: 'connect', message: 'oturum yok' };
    }
    if (cmd === 'processes') return { processes: [] };
    if (cmd === 'app_info') return {};
    if (cmd === 'sleep_snapshot' || cmd === 'sleep_baseline') return null;
    throw new Error(`test: bilinmeyen komut ${cmd}`);
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

function full() {
  const s = snap();
  s.gpu.vram_used_mib = { value: vramUsed, age_ms: 0 };
  s.gpu.vram_total_mib = { value: 8151, age_ms: 0 };
  return s;
}
const settle = async () => {
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 0));
    await tick();
  }
};

const { app } = await import('../lib/state.svelte.js');
const App = (await import('../App.svelte')).default;
const Display = (await import('../lib/views/Display.svelte')).default;

afterEach(cleanup);

describe('VRAM uyarısı', () => {
  it('%90 üstünde üst bantta uyarı ve süreçlere geçiş', async () => {
    vramUsed = 7596;
    const { container } = render(App);
    await settle();
    Object.assign(app, { connected: true, snap: full(), view: 'overview' });
    await settle();
    const band = [...container.querySelectorAll('.banner')].find((b) => b.textContent.includes('dGPU belleği'));
    expect(band?.textContent).toContain('%93');
    band.querySelector('button').click();
    await settle();
    expect(app.view).toBe('display');
  });

  it('%90 altında uyarı yok', async () => {
    vramUsed = 4000;
    const { container } = render(App);
    await settle();
    Object.assign(app, { connected: true, snap: full(), view: 'overview' });
    await settle();
    expect(container.textContent).not.toContain('dGPU belleği');
  });

  it('Ekran sekmesi süreçleri adına göre toplar ve en büyükten sıralar', async () => {
    Object.assign(app, { connected: true, snap: full(), caps: goodCaps, view: 'display' });
    const { container } = render(Display);
    await settle();
    expect(container.textContent).toContain('pc-eye ×3 · 1,8 GB');
    expect(container.textContent).toContain('mpvpaper · 0,8 GB'); // C+G tek süreç
    const names = [...container.querySelectorAll('tbody tr td:nth-child(2)')].map((td) => td.textContent);
    expect(names.slice(0, 3)).toEqual(['mpvpaper', 'mpvpaper', 'pc-eye']); // tablo C ve G satırlarını ayrı gösterir
    expect(Number(container.querySelector('tbody tr td:nth-child(3)').textContent)).toBe(14);
  });
});
