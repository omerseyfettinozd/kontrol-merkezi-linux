// jsdom'da olmayan tarayıcı API'leri (WebKitGTK'de var).
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};

// jsdom canvas çizmez; 2D bağlamı yöntemleri işlevsiz, ölçüm 0 genişlik döner.
const ctx2d = new Proxy(
  {},
  {
    get: (t, k) => (k in t ? t[k] : k === 'measureText' ? () => ({ width: 0 }) : () => {}),
    set: (t, k, v) => ((t[k] = v), true),
  },
);
HTMLCanvasElement.prototype.getContext = function () {
  return ctx2d;
};
