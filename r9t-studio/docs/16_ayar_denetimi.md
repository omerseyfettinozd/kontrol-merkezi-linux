# 16 — CPU/GPU ayar denetimi ve tavan taraması (2026-10-21)

Kurulu r9td (0.1.0, `b37fbe3` paketi) üzerinde, prizde, fan eğrisi sabitken. Ölçümler sentetiktir; oyun kabulü değildir.

## Bulunan hatalar

| # | Belirti | Kanıt | Düzeltme |
|---|---|---|---|
| 1 | Açılışta hiçbir profil uygulanmıyor: her çağrı `NVML init: hata 999; önceki ayarlar geri yüklendi` | 19:23 ve 20:50 açılışları; 20:59'da `systemctl restart r9td` sonrası düzeldi. Aynı anda kullanıcı süreçlerinde NVML çalışıyordu | Erken açılışta süreç içi `nvmlInit` hatası süreç ömrü boyunca kalıyor. İlk init artık `r9td --nvml-probe` alt sürecinde (2 sn sınır, 10 sn'de bir) başarılı olunca yapılıyor (`r9t_hal::gpu` preflight) |
| 2 | Açılıştaki geri yüklemede GPU hatası CPU/güç profilini de geri alıyordu, tekrar deneme yoktu | 19:23 `restore` olayı | GPU tavanı ayrı uygulanıyor, 10 sn arayla 30 kez deneniyor |
| 3 | "Sınırları kaldır" sonrası r9td, nvidia-powerd'nin `scaling_max_freq` düşürmesini 10 sn'de bir geri alıyordu | Kod incelemesi | Tavan 0 iken yabancı yazma denetimi kapalı |
| 4 | Sıcaklık hedefi durunca CPU 10 sn içinde hedefin son düşük tavanına geri kilitleniyordu | Kod incelemesi (`expected_cpu_max` güncellenmiyordu) | Hedef öncesi değer saklanıp geri konuyor |
| 5 | Presetler EPP yazmıyordu; ppd profili değişmezse önceki EPP kalıyordu | Yeniden başlatma sonrası `quiet`: ppd power-saver, EPP balance_performance | Her preset EPP'yi açıkça yazıyor (power-saver → power, diğerleri → balance_performance) |

**Düzeltme, 2026-10-22:** 1 numaranın gerçek nedeni süreç değil cgroup'tu. `r9td.service` `DeviceAllow=/dev/nvidia0` yolunu birim başlarken bir kez çözüyor; açılışta r9td 22:33:38.0607'de başladı, `/dev/nvidia0` 38.0679'da oluştu (7 ms sonra). Kural düştü, `DevicePolicy=closed` NVIDIA aygıtlarını servis ömrü boyunca kapattı; `--nvml-probe` aynı cgroup'ta olduğu için preflight hiç geçmedi ve GPU tavanlı her mod `NVML henüz hazır değil; önceki ayarlar geri yüklendi` verdi (kullanıcı olarak probe exit 0). `restart` düğüm artık var olduğu için düzeltiyordu. Birimler artık sınıfla izin veriyor: `DeviceAllow=char-nvidia rw`, `char-tuxedo_io_cdev` (majörler `systemd-modules-load` ile kayıtlı, r9td ondan sonra başlıyor). Doğrulama: `systemd-run -p DevicePolicy=closed -p "DeviceAllow=char-nvidia rw" r9td --nvml-probe` → success; r88 kurulup r9td yeniden başlatılınca geri yükleme ve GPU tavanı (1200 MHz) ilk denemede uygulandı. Soğuk açılış kabulü bekliyor.

Ayrıca: 19:56'da r9td 10 sn yanıt vermedi ve watchdog SIGABRT ile kapattı (yığın sembolsüz, neden belirsiz). Kapanışta r9td ↔ power-profiles-daemon systemd sıralama döngüsü (`ileride.md`).

## Preset geri okuması (düzeltmeden önce, r9td yeniden başlatıldıktan sonra)

CPU tarafı (ppd, boost, `scaling_max_freq`) her presette doğru. GPU tavanı yük altında doğrulandı: 2100 istendi, 2077–2085 MHz.

## GPU tavanı (Blender Cycles OptiX, 1920×1080, 256 örnek, 90 sn)

| Tavan | Ort. saat | Güç | GPU °C | s/kare | Hız |
|---|---|---|---|---|---|
| yok | 2661 | 90,0 W | 83 | 3,255 | %100 |
| 2700 | 2597 | 83,2 W | 83 | 3,441 | %95 |
| 2475 | 2413 | 70,6 W | 79 | 3,552 | %92 |
| 2250 | 2218 | 59,4 W | 75 | 3,820 | %85 |
| 2100 | 2085 | 54,6 W | 73 | 4,007 | %81 |
| 1800 | 1789 | 47,6 W | 70 | 4,652 | %70 |

## CPU tavanı

Tüm çekirdek (7-Zip, 16 iş parçacığı): işlemci güç sınırında ~3 GHz'de kalıyor; 3000–5386 tavanları arasında puan farkı yok (gürültü içinde).

Oyun benzeri yük (6 iş parçacığı, tamsayı döngüsü, 70 sn):

