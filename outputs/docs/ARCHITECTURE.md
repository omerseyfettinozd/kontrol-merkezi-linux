# Mimari — 0.5.0

- `gui.py`: Qt arayüzü; seçilen ayarlar, okunan durum ve son işlem sonucunu ayrı tutar. Tek donanım işlemi etkin olabilir. QProcess ile servis istemcisi ve GPU ölçümleri arka planda yürür.
- `fans.py`: fan hedefleri/eğrileri, mod/PWM/RPM doğrulaması; `fan-driver/r9t_fan.c` lease, sıcaklık koruması ve PM notifier içerir.
- `automation.py`: root sahipli profiller/kurallar, priz/pil ve süreç eşleşmesi, kalıcı duraklatma.
- `features.py`: sabit Fn Lock/standart pil eşik/dahili USB kamera adaptörleri; OEM destek kanıtı ve eksiklik nedenleri.
- `desktop.py` / `session.py`: ayrı kullanıcı servisi, KDE/Wayland kontrolleri, isteğe bağlı düşük Hz, bildirimler ve `session-idle.cpp` KF6 boşta kalma bağlantısı.
- `cooling.py`: AMD P-State CPU frekans sınırı ve NVML GPU kilit komutu; hazır soğutma profilleri, GPU sahiplik işareti ve otomatik geri dönüş. GPU hedefinin bağımsız geri okuması yoktur; voltaj değiştirilmez.
- `telemetry.py`: kullanıcı yetkisiyle Linux sensörlerini okur.
- `settings.py`: şema doğrulaması, atomik JSON, eski fan profili arşivi.
- `protocol.py`: sürüm 1; işlem kimliği, tam alan listesi, 8 KiB istek / 128 KiB yanıt sınırı. Bağlantı başına tek ileti.
- `service.py`: yalnız root donanım işlemleri; SO_PEERCRED kontrolü, sıralı komut işleme, watchdog, dönen işlem günlüğü.
- `hardware.py`: modele özel yetenekler; boost/RGB geri okuma ve geri yükleme; birleşik profil işlemi.
- `driver_io.py`: donanım kimliğini doğrulayan ioctl sınırı. Eski genel fan yazma yolu kullanılmaz; manuel hedefler kısıtlı R9T köprüsüne gönderilir.

Donanım servisi UI'dan bağımsızdır. Arayüz kapanınca servis çalışır; hiçbir
kullanıcı profili başlangıçta otomatik uygulanmaz. EC bağlantı hatası güç/RGB
kullanımını durdurmaz. Fan geri bildiriminde mod biti, PWM ve RPM ayrı ölçülür; köprü hazır değilse manuel kontrol devre dışı kalır.

## Sınırlamalar

Profil uygulama işlemi tüm sistem için bir kilit değildir: başka programlar
aynı sysfs/power-profile ayarlarını eşzamanlı değiştirebilir. Geri yükleme
her zaman mümkün olmayabilir; bu durumda servis açık hata verir. İstek zaman
 aşımı/bağlantı kaybı, donanım işleminin gerçekleşmediğini garanti etmez;
etkin değer bir sonraki başarılı durum sorgusunda gösterilir.

İşlem geçmişi uygulama ayarları içermez, yalnız işlem türü/sonucu/süresi içerir.
Tanılama kullanıcı adı ve cihaz seri numarası toplamaz. Eski günlükler döndürülür;
yalnız mevcut günlükteki son 100 kayıt başlangıçta yüklenir.

- `lighting.py`: 6 × 21 statik RGB harita doğrulaması ve hazır desenler. RPC profilinde `rgb_map` yalnız 126 RGB üçlüsü kabul eder, dosya yolu veya HID komutu kabul etmez. Tek renk ve harita birbirini dışlar. Işık zaman aşımı tam haritayı geri yükler.

## 0.7.0

- `history.py`: gerçek monotonic / UTC zamanlarıyla 30 dakika / 1200 satır sınırlandırılmış GUI oturum geçmişi. CPU sıcaklık/kullanım yerel okunur; fan ve GPU frekansı son başarılı root yanıtından, GPU sıcaklık/kullanım/tüketimi son NVIDIA ölçümünden en fazla 5 saniye tazelikle alınır. Grafikte 9 saniyeden uzun aralıklar birleştirilmez. CPU watt ölçümü veya ayarlanan GPU güç tavanı tüketim diye kaydedilmez.
- `CurveSmoother`: her fanın ayrı soğuma bekleme durumu; kontrol başlangıcında ve otomatiğe dönüşte sıfırlanır. Thermal override smoothing hedefini iki fan için 100'e çıkarır. Lease ve gerçek PWM/RPM kontrolü sürer.
- Profil `cpu_epp` alanı: yalnız dört bilinen enerji tercihi; cihazın tüm policy dosyalarının ortak destek listesinden geçmeli. Güç / boost / CPU frekansı işlemlerinden sonra uygulanır, sonraki işlem hatasında eski EPP değerleri geri yüklenir. Otomasyon uygulama öncesi anlık görüntüsüne EPP dahil eder. Yeni bir genel sysfs yazma RPC'si yoktur.
- `storage_memory_sensors`: sadece nvme / spd5118 hwmon sürücüleri, read-only sıcaklık ve etiketler. Sensör kaybolursa kart veri yok durumuna geçer.

## 0.8.0

- `temperature_target.py`: servis içinde 2 saniyelik fan/frekans geri beslemesi, ayrı CPU/GPU hedefleri, kısıtlı aralık doğrulaması, CPU geri dönüş kaydı. Yeni RPC yalnız iki tamsayı hedefi veya durdurma komutu alır. Fan ayarı değişirken RPM hata zamanlayıcısı sıfırlanmaz.
- Servis yeniden başlatma CPU geri dönüşünü ve mevcut GPU reset mekanizmasını kullanır; hedef etkinliği başlangıçta kapalıdır. Otomasyon yeniden etkinleştirilmeden veya elle güç/fan/frekans/profil uygulanmadan önce hedef geri dönüşü tamamlanır.

## 0.9.0

- `gpu_diagnostics.py` ve `r9t-gpu-diagnostics.py`: sınırlı, kullanıcı yetkili sysfs/procfs okuyucusu. NVML çağrısı veya aygıt açma yok; PID başlangıç kimliği yeniden kontrol edilir, açık dosyalar GPU eşlemesine göre ayrılır. GUI QProcess ile okur; tanılama sekmesinde düzenli root durum ve nvidia-smi sorguları başlatılmaz.
- Arayüz süreç adlarını düz metinle gösterir; komut satırı / ortam / UUID toplanmaz. GPU root kontrolü ve otomasyon bağımsız çalışır.

## 0.10.0

- `power_monitor.py`: root monitor döngüsünde en fazla saniyede bir paket enerji sayacı, sayaç aralığına göre taşma hesabı ve örnek tazeliği. Watt `cooling.cpu_power` altında salt okunur status ile döner; yeni yazma RPC'si yok.
- `dynamic_boost.py` / `r9t-dynamic-boost.py`: kullanıcı yetkisiyle NVIDIA firmware procfs desteği ve sabit systemctl show çağrısı; 2 saniye süre sınırı. GUI QProcess + 4 saniye dış sınır, destek/daemon ile davranış kanıtını ayırır.
- `cpu_power` History metric: RAPL paket wattı; root yanıtının gelişi ile sayaç örneğinin kendi monotonic zamanı ayrı kontrol edilir. NVIDIA N/A veya nonfinite alanlar tek tek boş değere dönüşür; diğer geçerli GPU ölçümleri korunur.
