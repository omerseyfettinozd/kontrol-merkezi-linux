# Faz 4 — Arayüz (Tauri v2 + Svelte 5): ara durum

Tarih: 2026-10-18. Kod: `ui/` (Svelte 5 + Vite 8, `ui/src-tauri` ayrı workspace). Derleme: `cd ui && npx tauri build --no-bundle`.

## Yapılanlar

- Rust kabuk (`ui/src-tauri`): r9td için iki kalıcı bağlantı (okuma / denetim; polkit önbelleği bağlantı başına), r9t-session için çağrı başına bağlantı, abonelik iş parçacığı (pencere görünürken `snapshot` olayı), tepsi (renk = güç profili, kırmızı = termal koruma; tooltip CPU/GPU °C; menüde senaryolar + fan otomatik), tek örnek, kapatınca tepside kalma, `__NV_DISABLE_EXPLICIT_SYNC=1`, `--software`, `--hidden`, `--view <modül>`.
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

## Kalan (Faz 4 kabulü)

- [ ] Klavye "Haritayı uygula" tıklaması son yapıda yeniden doğrulanmalı (önceki yapıda işaretçi yakalaması nedeniyle tıklama ulaşmadı; yakalama kaldırıldı, test edilmedi).
- [ ] Boşta CPU ölçümünün gerçek eDP-1 üzerinde tekrarı (headless çıktı yazılımla birleştiriyor olabilir).
- [ ] Kullanıcı başındayken: tepsi ikonu/menü, Hz değişimi, gerçek KDE oturumu, ses tepkili efekt.
- [ ] Kalıcı geçiş (`r9t-switch --commit`, menü başlatıcı, exec-once) — kullanıcı onayıyla.
