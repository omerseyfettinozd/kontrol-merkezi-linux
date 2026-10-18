// Ortak durum: r9td anlık görüntüsü, geçmiş halka tamponu, yetenekler, politika ve
// r9t-session durumu. Tüm çağrılar Rust tarafındaki komutlardan geçer.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// Sınıf alanları: `snap` ham tutulur (saniyede bir ~30 KB nesneyi derin vekile
// çevirmemek için); yeniden atama yine de tüm okuyucuları günceller.
class AppState {
  snap = $state.raw(null);
  caps = $state.raw(null);
  policy = $state.raw(null);
  session = $state.raw(null);
  hello = $state.raw(null);
  connected = $state(false);
  daemonError = $state(null);
  sessionError = $state(null);
  toasts = $state([]);
  tick = $state(0);
  view = $state('overview');
  pending = $state({});
}

export const app = new AppState();

/** 1 Hz halka tamponu (en çok 300 nokta). Dizi reaktif değil; `app.tick` değişimi bildirir. */
export const history = { points: [] };
const HISTORY_MAX = 300;

export const val = (f) => (f && f.value !== undefined ? f.value : null);
export const ageMs = (f) => (f ? f.age_ms : null);
export const isFresh = (f, maxMs = 10000) => !!f && f.value != null && f.age_ms != null && f.age_ms <= maxMs;

export function fmt(x, digits = 0, unit = '') {
  if (x == null || typeof x !== 'number' || !Number.isFinite(x)) return '—';
  const s = x.toLocaleString('tr-TR', { minimumFractionDigits: digits, maximumFractionDigits: digits });
  return unit ? `${s} ${unit}` : s;
}

export const POWER_NAMES = { 'power-saver': 'Güç tasarrufu', balanced: 'Dengeli', performance: 'Performans' };
export const EPP_NAMES = {
  performance: 'Performans',
  balance_performance: 'Dengeli performans',
  balance_power: 'Dengeli tasarruf',
  power: 'Tasarruf',
};
export const SCENARIO_NAMES = { quiet: 'Sessiz', balanced: 'Dengeli', game: 'Oyun', turbo: 'Turbo' };

const ERROR_TEXT = {
  '-32006': 'r9td salt okunur (gölge) modda; değişiklik yapılamaz. Denetim için r9td etkin olmalı.',
  '-32001': 'Yetki verilmedi (r9t grubu ya da polkit onayı gerekli).',
  '-32002': 'Bu donanımda desteklenmiyor.',
  '-32003': 'Donanım meşgul ya da uyku sürüyor; biraz sonra yeniden deneyin.',
};

export function errorText(e) {
  if (!e) return 'Bilinmeyen hata';
  if (typeof e === 'string') return e;
  if (e.kind === 'connect') return `Servise bağlanılamadı: ${e.message}`;
  const base = ERROR_TEXT[String(e.code)];
  return base && e.message ? `${base} (${e.message})` : base || e.message || JSON.stringify(e);
}

let toastSeq = 1;
export function toast(text, kind = 'ok', ms = 4500) {
  const id = toastSeq++;
  app.toasts.push({ id, text, kind });
  setTimeout(() => {
    const i = app.toasts.findIndex((t) => t.id === id);
    if (i >= 0) app.toasts.splice(i, 1);
  }, ms);
}

/** Salt okunur r9td çağrısı (hata fırlatır, bildirim göstermez). */
export function read(method, params = {}) {
  return invoke('daemon_call', { method, params });
}

/**
 * Denetim çağrısı: bekleme durumu, sonuç bildirimi. `key` aynı düğmenin tekrar
 * basılmasını engeller. Başarıda r9td'nin `message` alanı gösterilir.
 */
export async function control(method, params = {}, { key = method, quiet = false } = {}) {
  if (app.pending[key]) return null;
  app.pending[key] = true;
  try {
    const r = await invoke('daemon_call', { method, params });
    if (!quiet) toast(r?.message || 'Uygulandı.', r?.verified === false ? 'warn' : 'ok');
    if (['configure', 'automation_pause', 'automation_resume', 'profile', 'apply_profile', 'curve', 'rgb', 'rgb_map', 'rgb_brightness'].includes(method)) {
      refreshPolicy();
    }
    return r ?? {};
  } catch (e) {
    toast(errorText(e), 'bad', 7000);
    return null;
  } finally {
    delete app.pending[key];
  }
}

export async function session(method, params = {}, { notify = true } = {}) {
  try {
    const r = await invoke('session_call', { method, params });
    app.sessionError = null;
    return r;
  } catch (e) {
    if (notify) toast(e?.kind === 'connect' ? 'r9t-session çalışmıyor (systemctl --user start r9t-session).' : errorText(e), 'bad', 7000);
    if (e?.kind === 'connect') app.sessionError = 'r9t-session çalışmıyor';
    throw e;
  }
}

