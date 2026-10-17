'use strict';
// Faz 0.4: WebKitGTK'de Canvas/SVG çizim maliyeti ve kare zamanlaması.
const invoke = window.__TAURI__.core.invoke;
const canvas = document.getElementById('chart');
const ctx = canvas.getContext('2d');
const N = 300;
const series = { cpu: new Float32Array(N), gpu: new Float32Array(N), watt: new Float32Array(N) };
let head = 0, t = 0, animate = true, raf = null, idleTimer = null;

function sample() {
  t += 1;
  series.cpu[head] = 55 + 20 * Math.sin(t / 23) + 4 * Math.sin(t / 3.1);
  series.gpu[head] = 50 + 15 * Math.sin(t / 31 + 1) + 3 * Math.sin(t / 2.3);
  series.watt[head] = 60 + 45 * Math.sin(t / 17) ** 2;
  head = (head + 1) % N;
}
for (let i = 0; i < N; i++) sample();

function resize() {
  const r = canvas.getBoundingClientRect(), d = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.round(r.width * d));
  canvas.height = Math.max(1, Math.round(r.height * d));
  ctx.setTransform(d, 0, 0, d, 0, 0);
}
window.addEventListener('resize', () => { resize(); draw(0); });

function line(arr, lo, hi, color, w, h, offset) {
  ctx.strokeStyle = color; ctx.lineWidth = 1.6; ctx.beginPath();
  for (let i = 0; i < N; i++) {
    const v = arr[(head + i) % N];
    const x = 48 + (i - offset) * (w - 96) / (N - 1);
    const y = 12 + (1 - (v - lo) / (hi - lo)) * (h - 36);
    i ? ctx.lineTo(x, y) : ctx.moveTo(x, y);
  }
  ctx.stroke();
}

function draw(offset) {
  const w = canvas.clientWidth, h = canvas.clientHeight;
  ctx.clearRect(0, 0, w, h);
  ctx.strokeStyle = '#2a2f3a'; ctx.lineWidth = 1; ctx.fillStyle = '#9aa3b2'; ctx.font = '11px system-ui';
  for (let k = 0; k <= 4; k++) {
    const y = 12 + k * (h - 36) / 4;
    ctx.beginPath(); ctx.moveTo(48, y); ctx.lineTo(w - 48, y); ctx.stroke();
    ctx.fillText(`${100 - k * 20}°`, 8, y + 4);
    ctx.fillText(`${150 - k * 30} W`, w - 44, y + 4);
  }
  ctx.save(); ctx.beginPath(); ctx.rect(48, 0, w - 96, h); ctx.clip();
  line(series.cpu, 20, 100, '#ff7a1a', w, h, offset);
  line(series.gpu, 20, 100, '#4fb3ff', w, h, offset);
  line(series.watt, 30, 150, '#7ddc8a', w, h, offset);
  ctx.restore();
}

// Kare zamanlaması: 5 sn'lik pencerelerde FPS, p50/p99 ve > 25 ms kareler.
let frames = [], last = null, windowStart = performance.now(), progress = 0;
function frame(now) {
  if (last !== null) frames.push(now - last);
  last = now;
  progress += 1 / 60;
  if (progress >= 1) { progress -= 1; sample(); }
  draw(progress);
  if (now - windowStart >= 5000) {
    const sorted = frames.slice().sort((a, b) => a - b);
    const pick = q => sorted.length ? sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] : 0;
    const fps = frames.length * 1000 / (now - windowStart);
    const p50 = pick(0.5), p99 = pick(0.99), long = frames.filter(d => d > 25).length;
    document.getElementById('fps').textContent = fps.toFixed(1);
    document.getElementById('p99').textContent = p99.toFixed(1);
    invoke('report', { fps, p50Ms: p50, p99Ms: p99, longFrames: long }).catch(() => {});
    frames = []; windowStart = now;
  }
  raf = requestAnimationFrame(frame);
}

function setAnimate(on) {
  animate = on;
  document.getElementById('toggle').textContent = on ? 'Animasyonu durdur' : 'Animasyonu başlat';
  if (on) {
    clearInterval(idleTimer); idleTimer = null;
    last = null; frames = []; windowStart = performance.now();
    raf = requestAnimationFrame(frame);
  } else {
    cancelAnimationFrame(raf); raf = null;
    // Boşta: gerçek telemetri gibi 1 Hz örnek + tek çizim.
    idleTimer = setInterval(() => { sample(); draw(0); }, 1000);
    draw(0);
  }
}
document.getElementById('toggle').addEventListener('click', () => setAnimate(!animate));

// SVG klavye ızgarası: tıkla ya da sürükleyerek boya.
const svg = document.getElementById('keys');
const NS = 'http://www.w3.org/2000/svg';
let painting = false, hue = 20;
for (let r = 0; r < 6; r++) for (let c = 0; c < 21; c++) {
  const k = document.createElementNS(NS, 'rect');
  k.setAttribute('x', 4 + c * 50); k.setAttribute('y', 4 + r * 49);
  k.setAttribute('width', 46); k.setAttribute('height', 45);
  k.setAttribute('fill', '#232733'); k.dataset.index = r * 21 + c;
  svg.appendChild(k);
}
const paint = e => { if (e.target.tagName === 'rect') e.target.setAttribute('fill', `hsl(${hue} 90% 55%)`); };
svg.addEventListener('pointerdown', e => { painting = true; hue = (hue + 37) % 360; paint(e); });
svg.addEventListener('pointerover', e => { if (painting) paint(e); });
window.addEventListener('pointerup', () => { painting = false; });

invoke('spike_config').then(cfg => {
  document.getElementById('mode').textContent =
    `dmabuf ${cfg.dmabuf_disabled ? 'kapalı' : 'açık'} · compositing ${cfg.compositing_disabled ? 'kapalı' : 'açık'}`;
  resize();
  setAnimate(cfg.animate);
});
