# Faz 3a — Kontrol Yolu: Kabul

Tarih: 2026-10-18. Denemeler `r9t-switch --trial` ile; günlük kullanım eski serviste kaldı. Araçlar: `tools/fault-tests.sh`, `tools/lease-test.sh`.

## Komutlar (canlı, geri okumalı)

| Komut | Gözlenen |
|---|---|
| `manual 60 60` | `mode=1`, PWM 120/120, 3750/3717 RPM, lease 12.8 sn'de yenileniyor |
| `fan_preset cool` | 69/62 °C'de duty 84/77 (eğri hesabıyla aynı) |
| `thermal_preset quiet` | `scaling_max_freq` 1800000, boost 0, power-saver, GPU kilidi 1200 MHz; fan auto'da kaldı |
| `scenario game` | Tek atomik komutla 3500 MHz + boost + balanced + EPP balance_performance + GPU 2100 + serin eğri (eski GUI'deki "yalnız ilk komut" hatası yok) |
| `thermal_preset stock` | CPU 5386028 (sürücü tavanı), GPU kilidi sıfırlandı |
| `temperature_target 75/72` | 66/59 °C'de duty 55/50 (formül); `temperature_target_stop` önceki eğriyi ve sınırları geri getirdi |
| `rgb`, `rgb_effect wave`, `rgb_map` | Kırmızı %30 (seviye 15/50); dalga kayıyor; önceki harita geri; `buffer_input=0` |
| `fn_lock` | Çevrildi ve geri alındı, geri okundu |
| `manual 40`, `charge_limit` | `INVALID_PARAMS`, `UNSUPPORTED` |
| polkit | r9t grubundaki kullanıcı (aktif oturum) `org.r9t.control` ile izin aldı |
| `auto` | `mode=0`, `mode_byte & 0x40 == 0` doğrulandı |

## Arıza ve geçiş testleri

| Ölçüt (plan Faz 3) | Sonuç |
|---|---|
| `kill -9` → ExecStopPost ile ≤ 2 sn'de mode=0 | **2.0–2.1 sn** (0.2 sn'lik yoklama dahil); CPU/GPU sınırları kayıtlı varsayılana döndü |
| Yeniden başlatma ≤ 5 sn'de son politika | `systemctl restart`: **4.0 sn**. Çökme sonrası (ExecStopPost + 0.5 sn RestartSec dahil): auto'dan 5.9 sn sonra |
| `kill -STOP` → watchdog, arada fan auto | Watchdog 10 sn'de SIGABRT; **10.6 sn**'de auto; 6.5 sn sonra son politika |
| Test drop-in'i (Restart=no, ExecStopPost boş) + manual %60 + `kill -9` → lease ile ≤ 16 sn'de mode=0, `error=-110` | **≈16 sn, `error=-110`** (yoklama 1.3 sn aralıklı) |
| `--trial` süre dolunca eski servise temiz dönüş | 10 dk sonunda `slayer-r9t` ve gölge aktif, fan auto, CPU kullanıcının ayarına döndü |
| r9td yeniden başlatılınca Waybar ≤ 5 sn'de geri gelir | **2.0 sn** (`r9tctl waybar --follow`) |
| Her test sonrası mode=0 ve bit 0x40 temiz | Evet |
| `systemd-analyze security r9td` | 1.7 OK |

## Bulunan ve düzeltilenler

- **systemd 262:** `OnFailure=` artık yeniden başlatmadan *önce* tetikleniyor; geri dönüş eski servisi başlatıp `Conflicts=` ile r9td'nin yeniden başlatma işini iptal ediyordu. `RestartMode=direct` ile OnFailure yalnız başlatma sınırı (3/60 sn) tükenince çalışıyor.
- `systemctl kill` ile gönderilen sinyal "client request" sayılır: ExecStopPost dahil cgroup'u öldürür ve yeniden başlatma yapılmaz. Çökme testi `kill -9 <MainPID>` ile yapılır.
- ExecStopPost önce sınırları sonra fanı yazıyordu (3.4 sn); sıra **önce fan auto + doğrulama, sonra sınırlar** yapıldı.
- NVML saat kilidi `CAP_SYS_ADMIN` istiyor (setpriv ile ölçüldü); yalnız kontrol unit'ine verildi, gölge unit yetkisiz.
- Açılıştaki tam yoklama (~1.5 sn, EC/RAPL/NVML okuması) hafif yoklamayla değiştirildi; tek okuyucu örnekleyici.
- Başlangıç değerleri (`/var/lib/r9t/baseline.json`) ve son istenen ayarlar (`last.json`) kalıcı; `r9t-switch --trial` her denemede ikisini sıfırlar.

## Yük testleri (16 iş parçacığı tam yük, `tools/load-test.py`, ham veri `evidence/12_*.json`)

| Ölçüt | Sonuç |
|---|---|
| Sıcaklık hedefi yük altında tutar (CPU 75 / GPU 72, `stock` + boost) | 80 °C'de fan %100; tavan 2 sn'de 100 MHz inerek 5286 → 2086 MHz; 100. sn'de 72–73 °C; hedef −3 °C altında 10 sn sonra fan %86'ya indi ve tavan 2686 MHz'e kademeli yükseldi. Durdurunca önceki sınırlar geri geldi |
| Fan eğrisi yükte izlenir (serin preset) | 76 °C'de duty %91 (eğri değeri); 60 örnekte eğriden en büyük sapma %4 (kademeli düşüş bekleme süresi), ortalama %0.1 |
| Oyun açılışında politika ≤ 1 sn | `class:steam_app_*` kuralı: pencere açılışı dahil **0.78 sn**'de 3500 MHz; kapanınca **0.80 sn**'de önceki ayarlar geri |

Yük testinde bulunan hata: yük altında 16 politikadan biri `scaling_max_freq` yazıldıktan hemen sonra birkaç ms eski değeri döndürüyordu; sıcaklık hedefi bunu hata sayıp güvenli şekilde (fan auto) durdu. HAL geri okuması artık 100 ms boyunca tekrar okuyor ve gerekirse bir kez yeniden yazıyor.

## Fiziksel kabulde kalan (kullanıcı başında)

- AC/pil geçişinde politikanın ≤ 1 sn'de uygulanması (fişi çekmek gerekir; mantık birim testli, 3 sn kararlılık süresi plandaki eski davranış).
- Uyku/uyanma (değişmez 7) — `docs/09` olayı nedeniyle yalnız kullanıcı başındayken.
