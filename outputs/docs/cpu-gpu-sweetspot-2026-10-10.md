# 10 Ekim 2026 — R9T CPU/GPU beş döngü verimlilik ölçümü

Canlı cihaz: Ryzen 9 8940HX, 16 çekirdek; RTX 5070 Laptop GPU; BIOS N.1.19GAR06,
kernel 7.2.9-2-cachyos, NVIDIA 615.78.08. Priz bağlı. Panel 2560×1600 / 300 Hz.

Beş döngünün tamamı gerçek donanımda yapıldı. Her döngü: 10 s tek çekirdek SHA256,
25 s 16 çekirdek SHA256, 30 s GPU terrain, 40 s eşzamanlı CPU+GPU. CPU çekirdek
sabitlenmiş OpenSSL SHA256 16 KiB blok, her sonuç ilk digest ile karşılaştırıldı;
wall-clock MB/s ölçüldü. GPU glmark2 2023.01 `--off-screen --frame-end finish`
2560×1600 terrain; renderer gerçek NVIDIA olarak okundu. FPS sentetik sahneye aittir.

Bütün döngülerde aynı Serin fan eğrisi kullanıldı: 45°C/%60, 60/%75, 75/%90,
85/%100. CPU EPP balance_performance, boost açık. CPU üst sınırları sysfs/servis
ile geri okundu. GPU üst sınır komutları NVML tarafından kabul edildi; bağımsız
locked-limit getter yoktur. Bunun yerine yükte gözlenen tüm GPU frekans örnekleri
istenen sınırın altında kaldı (15 MHz tolerans). GPU watt sınırı veya voltaj/CO
ayarına yazılmadı; CO önceki +10 durumunda kaldı. Kontrol GUI kapalı, fan servisi açık.

CPU 90°C / GPU 85°C durdurma vardı; hiçbir faz kesilmedi. Tüm benchmark çıkışları
0; CPU karşılaştırma hata sayıları 0. Sıcaklık/NVML yaklaşık 0.4 s örneklendi.
CPU wattı servisin RAPL package delta ölçümünden, 3 saniyelik status sorgusuyla
alındı; 6 saniyeden bayat önbellekler ortalamaya katılmadı. CPU+GPU toplamı duvar
prizi veya bütün laptop tüketimi değildir.

Soğuma: döngü başında en az 15 saniye, CPU≤68/GPU≤63°C, en çok 120 saniye.
CPU testinden sonra GPU testine de soğuma kondu. Başlangıçlar tam eşit değil:
ilk döngü 64.625/57°C, son adayın üçüncü ve beşinci döngüleri 67.875/60 ve68/60°C.
Fan RPM yaklaşık CPU5000–5500/GPU4400–5500; sessizlik kabulü yapılmadı.
Masaüstü/Codex/diğer açık uygulamalar devam etti. Fazlar kısa; uzun ısı dengesi,
saatler süren kararlılık, uyku/reboot ve gerçek oyun FPS kabulü ayrıca gerekir.

## Karşılaştırma

GPU FPS sütunu yalnız GPU fazıdır; watt/sıcaklık sütunları birleşik 40 saniyelik
fazdır. En kötü kayıp beş ayrı hız oranının en düşüğüdür: tek CPU, tüm CPU,
GPU, birleşik CPU, birleşik GPU. Kabul ≥%95 hız ve hiçbir termal/hata kesilmesi.

| Döngü | CPU/GPU üst sınır MHz (0=otomatik) | GPU FPS | Birleşik CPU/GPU tepe °C | Birleşik CPU+GPU W | En kötü hız kaybı | Kabul |
| --- | --- | ---: | --- | ---: | ---: | --- |
| 1 | 0/0 | 1127 | 80.50/84 | 139.46 | %0.00 | geçti |
| 2 | 4900/0 | 1124 | 80.38/84 | 139.32 | %2.59 | geçti |
| 3 | 4900/2475 | 1078 | 80.50/79 | 125.42 | %4.35 | geçti |
| 4 | 4900/2385 | 1064 | 80.62/77 | 121.60 | %5.59 | elendi |
| 5 | 4900/2475 | 1072 | 80.50/79 | 125.91 | %4.88 | geçti |

## Uygulanan sonuç

