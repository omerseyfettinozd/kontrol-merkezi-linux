// Duman testleri: her görünüm bağlantı yokken, sensörler null iken ve probe okuması
// başarısızken render olmalı; null değer asla "0 °C" diye gösterilmemeli.
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import { tick } from 'svelte';
import { snap, unavailableCaps, goodCaps } from './fixtures.js';

const calls = [];
let daemonOk = true;
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd, args = {}) => {
    calls.push([cmd, args]);
    if (cmd === 'daemon_call') {
      if (!daemonOk) throw { code: -1, kind: 'connect', message: 'bağlanılamadı' };
      if (args.method === 'status') return snap();
      if (args.method === 'history') return { window_s: 300, points: [] };
      if (args.method === 'events') return { events: [] };
      throw { code: -32601, kind: 'rpc', message: `test: ${args.method}` };
    }
    if (cmd === 'session_call') throw { code: -1, kind: 'connect', message: 'oturum yok' };
    if (cmd === 'processes') return { processes: [] };
    if (cmd === 'app_info') return {};
    throw new Error(`test: bilinmeyen komut ${cmd}`);
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

const { app } = await import('../lib/state.svelte.js');
const VIEWS = {
  Overview: (await import('../lib/views/Overview.svelte')).default,
  Performance: (await import('../lib/views/Performance.svelte')).default,
  Keyboard: (await import('../lib/views/Keyboard.svelte')).default,
  Display: (await import('../lib/views/Display.svelte')).default,
  Hardware: (await import('../lib/views/Hardware.svelte')).default,
  Profiles: (await import('../lib/views/Profiles.svelte')).default,
};

const STATES = {
  'bağlantı yok': () => Object.assign(app, { connected: false, snap: null, caps: null, policy: null, session: null }),
  'sensörler null': () => Object.assign(app, { connected: true, snap: snap({ empty: true }), caps: goodCaps, policy: null }),
  'probe okunamadı': () => Object.assign(app, { connected: true, snap: snap(), caps: unavailableCaps, policy: null }),
  'tam veri': () => Object.assign(app, { connected: true, snap: snap(), caps: goodCaps, policy: null }),
};

beforeEach(() => {
  calls.length = 0;
  daemonOk = true;
});
afterEach(() => cleanup());

describe.each(Object.entries(VIEWS))('%s', (name, View) => {
  it.each(Object.keys(STATES))('%s durumunda render olur', async (state) => {
    STATES[state]();
    const { container } = render(View);
    await tick();
    await new Promise((r) => setTimeout(r, 20));
    expect(container.textContent.length).toBeGreaterThan(0);
    if (state === 'sensörler null' || state === 'bağlantı yok') {
      // Değişmez 8: geçersiz sensör asla 0 °C görünmez.
      expect(container.textContent).not.toMatch(/(^|[^\d.])0(\.0)?\s*°C/);
    }
  });
});

describe('App', () => {
  it('daemon kapalıyken açılır ve hata bandı gösterir', async () => {
    daemonOk = false;
    Object.assign(app, { connected: false, snap: null, caps: null, daemonError: null });
    const App = (await import('../App.svelte')).default;
    const { container } = render(App);
    await new Promise((r) => setTimeout(r, 30));
    expect(container.querySelector('nav')).not.toBeNull();
    expect(app.daemonError).toBeTruthy();
  });
});