export async function refreshSession() {
  try {
    app.session = await invoke('session_call', { method: 'status', params: {} });
    app.sessionError = null;
  } catch (e) {
    app.session = null;
    app.sessionError = e?.kind === 'connect' ? 'r9t-session çalışmıyor' : errorText(e);
  }
}

export async function refreshPolicy() {
  try {
    app.policy = await read('policy');
  } catch {
    app.policy = null;
  }
}

/** Yetenek: `available` + gerekçe. Bilinmeyen yetenek etkin sayılmaz. */
export function cap(id) {
  const c = app.caps?.probe?.capabilities?.find((x) => x.id === id);
  return c ? { ok: !!c.available, reason: c.reason || '' } : { ok: false, reason: 'yetenek bilgisi yok' };
}

export const isShadow = () => app.snap?.authority?.state === 'shadow' || app.caps?.mode === 'shadow';
export const canControl = () => app.connected && !isShadow();

function pointFromSnap(s) {
  return {
    t: s.taken_at_ms,
    cpu_c: val(s.cpu.ec_temp_c),
    gpu_c: val(s.gpu.ec_temp_c),
    cpu_w: val(s.cpu.package_w),
    gpu_w: val(s.gpu.power_w),
    cpu_rpm: val(s.fans.cpu_rpm),
    gpu_rpm: val(s.fans.gpu_rpm),
    battery_w: val(s.power.battery.power_w),
  };
}

async function prefillHistory() {
  try {
    const h = await read('history', { seconds: 300 });
    const pts = (h.points || []).map((p) => ({ ...p, t: p.t_ms }));
    const newest = history.points.length ? history.points[0].t : Infinity;
    history.points = [...pts.filter((p) => p.t < newest), ...history.points].slice(-HISTORY_MAX);
    app.tick++;
  } catch {
    /* geçmiş yoksa canlı akış doldurur */
  }
}

async function onConnected() {
  try {
    app.caps = await read('capabilities');
  } catch {
    app.caps = null;
  }
  refreshPolicy();
  prefillHistory();
}

export async function start() {
  try {
    const info = await invoke('app_info');
    if (info.view) app.view = info.view;
  } catch {
    /* tarayıcıda geliştirme */
  }
  await listen('navigate', (e) => (app.view = e.payload));
  // Geçmiş her örneği alır; ekran kullanıcı etkileşimdeyken 1 Hz, boştayken (odak
  // dışı ya da 15 sn girdi yok) 3 sn'de bir tazelenir (boşta CPU < %1 hedefi).
  // Termal alarm her zaman anında gösterilir.
  let lastPaint = 0;
  let lastInput = Date.now();
  for (const ev of ['pointermove', 'pointerdown', 'keydown', 'wheel']) {
    window.addEventListener(ev, () => (lastInput = Date.now()), { passive: true, capture: true });
  }
  await listen('snapshot', (e) => {
    const s = e.payload;
    const p = pointFromSnap(s);
    const last = history.points[history.points.length - 1];
    if (!last || p.t > last.t) {
      history.points.push(p);
      if (history.points.length > HISTORY_MAX) history.points.splice(0, history.points.length - HISTORY_MAX);
    }
    const now = Date.now();
    const alarm = s.authority?.guard_active || s.fans?.thermal_override?.value;
    const active = document.hasFocus() && now - lastInput < 15000;
    if (active || alarm || now - lastPaint >= 2900 || !app.snap) {
      lastPaint = now;
      app.snap = s;
      app.tick++;
    }
  });
  window.addEventListener('focus', () => {
    lastInput = Date.now();
    app.tick++;
  });
  await listen('daemon', (e) => {
    const was = app.connected;
    app.connected = !!e.payload.connected;
    app.daemonError = e.payload.error ? errorText(e.payload.error) : null;
    if (app.connected) {
      app.hello = e.payload.hello;
      if (!was) onConnected();
    }
  });
  await listen('tray-result', (e) => {
    const r = e.payload;
    toast(r.ok ? `Tepsi: ${r.method} uygulandı.` : errorText(r.error), r.ok ? 'ok' : 'bad');
  });
  await listen('shown', () => {
    // Gizliyken anlık görüntü gönderilmez: boşluğu geçmişten doldur.
    if (app.connected) prefillHistory();
    refreshSession();
  });
  // Abonelik zaten bağlıysa ilk "daemon" olayı kaçmış olabilir.
  try {
    app.hello = await read('hello', { protocol_version: 1, client: 'r9t-studio-ui' });
    app.connected = true;
    app.snap = await read('status');
    onConnected();
  } catch (e) {
    app.daemonError = errorText(e);
  }
  refreshSession();
}

/** Bileşen görünürken periyodik iş; `document.hidden` iken çalışmaz. */
export function every(ms, fn) {
  fn();
  const id = setInterval(() => {
    if (!document.hidden) fn();
  }, ms);
  return () => clearInterval(id);
}
