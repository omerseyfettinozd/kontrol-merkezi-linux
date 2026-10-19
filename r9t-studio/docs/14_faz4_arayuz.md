# Faz 4 — Arayüz (Tauri v2 + Svelte 5): ara durum

Tarih: 2026-10-18. Kod: `ui/` (Svelte 5 + Vite 8, `ui/src-tauri` ayrı workspace). Derleme: `cd ui && npx tauri build --no-bundle`.

## Yapılanlar

- Rust kabuk (`ui/src-tauri`): r9td için iki kalıcı bağlantı (okuma / denetim; polkit önbelleği bağlantı başına), r9t-session için çağrı başına bağlantı, abonelik iş parçacığı (pencere görünürken `snapshot` olayı: odaktayken her örnek, odak dışında 5 sn'de bir, termal alarmda ve alarm değişiminde hemen), tepsi (renk = güç profili, kırmızı = termal koruma; tooltip CPU/GPU °C; menüde senaryolar + fan otomatik), tek örnek, kapatınca tepside kalma, `__NV_DISABLE_EXPLICIT_SYNC=1`, `--software`, `--hidden`, `--view <modül>`.
- Yerel salt okunur tanılama: süreç örnekleyici (/proc, komut satırı okunmaz), s2idle anlık görüntüsü (suspend_stats + hw_sleep, pil enerjisi `voltage_min_design` ile — r9td ile aynı).
- 6 modül: Genel Bakış, Performans & Termal (sürüklenebilir eğri, %50 taban, canlı imleç, PID hedefi), Klavye & RGB (6×21 ızgara, çizgi enterpolasyonlu boyama, global parlaklık ayrı), Ekran/GPU/Otomasyon (Hz, parlaklık, kurallar, Steam listesi, GPU Wake Inspector), Donanım/Ses/Tanılama, Profiller & Kıyaslama (tablo, JSON içe/dışa, 60 sn kararlılık + A/B, CSV).
- Protokol eklemeleri: r9td `policy` (belge + duraklatma + last.json), `rgb_state` (salt okunur LED geri okuma), `rgb_brightness`, `capabilities` içinde preset/senaryo listesi; r9t-session `steam_games`.
- Düzeltme: r9t-session gölge r9td'de `session_register` reddini 5 sn'de bir tekrarlayıp denetim günlüğünü dolduruyordu → READ_ONLY'de soket kapanana (ya da 5 dk) kadar bekler.

## Doğrulama (canlı donanım, headless Hyprland çıktısında; kullanıcı ekranı etkilenmedi)

- Altı modül gerçek veriyle ekran görüntüsünden geçti (gölge ve `r9t-switch --trial 15` denetim modu).
- 1280 px ve 1180 px (pencere 1156 px): yatay taşma yok; dar genişlikte üst şerit iki satıra kırılıyor.
- Etkileşim: fan eğrisi noktası sürüklendi → uygulandı → r9td `last.fan` = `[[45,50],[60,72],[75,80],[85,100]]`, fan görevi CPU %79 / GPU %76 (arayüz imleci tahminiyle aynı). "Otomatik (EC)" → bildirim "mod biti doğrulandı", mode=auto. Klavye ızgarasında tık ve sürükleyerek boyama çalıştı.
- Bulunan/düzeltilen hatalar: aynı PID'in G+C olarak gelmesi (each anahtarı), kaydırıcıların gerçek değer gelmeden orta noktaya yazması, hızlı sürüklemede boşluk, istenmeyen metin seçimi.
- Boşta CPU (60 sn, Genel Bakış, headless çıktı): ilk ölçüm %1,83 → kısma sonrası odak dışı %0,88 (bağlantısız taban %0,37). Etkileşimde 1 Hz, boştayken (odak yok ya da 15 sn girdi yok) 3 sn'de bir tazeleme.

## 2026-10-19 devam

- Altı modül yeniden ekran görüntüsünden geçti: headless `R9TTEST` çıktısı (1280×820, pencere 1256×756, eDP-1'den uzak konumda; kullanıcı ekranı ve odağı değişmedi), gezinme `--view` tek örnek yönlendirmesiyle (sanal girdi yok). r9td gölge, r9t-session çalışıyor (power-watch.sh açık olduğu için Hz'e dokunmuyor). Yatay taşma yok; koyu temada okunamayan metin görülmedi.
- Boşta CPU (60 sn, Genel Bakış, headless çıktı, odak dışı): **%0,88** (WebKit süreçleri dahil). eDP-1 tekrarı hâlâ açık.
- Düzeltme: süreç izleyici ilk açılışta her süreci %0 gösteriyordu (önceki örnek yok) → ilk çağrı 300 ms taban örneği alır; birim testi meşgul iş parçacığıyla CPU > 0 doğrular. Canlı: ilk çizimde gerçek yüzdeler.
- Ekleme: anlık görüntüye `camera` alanı (USB `authorized`, 5 sn'de bir; `r9t_hal::CAMERA_USB` tek kaynak). Eski daemon alanı göndermezse `null` (serde default, test var). Arayüz rozeti açık/kapalı gösterir, mevcut durumla aynı düğme devre dışı. **Kurulu gölge r9td eski sürüm: rozet canlıda "—"; yeni r9td kurulumu sudo bekliyor.** Sysfs değeri: `2b7e:c906 FHD WebCam`, authorized=1.
- pc-eye notu: `pc-eye context` ses örneklemesinde (`pw-record --target @DEFAULT_AUDIO_SINK@`) zaman aşımsız takılıyor ve bu hedef dizgesi mikrofona bağlanıyor; diğer pc-eye komutları ve `grim -o` kullanıldı.

### Deneme modu (2026-10-19 02:00–02:21, `r9t-switch --trial 20`, kullanıcı sudo ile kurdu)

- Yeni r9td (`install-r9td-dev.sh`) denetimde: `camera` alanı canlı `{"value": true}` (sysfs `authorized=1` ile aynı).
- **Klavye "Haritayı uygula" geçti:** headless çıktıda sanal fareyle "Satır 1" (fırça `#e63946`) → "Haritayı uygula". `rgb_state` geri okuması: 1. satır 21 LED `(230,57,70)`, kalan 105 LED `(255,255,255)`, parlaklık 8/50 değişmedi. Denetim günlüğü: `rgb_map ok`, pid = arayüz. Bildirim "Klavye haritası uygulandı.". Sonra `rgb` ile tümü beyaza geri alındı (geri okumayla doğrulandı). Klavyenin fiziksel gözlemi kullanıcı tarafından yapılmadı.
- **Boşta CPU, gerçek eDP-1 (2560×1600@300 Hz), pencere 1280×820 yüzen, görünür, odak dışı, Genel Bakış, 60 sn × 3:** %1,02 / %0,82 / %1,52 (ort. ≈ %1,12; yükün çoğu WebKitWebProcess: 0,63–1,07). **< %1 hedefi tutarlı karşılanmıyor.** Aynı yapı headless 60 Hz'de %0,88. Anlık görüntü ~4 KB/sn; odak dışı yeniden çizim 3 sn'de bir. Sonraki adım: WebKit profili (300 Hz'te 3 sn'lik çizimin maliyeti, kullanıcı etkinliğinin etkisi), gerekirse odak dışında grafikleri dondurma.
- AT-SPI: WebKitGTK penceresi erişilebilirlik ağacı yayınlamıyor (pc-eye `ui` → 0 öğe); otomasyon koordinatla yapıldı, imleç ve odak geri yüklendi.

### Tepsi ve Hz (2026-10-19, gölge r9td + r9t-session, power-watch.sh açık)

- **Hz:** arayüzden 240 → Hyprland `eDP-1 240.0`; 300 → `300.0`, 5 sn sonra hâlâ 300 (power-watch geri çevirmedi). r9t-session `status` doğru değeri döndürüyor.
- **Düzeltme (CSS):** `button:hover:not(:disabled)` `button.active`'den özgül olduğu için etkin düğmenin üzerine gelince etkin görünümü kayboluyordu → `button.active:hover` kuralı. Canlı: imleç "300" üzerindeyken kırmızı çerçeve duruyor.
- **Tepsi:** Tauri `tray-icon` 0.25 Linux'ta `set_tooltip`'i boş bırakıyor (kaynakta no-op) → SNI ToolTip hep boştu. Tepsi `ksni` 0.3 ile yeniden yazıldı (`ui/src-tauri/src/tray.rs`); uygulama artık libayatana-appindicator'a bağlı değil (ldd). D-Bus doğrulaması: ToolTip = "R9T Studio · power-saver" / "CPU 68 °C · GPU 62 °C", Category Hardware, IconPixmap 32×32 ARGB (halka 63,185,80 = güç tasarrufu, merkez beyaz), dbusmenu 9 öğe (göster, 4 senaryo, fanlar otomatik, çık). Waybar'da simge görünüyor. Pencereyi kapatınca süreç tepside kalıyor, `Activate` (sol tık) pencereyi geri getiriyor. Waybar ipucunun gözle kontrolü kullanıcıda.

### Boşta CPU iyileştirmesi (2026-10-19)

Ölçüm: süreç + çocuklarının `/proc/<pid>/stat` utime+stime farkı, 60 sn, Genel Bakış, pencere görünür ve odak dışı. Maliyet ayrıştırması geçici deney anahtarlarıyla (commit'e girmedi), headless `R9TTEST` 1280×820@300 Hz, pencere görünür çalışma alanında, her satır 3×60 sn ortalaması:

| Varyant (odak dışı) | Toplam | WebKitWebProcess |
|---|---|---|
| Eski yapı (1 Hz olay, 3 sn'de bir DOM + grafik) | %0,85 | 0,67 |
| Grafik çizimi kapalı | %0,75 | 0,58 |
| `app.snap` ataması yok (olay gelir, DOM değişmez) | %0,41 | 0,28 |
| Rust olay yollamıyor | %0,11 | 0,04 |

Sonuç: Rust'tan gelen 1 Hz `snapshot` olayının kendisi (JSON → webview `eval` → ayrıştırma) ≈ %0,30, her DOM tazelemesi (tüm kartlar + iki tuval) ≈ %0,13/tazeleme; grafiklerin payı küçük (≈ %0,10). 300 Hz ile 60 Hz arasında fark görülmedi. Dikkat: Hyprland'da adlı çalışma alanı (`name:r9ttest`) headless çıktıda etkin değilse pencere görünmez ve ölçüm düşük çıkar (aynı yapı %0,64); ölçümde pencere etkin çalışma alanında olmalı.

Değişiklik: odak dışında (`WindowEvent::Focused`) Rust abonelik iş parçacığı örnekleri 5 sn'de bir yollar; termal alarm (koruma ya da kernel override) etkinken her örnek ve alarm değişimi hemen gider. Odak gelince webview r9td'nin 1 Hz `history`'siyle aradaki boşlukları doldurur (`prefillHistory` artık iç boşlukları da kapatıyor; önceden yalnız eski noktaları başa ekliyordu). Grafik çizgisi 6,5 sn'den uzun boşlukta kesilir (önce 5 sn; seyrek örnekler kesik çizilmesin diye). Odaktayken davranış aynı (etkileşimde 1 Hz, 15 sn girdi yoksa 3 sn).

**eDP-1 (2560×1600@300 Hz), pencere 1280×820 yüzen, görünür, odak dışı, Genel Bakış, 60 sn:** eski yapı %0,82 / %0,82 / %0,82 → yeni **%0,45 / %0,45 / %0,48** (WebKitWebProcess 0,33–0,35, ana süreç 0,12–0,13). Headless 300 Hz yeni yapı %0,57 / %0,45 / %0,50. Doğrulama: odak dışında üst şerit değerleri ~5 sn'de bir, odaktayken her saniye değişti (ekran görüntüsü); odak sonrası grafikler 5 dk boyunca 1 Hz yoğunlukta dolu. Termal alarm yolu canlı tetiklenmedi (kod yolu: alarm etkinken her örnek yollanır). Önceki eDP-1 ölçümündeki %1,12 (deneme modu) bu kez eski yapıyla da tekrarlanmadı (%0,82); fark muhtemelen o sıradaki ekran etkinliği.

## Kalan (Faz 4 kabulü)

- [x] Klavye "Haritayı uygula" tıklaması son yapıda doğrulandı (2026-10-19, LED geri okuması).
- [x] Boşta CPU < %1 gerçek eDP-1'de (2026-10-19): 300 Hz, odak dışı, 3×60 sn %0,45 / %0,45 / %0,48 (odak dışında olay seyreltme).
- [x] Tepsi ikonu/menü/ipucu (D-Bus) ve Hz değişimi (2026-10-19).
- [ ] Kullanıcı başındayken: Waybar ipucunun gözle kontrolü, gerçek KDE oturumu, ses tepkili efekt (denetim modu gerekir).
- [ ] Kalıcı geçiş (`r9t-switch --commit`, menü başlatıcı, exec-once) — kullanıcı onayıyla.