| Tavan | İş (M döngü/sn) | Hız | EC °C |
|---|---|---|---|
| yok | 6195 | %100 | 73 |
| 4900 | 6159 | %99 | 74 |
| 4500 | 6156 | %99 | 74 |
| 4000 | 5769 | %93 | 71–76 |
| 3500 | 5038 | %81 | 69 |

Paket gücü örnekleri arka plan yüküyle gürültülü; güç farkı bu testten çıkarılmadı. Sıcaklıklar sıralı ölçümde ısı birikiminden etkilenir.

## Karar

- `sweet_spot` (4900/2475) değişmedi: GPU −%8 hız, −%22 güç, −4 °C.
- `gaming_cool` 3500/2100 → **4000/2250**: eski değerler CPU ve GPU'da −%19 hız kaybettiriyordu.
- Kabul için bekleyen: aynı oyunda FPS + sıcaklık (CS2 uzun oturum).

## Hafif yük: Sessiz / Kütüphane (kullanıcının gerçek masaüstü yükü, 3–5 dk)

Sessiz frekansları (power-saver, boost kapalı, 1800/1200, EPP power). Arka planda ~1,5 çekirdek süren uygulamalar vardı.

| Durum | CPU °C | GPU °C | Fan | CPU W | GPU W |
|---|---|---|---|---|---|
| 300 Hz + duvar kağıdı, EC auto | 69 | 63 | %55 · ~3500 RPM | 24,0 | 20,7 (P0) |
| 60 Hz + duvar kağıdı, EC auto | 67 | 59 | %46 · ~3000 | 19,9 | 13,9 |
| 60 Hz, duvar kağıdı donmuş (süreç duruk) | 65 | 58 | %42 · ~2770 | 16,4 | 12,1 (P0) |
| 60 Hz + duvar kağıdı, sessiz eğri | 70 | 63 | %40 · ~2500 | 21,8 | 14,2 |
| 60 Hz, duvar kağıdı kapalı, sessiz eğri | 67 | 59 | %36 · ~2300 | 17,6 | 6,9 (P8) |

- mpvpaper `hwdec=nvdec` CUDA bağlamı açık tuttuğu sürece dGPU P0'da ve bellek 12001 MHz'de kalıyor; süreç donsa bile. Kapanınca 10 sn içinde P8, 405 MHz bellek, 8,5 W. `hwdec=vulkan` P4 / ~15 W, kazanç yok.
- Sessiz eğrinin altında fan payını r9t_fan tabanı belirliyor (CPU 60→75 °C arasında %25→50). Fanı kısmak için ısı kaynağını azaltmak gerekiyor.

## Oyun benzeri karma yük (6 iş parçacığı CPU + Blender GPU, 150 sn, 60 Hz)

| Mod | GPU s/kare | GPU hızı | CPU iş | CPU hızı | CPU °C | GPU °C | GPU W | Fan |
|---|---|---|---|---|---|---|---|---|
| Sınırları kaldır (`balanced` senaryosu) | 2,952 | %100 | 4128 | %100 | 81 | 83 | 89,0 | 5793 |
| Turbo | 2,948 | %100 | 4051 | %98 | 80 | 82 | 88,6 | 5803 |
| Sweet Spot | 3,103 | %95 | 4996 | %121 | 81 | 77 | 70,7 | 5561 |
| Serin Oyun (4000/2250, serin eğri) | 3,278 | %90 | 5441 | %132 | 81 | 73 | 60,4 | 5557 |

- Turbo, Sınırları kaldır'dan hızlı değil; ikisinde fanlar GPU 84 °C korumasıyla %100'de.
- CPU ve GPU aynı güç/ısı bütçesini paylaşıyor: GPU tavanı CPU'ya pay bırakıyor. CPU ağırlıklı oyunlarda (CS2) Serin Oyun daha yüksek FPS verebilir; doğrulama oyunda yapılmalı.

## Eklenenler

- `library` (Kütüphane): Sessiz frekansları + `quiet` fan eğrisi `[[60,25],[70,30],[78,50],[85,100]]`.
- `quiet` artık fanı açıkça EC otomatiğine alıyor (önceden önceki modun eğrisinde kalıyordu).

## Oyun: Mount & Blade II Bannerlord, savaş benchmark'ı (384 asker), 2026-10-21

Serin Oyun GPU tavanı (2250), fanlar EC otomatik (koruma CPU 90 / GPU 84 °C'de %100'e aldı). Asker dağılımı her koşuda biraz farklı (±%3–4).

| Koşu | Ort. FPS | En düşük %1 (GPU) | CPU en yüksek | GPU en yüksek |
|---|---|---|---|---|
| boost açık, CPU 4000, ppd performance | 66,2 | 44,9 | 91 °C | 87 °C |
| boost açık, CPU 4000, ppd balanced | 70,9 | 47,1 | 90 °C | 87 °C |
| boost kapalı (2400), ppd balanced | 75,0 | 60,6 | 81 °C | 84 °C |

- ppd performance (EPP performance) daha sıcak ve daha yavaş.
- Bu sahnede darboğaz GPU (%99): CPU'yu serinletmek paylaşılan güç ve soğutmayı GPU'ya bırakıyor. `gaming_cool` boost kapalı + `cool` eğri oldu; CPU ağırlıklı oyunlar için `sweet_spot` boost açık kalıyor.
