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
