# 10 Ekim 2026 — R9T Curve Optimizer canlı erişim denemesi

Canlı CPUID marka metni **Ryzen 9 8940HX**, family 25 / model 97; BIOS N.1.19GAR06,
kernel 7.2.9-2-cachyos. Önceki 8945HX tanımı bu oturumda doğrulanmadı ve düzeltildi.

RyzenAdj 0.19.0 `--info` Dragon Range PM izleme tablosunu desteklemiyor; bu,
CO komutlarının desteklenmediği anlamına gelmiyor. Kaynakta Dragon Range için
RSMU all-core setter 0x07, per-core setter 0x06 bulunuyor. G-Helper Linux
aynı aile için 0xD5 GetDldoPsmMargin geri okumasını kullanıyor.

Yeni sürücü kurulmadan mevcut PCI / devmem altyapısıyla sabit aileye ve sabit
komutlara sınırlı deney yapıldı. Başlangıçta 16 çekirdekte +10 okundu.
+5, 0, -5 yazımları kabul edildi ve her aşamada 16 çekirdeğin tamamında geri
okundu. Başlangıç +10 geri yüklendi ve 16/16 geri okuma doğrulandı.
CO sayısı volt veya milivolt değildir; +10 değerinin tek başına ısınmanın nedeni
olduğu ileri sürülmüyor.

| CO | CPU paket ortalama W | En yüksek CPU °C | SHA256 kB/s (CPU zamanı) |
| --- | ---: | ---: | ---: |
| +10 | 24.846 | 65.50 | 1165197.61 |
| +5 | 23.293 | 65.50 | 1166255.86 |
| 0 | 21.812 | 65.38 | 1165181.98 |
| -5 | 22.280 | 65.38 | 1166115.27 |

Her aşama CPU0'a sabitlenmiş 8 saniyelik OpenSSL SHA256 yüküydü. Turbo servis
okumasında kapalıydı; 300 Hz korundu. Bu sıralı kısa test, arka plan yükünü ve
zamanla değişen sıcaklığı kontrol etmez. Kalıcı tasarruf yüzdesi, genel performans
ve uzun süreli kararlılık sonucu çıkarılmaz. Journal'ın erişilebilen son on
dakikasında MCE/Hardware Error eşleşmesi görülmedi; kapsamlı hata taraması değildir.

Deneyde 85°C durdurma, sınırlı SMU yanıt bekleme, SIGINT/SIGTERM geri dönüşü ve
ana süreç çıkarsa/75 saniye aşılırsa başlangıç değerini geri yükleyen ayrı gözcü
vardı. Gerçek işlemde normal geri yükleme geçti; bu hata yolları fiziksel olarak
zorlanmadı. Yeniden başlatma sıfırlaması denenmedi.

Kanıt ve deney kaynakları:
`/home/chaple/.local/state/thermal-diagnostics/curve-optimizer-20261010/`.
Kurulu servise veya üretim arayüzüne CO özelliği eklenmedi. Kalıcı otomasyon yok.

Sonraki kabul: tekrarlı A/B; turbo frekanslarında her çekirdek ve bütün çekirdek
yükü; düşük yük/boşta geçişler; uzun kararlılık ve donanım hataları; uyku/yeniden
başlatma ve geri dönüş. Testler geçmeden profil üretimde etkinleştirilmemeli.

Kaynaklar:
- https://github.com/FlyGoat/RyzenAdj/pull/359
- https://github.com/utajum/g-helper-linux/blob/master/docs/amd-undervolting.md
- RyzenAdj kaynak SHA: 5775fc3e6dbb25c7030ee2d100a1bdd6e8bf2d0a
- G-Helper kaynak SHA: b560c5fb6bc57b3d2f27cf47ca1c388c1d5f7dc7


## Devam: EPP, sorgu yükü ve turbo açık çekirdek taraması

- EPP ABAB: balance_performance birleşik 21.199 W, balance_power 22.078 W.
  Her faz 3 saniye yerleşme ardından altı servis örneğidir; arka plan açıktır.
  Fayda ölçülmedi, balance_performance geri yüklendi.
- Yalnız kontrol merkezi GUI penceresi kapatıldı. Fan/root servis çalışır.
  Sorgulu altı örnekte 21.079 W; 20 saniye ek sorgusuz servis CPU kullanımı
  tek çekirdeğin %3.35'i. Son 23.629 W yalnız anlık örnektir. Belirgin paket
  güç kazancı gösterilmedi; bütün tüketim kontrol merkezine bağlanamaz.
- Turbo geçici açıldı, CPU üst sınır otomatik 5386 MHz yapıldı. +10 CPU0
  8 saniye: 44.477 W, 80.00°C, SHA256 2388282.92 kB/s (CPU zamanı).
  CO -5 yazıldı/16 çekirdekte geri okundu. Her çekirdek ayrı dört saniyelik
  OpenSSL SHA256 testini çıkış 0 ile tamamladı. Eşzamanlı bütün çekirdek
  yükü ve uzun kararlılık testi değildir; tam turbo frekansına ulaşıldığı
  ayrıca ölçülmedi.
- 10 saniye boşta geçişinden sonra -5 CPU0 sekiz saniye: 43.265 W,
  80.62°C, 2370543.22 kB/s. Bu tek sıralı karşılaştırmada güç yaklaşık
  %2.7, işlem hızı %0.7 düşük; tekrar/kontrollü arka plan olmadığı için
  doğrulanmış tasarruf veya eşdeğer performans iddiası yapılmaz.
- Sonuç exit 0; **CO +10 16/16 geri yüklendi**. Servis profili eski haline
  doğrulandı: turbo kapalı, CPU max 2401 MHz, EPP balance_performance,
  fan otomatik. Ekran kscreen-doctor canlı okumasında 300 Hz.
- Salt okunur MP1 0x23: response 1, raw 0x370064; ZenStates yorumuyla
  yapılandırılmış sustained güç 55 W / termal sınır 100°C. RSMU fused PPT
  0xDC: response 1, raw 0x27100 (160000). Fused değer çalışma anındaki
  mutable PPT başlangıcı değildir. Fast/slow/STAPM başlangıçları tam
  okunamadığı için PPT yazılmadı. Çalışma termal sınırı 85°C tutuldu.
- Deney geri dönüş gözcüsü 240 saniye; normal geri yükleme doğrulandı,
  zorunlu süreç ölümü/yüksek sıcaklık yolları ayrıca denenmedi.
- ZenStates-Core kaynak SHA: 869b7e82572337e5a673e92230edc92bddf0e44b.
- Kanıt: `/home/chaple/.local/state/thermal-diagnostics/co-core-sweep-20261010/`
  (önce/sonra, 18 benchmark, C ve bounded SMU kaynağı, binary, sonuç).
  EPP: aynı üst dizinde `epp-comparison-20261010.json`.
  CO kalıcı uygulanmadı, üretim kaynak/servis/sürücü değişmedi.
