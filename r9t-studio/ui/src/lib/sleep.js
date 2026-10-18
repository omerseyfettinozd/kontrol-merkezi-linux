// Eski sleepdiag.compare portu + AMD donanım uykusu (s0i3) oranı. Uyku başlatmaz.
export const WARNING =
  'Bu araç uykuyu başlatmaz ve fiziksel uyku testi yapılmış sayılmaz. Şarj/priz durumu değişirse sonuç geçersizdir.';

export function compareSleep(before, after) {
  const res = { duration_s: null, suspend_delta: null, hw_ratio: null, loss_w: null, loss_pct_h: null, valid: true, warnings: [WARNING] };
  const bad = (m) => {
    res.valid = false;
    res.warnings.push(m);
  };
  const dur = after.time - before.time;
  if (!(dur > 0)) bad('Saat geriye gitti ya da süre sıfır; sonuç geçersiz.');
  else res.duration_s = dur;
  if (before.ac_online == null || after.ac_online == null) bad('Priz durumu okunamadı; geçerlilik doğrulanamadı.');
  else if (before.ac_online || after.ac_online) bad('Priz bağlıydı ya da değişti; sonuç geçersiz.');
  if ([before, after].some((s) => ['Charging', 'Full'].includes(s.battery?.status))) bad('Pil şarj oluyordu; sonuç geçersiz.');
  const e0 = before.battery?.energy_uwh, e1 = after.battery?.energy_uwh;
  if (e0 != null && e1 != null && res.duration_s) {
    res.loss_w = (e0 - e1) / 1e6 / (res.duration_s / 3600);
    const full = before.battery?.energy_uwh && before.battery?.capacity ? (before.battery.energy_uwh / before.battery.capacity) * 100 : null;
    if (full) res.loss_pct_h = ((e0 - e1) / full) * 100 / (res.duration_s / 3600);
  } else bad('Pil enerjisi okunamadı.');
  const s0 = before.stats ?? {}, s1 = after.stats ?? {};
  if (s0.success != null && s1.success != null) {
    res.suspend_delta = s1.success - s0.success;
    if (res.suspend_delta <= 0) bad('Uyku sayacı artmadı; uyku doğrulanamadı.');
  } else bad('Uyku sayacı okunamadı; uyku doğrulanamadı.');
  if (s0.total_hw_sleep_us != null && s1.total_hw_sleep_us != null && res.duration_s) {
    res.hw_ratio = (s1.total_hw_sleep_us - s0.total_hw_sleep_us) / 1e6 / res.duration_s;
    if (res.suspend_delta > 0 && res.hw_ratio < 0.5) res.warnings.push('Donanım uykusu oranı düşük: sistem s0i3 yerine sığ uykuda kalmış olabilir.');
  }
  if (s0.fail != null && s1.fail != null && s1.fail > s0.fail) res.warnings.push(`${s1.fail - s0.fail} uyku denemesi başarısız oldu.`);
  return res;
}
