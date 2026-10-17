# Faz 0.4 — Tauri v2 Denemesi (WebKitGTK + NVIDIA + Hyprland)

Tarih: 2026-10-18. Kod: `spikes/tauri-canvas/` (atılabilir). Ham ölçüm: `spikes/tauri-canvas/measure-result.json`, ölçüm betiği `measure.py`.

## Ortam

- Hyprland 0.56.2, eDP-1 300 Hz (dGPU'ya bağlı), NVIDIA 615.78.08, webkit2gtk-4.1 2.54.1, libsoup 3.8.0, libayatana-appindicator 0.6.0.
- tauri-cli 2.12.3 (proje içine `npm i -D @tauri-apps/cli@2`; global kurulum ve sudo yok). Release derlemesi 1 dk 45 sn, ikili 7.5 MB.
- Güç profili ölçüm boyunca **power-saver**'dı (AC bağlı). Değiştirilmedi.
- Ön yüz: paketleyicisiz HTML/JS. Canvas çift eksenli 300 noktalı grafik, SVG 6×21 boyanabilir ızgara, 5 sn'lik FPS/p99 raporu. Rust: tepsi ikonu + menü, kapatınca tepsiye gizlenme.

## Sonuçlar

| Yapılandırma | Açılış | Boşta CPU (1 Hz çizim) | Animasyon CPU | FPS (animasyon) | NVIDIA'da süreç |
|---|---|---|---|---|---|
| Varsayılan | **Çöküyor** (0.4 sn): `Gdk-Message: Error 71 (protokol hatası) dispatching to Wayland display` | — | — | — | — |
| `__NV_DISABLE_EXPLICIT_SYNC=1` | Çalışıyor | **%0.46** | **%9.7** | 31.1 (p99 33–36 ms) | G: ana süreç 18 MiB, WebKitWebProcess 98 MiB (GPU ile çizim) |
| `WEBKIT_DISABLE_DMABUF_RENDERER=1` | Çalışıyor | %0.8 | %17.5 | 31.2 (ilk 5 sn 66.9) | G: ana süreç 2 MiB (WebKit yazılımla çiziyor) |
| `WEBKIT_DISABLE_COMPOSITING_MODE=1` | Çöküyor (Error 71) | — | — | — | — |
| `GDK_BACKEND=x11` (XWayland) | Pencere açılıyor ama `Failed to create GBM buffer … Geçersiz argüman`, kare raporu gelmedi | — | — | — | — |

- **Bellek:** RSS toplamı 520–570 MB (paylaşılan kütüphaneler 3 süreçte tekrar sayılıyor). Gerçek PSS (explicit sync kapalı, boşta): ana süreç 94 MB + WebKitWebProcess 138 MB + WebKitNetworkProcess 46 MB = **~278 MB**.
- **30 FPS kilidinin nedeni:** kare süreleri tam 32 ms. Sistem power-saver'da ve portal `power-saver-enabled=true` döndürüyor. WebKit düşük güç modunda `requestAnimationFrame`'i bilerek 30 Hz'e kısıyor. Pencere aktif çalışma alanında, görünür ve haritalanmış durumdaydı (`hyprctl clients`). Bu bir hata değil, pil açısından istenen bir davranış. **Balanced/performance profilinde 60+ FPS henüz ölçülmedi.**
- **GPU etkisi:** dGPU ölçüm boyunca zaten P0'daydı (≈2.1 GHz, 25–30 W, ekran dGPU'da). Kontrol deneyi: spike'tan önce 29.9 W / %14.5, spike boştayken 29.6 W / %13.9, sonra 24.7 W / %9.0. Boşta spike'a bağlanabilecek ölçülebilir bir GPU yükü yok; dalgalanma diğer masaüstü yükünden geliyor. (Ayrı gözlem: masaüstü boştayken dGPU'nun P0'da kalması GPU Wake Inspector konusu.)
- **Tepsi:** `org.kde.StatusNotifierWatcher.RegisteredStatusNotifierItems` içinde `…/tray_icon_tray_app_r9t_spike` göründü (Waybar). Pencere `hyprctl dispatch closewindow` ile kapatılınca süreç tepside kaldı. libayatana-appindicator "deprecated" uyarısı veriyor; işlevi etkilemiyor.

## Ek ölçümler (2026-10-18)

- **Görsel kontrol (grim ekran görüntüsü, 1.5 sn arayla iki kare):** grafik, eksen etiketleri, koyu tema ve SVG ızgarası bozulmasız; iki kare arasında grafik kaydı; FPS/p99 rozetleri güncellendi. Siyah kare ya da titreme görülmedi.
- **Balanced profil (geçici olarak açıldı, sonra power-saver'a geri döndü):** 59.5–60.0 FPS, p50 17 ms, p99 18–19 ms, 5 sn'lik pencerelerde uzun kare (> 25 ms) 0–1.
- **KDE KWin (Hyprland içinde iç içe `kwin_wayland --socket r9t-kwin-test`):** varsayılan ayar aynı Error 71 ile çöktü; `__NV_DISABLE_EXPLICIT_SYNC=1` ile çalıştı (power-saver'da 31 FPS). Sorun compositor'dan bağımsız, NVIDIA sürücüsüne özgü.

## Karar: Tauri v2 (plan §6.2 kapandı)

1. Uygulama `main()`'in başında, GTK/EGL yüklenmeden önce `__NV_DISABLE_EXPLICIT_SYNC=1` ayarlar (kullanıcı tanımlamışsa ona dokunmaz). Yedek seçenek: `WEBKIT_DISABLE_DMABUF_RENDERER=1` (ayar + CLI bayrağı; CPU maliyeti yaklaşık iki katı).
2. Grafikler 1 Hz telemetriyle çizilir; sürekli animasyon yoktur (boşta CPU %0.46).
3. power-saver'da WebKit'in 30 FPS kısıtı kabul edilir; balanced'da 60 FPS ölçüldü.

## Fiziksel kabulde kalan

- [ ] Gerçek KDE Plasma oturumunda (iç içe değil) açılış ve tepsi ikonu (Faz 4 kabulünde).
- [ ] Fareyle SVG boyama (Faz 4 RGB modülü kabulünde).
