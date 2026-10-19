// r9t-protocol FanCurve::validate / duty_at kopyası (crates/r9t-protocol/src/profile.rs).
// Sunucu yine doğrular; bu yalnız düzenlerken anında geri bildirim içindir.
// Protokolün en düşük değeri (ölçülmüş güvenli taban, docs/07). r9td her komutu
// yüklü modülün tabanına (min_duty; 0.2.0'da 50) yükseltir.
export const DUTY_FLOOR = 25;
export const T_MIN = 40;
export const T_MAX = 95;

export function validate(points) {
  if (points.length < 2 || points.length > 10) return 'Eğri 2–10 nokta içermeli.';
  let prev = null;
  for (const [t, d] of points) {
    if (t < T_MIN || t > T_MAX || d < DUTY_FLOOR || d > 100) return 'Eğride sıcaklık 40–95 °C, fan %25–100 olmalı.';
    if (prev && (t <= prev[0] || d < prev[1])) return 'Sıcaklıklar artmalı, fan yüzdesi azalmamalı.';
    prev = [t, d];
  }
  const [tl, dl] = points[points.length - 1];
  if (dl !== 100 || tl > 90) return "Eğri en geç 90 °C'de %100 fanla bitmeli.";
  return null;
}

function segment(p, t) {
  for (let i = 0; i < p.length - 1; i++) if (t <= p[i + 1][0]) return i;
  return p.length - 2;
}

function linear(p, t) {
  const i = segment(p, t);
  const [t0, d0] = p[i], [t1, d1] = p[i + 1];
  return d0 + ((d1 - d0) * (t - t0)) / (t1 - t0);
}

function monotoneCubic(p, temp) {
  const n = p.length;
  const x = p.map((q) => q[0]), y = p.map((q) => q[1]);
  const delta = [];
  for (let i = 0; i < n - 1; i++) delta.push((y[i + 1] - y[i]) / (x[i + 1] - x[i]));
  const m = new Array(n).fill(0);
  m[0] = delta[0];
  m[n - 1] = delta[n - 2];
  for (let i = 1; i < n - 1; i++) m[i] = delta[i - 1] * delta[i] <= 0 ? 0 : (delta[i - 1] + delta[i]) / 2;
  for (let i = 0; i < n - 1; i++) {
    if (delta[i] === 0) {
      m[i] = 0;
      m[i + 1] = 0;
      continue;
    }
    const a = m[i] / delta[i], b = m[i + 1] / delta[i];
    const s = a * a + b * b;
    if (s > 9) {
      const tau = 3 / Math.sqrt(s);
      m[i] = tau * a * delta[i];
      m[i + 1] = tau * b * delta[i];
    }
  }
  const i = segment(p, temp);
  const h = x[i + 1] - x[i];
  const t = (temp - x[i]) / h;
  const t2 = t * t, t3 = t2 * t;
  return (2 * t3 - 3 * t2 + 1) * y[i] + (t3 - 2 * t2 + t) * h * m[i] + (-2 * t3 + 3 * t2) * y[i + 1] + (t3 - t2) * h * m[i + 1];
}

/** Valid eğri için verilen sıcaklıktaki fan yüzdesi. */
export function dutyAt(points, interpolation, temp) {
  const first = points[0], last = points[points.length - 1];
  if (temp <= first[0]) return first[1];
  if (temp >= last[0]) return 100;
  const d = interpolation === 'monotone_cubic' ? monotoneCubic(points, temp) : linear(points, temp);
  return Math.min(100, Math.max(DUTY_FLOOR, Math.round(d)));
}
