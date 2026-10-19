# Faz 0.7 — Fan Duty → RPM Eğrisi

Tarih: 2026-10-18. Ham veri: `07_fan_rpm_egrisi.json`. Araç: `tools/fan-rpm-sweep.py` (eski servisin `manual` RPC'si, kullanıcı olarak). Modül: r9t_fan 0.2.0. AC bağlı, boşta.

| Duty | CPU fan RPM (medyan) | GPU fan RPM (medyan) | Durma eşiği (%50, ≥ 500) |
|---|---|---|---|
| %50 | 3180 | 3232 | 1590 / 1616 |
| %60 | 3717 | 3756 | 1858 / 1878 |
| %70 | 4269 | 4269 | 2134 / 2134 |
| %80 | 4759 | 4718 | 2380 / 2359 |
| %90 | 5246 | 5170 | 2623 / 2585 |
| %100 | 5765 | 5615 | 2882 / 2808 |

- Her adım 24 sn, ilk 8 sn hızlanma için atıldı, adım başına ≥ 3 örnek. Durdurma koşulu tetiklenmedi. Sonunda `auto` gönderildi ve `mode=0`, `mode_byte=160` (bit 0x40 temiz) doğrulandı. Otomasyon zaten duraklatılmıştı, öyle bırakıldı.
- Auto modda boşta duty ~%53 (PWM 106/200), RPM ~3200–3390.
- **Plandan sapma:** ön koşul "CPU/GPU EC < 60 °C" yerine "CPU < 75, GPU < 70 °C" kullanıldı. Makine boşta 66–68 °C'de duruyor; tarama yalnız %50–100 yazdığı için auto'dan daha az soğutmaz. Durdurma eşikleri (80/75 °C) korundu. Tarama sırasında sıcaklık 69/64'ten 63/57 °C'ye düştü.
- %50 altı bu fazda ölçülemez (modül reddeder); Faz 5 v2'ye kalır.
- Fiziksel dinleme (ses) yapılmadı; kabul RPM ölçümüyle.

## Faz 5 — %50 altı (r9t_fan 0.3.0, 2026-10-19 21:2x)

Ham veri: `07_fan_rpm_dusuk_duty.json`. Araç: `sudo r9tctl fan-sweep` (r9td durdurulmuş, `allow_low_duty=1`). Kullanıcı başındaydı. AC bağlı, boşta; başlangıç 65/59 °C, fanlar auto 2871/2921 rpm.

| Duty | CPU fan RPM (medyan) | GPU fan RPM (medyan) | En düşük örnek | Sıcaklık (adım sonu) |
|---|---|---|---|---|
| %45 | 2855 | 2917 | 2840 / 2905 | 63 / 57 °C |
| %40 | 2563 | 2623 | 2560 / 2616 | 63 / 58 °C |
| %35 | 2253 | 2328 | 2250 / 2326 | 64 / 58 °C |
| %30 | 1933 | 2003 | 1928 / 2000 | 65 / 59 °C |
| %25 | 1597 | 1668 | 1593 / 1665 | 65 / 60 °C |
| %20 | 1264 | 1332 | 1261 / 1331 | 67 / 62 °C |

- **%20'ye kadar durma yok:** en düşük örnek 1261 rpm, kernel durma sınırı (500) çok uzakta; `stalls=0`. Eğri %50–100 ölçümünü (%50 ≈ 3180) doğrusal sürdürüyor, yaklaşık 64 rpm / %.
- Her adım 24 sn, ilk 8 sn atıldı, adım başına 4–7 örnek. Durdurma koşulu tetiklenmedi. Sonunda `auto`, `mode=0`, `mode_byte=128` (bit 0x40 temiz), `allow_low_duty=0` doğrulandı.
- Test modu tabanı tarama boyunca %20 kaldı: CPU 70 °C'nin altında, rampa başlamadı.
- **Spin-up ölçülmedi:** fan hiç durmadığı için duran bir fanın hangi duty'de kalktığı bu taramadan çıkmaz. Plan tabanı "spin-up + %5, en az %25"; ölçülen en düşük kararlı duty %20 olduğundan öneri, planın alt sınırı olan **`min_duty` = 25**.
- Fiziksel dinleme yapılmadı; kabul RPM ölçümüyle.