Son aday tekrarında CPU 4900 / GPU 2475 MHz; aynı Serin fan,
boost açık, EPP balance_performance. Beşinci döngünün en düşük hız oranı
%95.12; birleşik CPU+GPU güç farkı %9.72 düşük.
CPU tepe 80.50→80.50°C, GPU 84→79°C.
Bu sonuç ölçülen workload içindir; genel oyun FPS garantisi değildir.

Profil `Sweet Spot - 5 Döngü` kontrol merkezi ayarlarına ve servis profil belgesine
kaydedildi; mevcut Oyun - Serin ve otomasyon kuralları korundu. Şu an runtime aktif;
startup otomasyonu null, yeniden başlatmada otomatik yüklenmesi yapılandırılmadı.
Eski kullanıcı ayarları ve eski servis politikası kanıt dizininde yedeklendi.

Kanıt: `/home/chaple/.local/state/thermal-diagnostics/sweetspot-five-20261010`; CSV ve ham faz JSON/benchmark logları, önce/sonra,
C benchmark ve Python orchestration kaynakları. Guard normal tamamlanmada
kapatıldı; zorunlu süreç ölümü/termal kesilme yolları ayrıca fiziksel denenmedi.
Üretim uygulama/servis/sürücü kaynakları değiştirilmedi. Commit/push yapılmadı.

Birincil araç belgeleri:
- [glmark2 kullanım](https://github.com/glmark2/glmark2/blob/master/doc/glmark2.1.in)
- [NVIDIA SMI saat/güç izleme](https://docs.nvidia.com/deploy/nvidia-smi/index.html)


## Yük sonrası kısa boşta kontrol

Altı status örneği (4s aralık + sorgu süresi): CPU paket ortalaması24.825W;
CPU66→63°C/GPU58→55°C, CPUfan4900→4697RPM/GPU4482→4329RPM.
Boost açık/CPU4900/GPU istek2475 devam etti; kernel son25dk hata eşleşmesi0.
Bu yük sonrası soğuma örnekleridir; kararlı uzun boşta karşılaştırma ve boşta
enerji kazancı kanıtı değildir. Profil performans/yük dengesi için seçildi.


### 2026-10-10 — CS2 gerçek oyun CPU sıcaklığı düzeltmesi

Kullanıcı oyundayken CPU sıcaklığını düşürme yetkisi verdi. Önceki 4900/2475 kısa sentetik profil CS2 oyununda CPU Tctl 90,5–91°C, GPU 83–84°C verdi; bu profil uzun oyun için kabul edilmiş değildir. Yeni başlangıç CPU paket gücü ~73 W'a ulaştı. CPU üst sınırı sırasıyla 4500, 4000, 3800 ve 3500 MHz denenerek azaltıldı. 4500 isteği sonradan 4159/4426/5386 olarak değişti; çalışan nvidia-powerd aynı scaling_max_freq alanını değiştirebilen bir servistir (NVIDIA belgesi: https://download.nvidia.com/XFree86/Linux-x86_64/610.43.03/README/dynamicboost.html). Yazmayı yapan süreç doğrudan izlenmedi; kesin neden kanıtı yok. 4000/3800 aşamalarında CPU yaklaşık 88–91°C kaldı.

Seçilen geçici profil: CPU 3500 MHz, boost açık, EPP balance_performance, GPU 2475 MHz, Serin fan eğrisi. 3500 geri okumaları sabit; 20:06:14–20:07:03 örneklerinde CPU 86→~85°C (84,75–85,38°C son örnekler), paket gücü yaklaşık 46–49 W, GPU 82–84°C ve kullanım %65–85. GPU yükü oyun sahnesiyle değişti; bu kontrol edilmiş FPS A/B testi değildir. Average FPS / 1% low ölçülmedi, %5 minimum kayıp iddiası bu profile aktarılmaz. 300 Hz değiştirilmedi; NVIDIA Dynamic Boost, voltaj ve watt limitleri değiştirilmedi. Profil kullanıcı settings.json ve servis politikasına 'Oyun - CPU Serin 3500' adıyla eklendi; eski profiller korundu, otomasyon duraklatılmış ve başlangıç profili boş kaldı.

Kanıt: `/home/chaple/.local/state/thermal-diagnostics/cpu-game-tune-20261010/`. 4500-samples ve 3800-samples dosyaları kademeler arası geçişler içerir; her kaydın cpu_cap alanı esas alınmalı. 3500-samples.json seçilen aşamadır. Uzun oyun ısı dengesi ve FPS kabulü açık.
