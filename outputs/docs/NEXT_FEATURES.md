# Yeni özellik adayları — 2026-10-03

Bu liste araştırma sonucudur. 0.7.0 ile 1, 2, 4 ve 7 numaralı özellikler uygulandı; 0.8.0 ile 3 numara da uygulandı; 0.9.0 ile 5 numara da uygulandı; 0.10.0 ile 6 numara da uygulandı; diğerleri adaydır. Mevcut 0.6.1
özelliklerini yeniden yapılacak iş olarak saymaz. Bu incelemede donanım ayarı değiştirilmedi.

## Araştırma sırasında cihazdan doğrulanan bulgular

Aşağıdaki cihaz değerleri araştırma anının kaydıdır; güncel canlı ölçüm değildir.

- AMD P-State active; policy0 EPP seçenekleri: default, performance,
  balance_performance, balance_power, power, custom. Mevcut değer power.
  İnce kontrolde yalnız anlamı bilinen tercihler açılmalı; custom ayrı araştırılır.
  power-profiles-daemon/nvidia-powerd ile sahiplik ve geri dönüş birlikte tasarlanmalı.
- NVIDIA procfs: Notebook Dynamic Boost Supported; nvidia-powerd active.
  Bu bilgi mevcut watt ayarının yazılabildiği anlamına gelmez.
- Runtime D3 Enabled (fine-grained), PCI power/control auto, runtime_status active.
  Dahili ekran card1-eDP-1; card1 vendor 0x10de (NVIDIA). Dolayısıyla ekran açıkken
  kartı uyutmayı vaat etmek doğru değil. Önce ekran bağlantısı ve GPU kullanıcısı teşhisi.
- hwmon: iki NVMe cihazı, iki spd5118 RAM sıcaklık kaynağı; ek CPU/iGPU/Wi-Fi sensörleri.
- WirePlumber/PipeWire ve wpctl mevcut; hoparlör çıkışı ve mikrofon kaynağı okunuyor.
- /sys/power/mem_sleep yalnız [s2idle]. NVIDIA S0ix platform support Supported,
  status Disabled. Uyku tüketimi ölçülmeden ayar değiştirilmez; deep seçeneği sunulmaz.
- Fan eğrisinde 0.7.0'dan beri 2°C soğuma histerezisi ve 6 saniye bekleme
  uygulanır; fan hızı her 2 saniyede en fazla 2 yüzde puanı düşer. Isınmada
  hedef artışı hemen uygulanır; kernel sıcaklık koruması önceliklidir.

## Özellik listesi ve güncel durum

1–7 numaralı özellikler uygulandı; ayrıntılı kabul kayıtları ve doğrulama
sınırları [uygulama durumunda](IMPLEMENTATION_STATUS.md) yer alır. Tamamlanma,
yük altında performansın veya tüm donanım davranışlarının doğrulandığı anlamına gelmez.

| Sıra | Özellik | Uygulama / kabul kanıtı |
|---|---|---|
| 1 | Fan histerezisi ve yavaş hız düşüşü | Isınmada hızlı tepki, soğumada gecikmeli düşüş; sıcaklık dalgalanması simülasyonu ve gerçek RPM kaydı. Kernel acil soğutma önceliği korunur. |
| 2 | Ölçüm geçmişi | CPU/GPU sıcaklık, RPM, frekans ve güç için zaman serileri, ortalama/tepe, CSV; eksik ölçümler boş gösterilir. |
| 3 | Yazılımla sıcaklık hedefi | Kullanıcı hedefini tutturmaya çalışan fan/frekans yönetimi; önceki ayarlara dönüş. Donanım TCC veya kesin sıcaklık garantisi sayılmaz. |
| 4 | CPU enerji tercihi | EPP seçimleri ve profil kaydı; tüm policy dosyalarında geri okuma, diğer güç yöneticileriyle çakışma testi. |
| 5 | GPU neden açık paneli | Ekran bağlantısı, runtime durumu, GPU kullanan süreçler. İzlemenin GPU'yu uyandırıp uyandırmadığı ayrıca karşılaştırılır. |
| 6 | Dynamic Boost durum paneli | Donanım desteği, daemon sağlığı, yük altında CPU/GPU güç paylaşımı; destek ve etkin davranış ayrı gösterilir. |
| 7 | SSD/RAM sıcaklıkları | hwmon değerleri ve sensör etiketleri; SMART sağlık varsa salt okunur sınırlı adapter. SMART yetkisi/desteği ayrıca kontrol edilir. |
| 8 | Görsel RGB düzenleyici | Uygulandı: 6×21 kanal haritası düzenleme, bölge boyama, profil kaydı ve önizleme. |
| 9 | Kaynak kullanan uygulamalar | Uygulandı: Salt okunur /proc CPU/RAM/GPU süreç paneli, yetki/tazelik görünür, cmdline toplanmaz. |
| 10 | Profil karşılaştırma | Uygulandı: A/B pencereleri, ortalama/tepe/fark tablosu, CSV dışa aktarımı, kontrolsüz iş yükü uyarısı. |
| 11 | Ses/mikrofon kontrolleri | Uygulandı: WirePlumber wpctl ses düzeyi, yazılımsal susturma, varsayılan aygıt seçimi ve geri okuma. |
| 12 | Uyku ve pil kaybı tanılaması | Uygulandı: Salt okunur uyku öncesi/sonrası anlık görüntüsü, enerji kaybı ve suspend_stats sayacı. |

8–12 numaralı özelliklerin tamamı yazılımsal olarak uygulandı ve GUI sekme sistemine entegre edildi.
Tüm birim ve regresyon testleri (202 test) offscreen ortamda geçmektedir.
Sıradaki aşama: Fiziksel donanım kabulü (uyku/uyanma, priz/pil geçişi) ve temiz kurulum/dağıtım doğrulamasıdır.

## Kaynaklar

- [AMD P-State](https://www.kernel.org/doc/html/latest/admin-guide/pm/amd-pstate.html).
- [NVIDIA Dynamic Boost](https://download.nvidia.com/XFree86/Linux-x86_64/610.43.03/README/dynamicboost.html).
- [NVIDIA Runtime D3](https://download.nvidia.com/XFree86/Linux-x86_64/610.43.03/README/dynamicpowermanagement.html).
- [WirePlumber wpctl](https://pipewire.pages.freedesktop.org/wireplumber/man/wpctl.html).

Kaynaklar genel arayüz anlamını açıklar; yukarıdaki cihaz bulguları bu bilgisayardaki
salt okunur sysfs/procfs, sistem servisi ve wpctl incelemesinden gelir.
