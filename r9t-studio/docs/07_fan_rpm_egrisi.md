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
