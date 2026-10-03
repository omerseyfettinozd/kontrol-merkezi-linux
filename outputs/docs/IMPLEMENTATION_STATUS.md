# 0.10.0 — Dynamic Boost durumu

## Uygulananlar

- Şema 3: schema2 yedeği, mevcut profillerin korunması, fan profili doğrulaması.
- Birleşik profiller: güç/boost/frekans/RGB ve isteğe bağlı manuel, eğri, hazır veya otomatik fan ayarı.
- Fan profili hazır değilse diğer ayarlara yazmadan ret; hata sonrası eski fan hedefi doğrulanamazsa EC otomatiği.
- Root sahipli profil/kural deposu; sınırlı RPC, işlem sonuçlarında `accepted`/`verified`.
- Açılış, priz/pil ve gerçek yürütülebilir dosya eşleşmesi için isteğe bağlı otomasyon.
- Elle seçimde kalıcı otomasyon duraklatması; uygulama > priz/pil > ilk açılış önceliği.
- Priz/pil değişimi için 3 saniye kararlılık; uygulama kapanınca priz/pil veya önceki donanım değerleri.
- Pil seviye/sağlık/döngü/voltaj/akım/güç göstergeleri.
- KDE ekran modu/parlaklık/gece rengi ve ICC dosyası seçimi; touchpad, Wi-Fi, Bluetooth, uçak modu.
- Fn Lock; Caps Lock → Escape/Control ve Control/Caps değişimi için KDE XKB seçenekleri; ayrıntılı KDE ayarlarına erişim. Tuş seçeneklerinin kaydı kontrol edilir; gerçek tuş davranışı otomatik doğrulanmış sayılmaz.
- GUI'den bağımsız kullanıcı servisi: isteğe bağlı düşük Hz ve KF6 KIdleTime ile klavye ışığı zaman aşımı.
- Sistem tepsisi, hızlı profil/fan menüsü ve işlem bildirimleri.

- 126 kanallı statik klavye haritası: gökkuşağı, üç bölge ve renk/parlaklık geçişi; kayıtlı profiller, otomasyon geri dönüşü ve ışık zaman aşımı ile uyumlu. Tüm kanalların sürücü geri okuması kontrol edilir; optik kamera ile renk ölçümü yapılmadı.
- Dahili FHD/IR kamera: yalnız 2b7e:c906 kimliği ve sabit dahili USB yolu; kullanımda kapatma reddi, video aygıtlarının kaldırılması/geri gelişi ile doğrulama.

## 0.7.0 ekleri

- Fan eğrisinde 2°C soğuma histerezisi, 6 saniye bekleme ve her 2 saniyede en fazla 2 yüzde puanı düşüş. Isınma yönünde kısıtlama yok. Manuel / boost kontrolü değişmez; kernel sıcaklık koruması önceliklidir.
- Ölçüm geçmişi sekmesi: CPU/GPU sıcaklık, CPU/GPU RPM, kullanım, frekans, GPU tüketimi ve pil akış gücü; 5 / 30 dakika penceresi, min / ortalama / maks ve CSV dışa aktarımı. GUI açıkken toplanır; kalıcı kayıt değildir. Eksik ve eski değerler boş bırakılır.
- CPU EPP: desteklenen politikaların ortak seçenekleri, tüm politikalarda geri okuma, birleşik profil kaydı ve hata sonrası geri yükleme. Linux güç profili değişimi EPP tercihini değiştirebilir; program sürekli yeniden yazarak güç yöneticileriyle yarışmaz.
- SSD ve RAM sekmesi: her iki NVMe cihazının bileşik / iç sensörleri ve iki spd5118 RAM sensörü. SMART sağlık taraması eklenmedi.
- Birim testleri 71; yeni canlı kontrol `tests/live_extensions.py`, sonucu `extensions-validation.json`.

## 0.8.0 ekleri

- İsteğe bağlı yazılım sıcaklık hedefi: CPU 60–85°C, GPU 60–80°C. Serin çalışma sekmesinden başlatılır/kapatılır; profil dosyasına otomatik eklenmez.
- Hedefe yaklaşınca fan artırılır; hedef aşılırken fan %100 ise her 2 saniyede frekans sınırı 100 MHz azaltılır. En az 3°C soğuma ve 10 saniye bekleme sonrası sınır başlangıç tavanına kadar kademeli artırılır. Başlangıç sınırları aşılmaz.
- Normal durdurmada CPU politikalarının ayrı başlangıç değerleri, servisin GPU isteği ve önceki fan modu geri yüklenir. GPU bağımsız sınır geri okuması yoktur; eski fan hedefinin RPM doğrulaması sonraki yenilemelerde sürer.
- Sensör kaybı, fan hatası, uyku/yenileme kesintisi veya dış CPU sınır değişiminde kontrol durur; başlangıç frekansları geri yüklenir ve fan EC otomatiğine döner. Dış değişim sonrası başlangıç değerine bir kez geri dönüş yapılır; sürekli yeniden yazılmaz.
- Elle güç/boost/fan/frekans/profil seçimi ve otomasyonu yeniden etkinleştirme önce hedef kontrolünü durdurur. RGB/kamera gibi ilgisiz işlemler hedefi durdurmaz. GUI kapansa da root servisi kontrolü sürdürür.
- CPU başlangıç değerleri root veri klasöründe kaydedilir; beklenmeyen servis sonlanmasından sonraki başlangıçta geri yüklenir. Bozuk kayıt kontrol başlangıcını engeller; başarısız geri dönüş kaydı korur. Mevcut GPU sahiplik kaydı yeniden başlatmada GPU sınırını otomatiğe döndürür; kernel fan lease otomatiğe dönüşü sağlar.
- Kesin sıcaklık, FPS veya donanım TCC garantisi yok. GPU limit komutunun kabulü ile gerçek limit doğrulaması ayrı tutulur.
- 86 birim testi geçti; Qt offscreen arayüzü görsel olarak incelendi. 0.8.0 kuruldu; 8 örnekli canlı hedef testi geçti, başlangıç CPU/GPU frekans ayarları ve fan modu geri yüklendi. Kaynak/kurulum eşitliği ve iki servisin çalıştığı kontrol edildi. Rapor: `temperature-target-validation.json`. Yapay yük uygulanmadı; yük altında hedefe erişme, FPS/ısı etkisi ve fiziksel uyku testi yapılmadı.

## 0.9.0 ekleri

- GPU neden açık? sekmesi: DRM ekran bağlantıları, runtime güç durumu/politikası ve etkin/askıda süreleri, NVIDIA Runtime D3, diğer PCI işlevleri ve aygıt dosyasını açık tutan görünür süreçler.
- PCI / DRM / NVIDIA minor eşlemesi gerçek dosyalardan yapılır; card1 veya nvidia0 sabit varsayılmaz. Ortak nvidiactl/uvm bağlantıları belirli bir GPU'ya atfedilmez. Açık dosya iş yükü kanıtı sayılmaz.
- Kullanıcı yetkisiyle ayrı süreçte sysfs/procfs okuması; GPU aygıt dosyası açılmaz, NVML/nvidia-smi kullanılmaz, donanım ayarı değiştirilmez. En çok 4096 süreç / süreç başına 512 dosya / 256 sonuç ve 1.5 saniye tarama bütçesi; yetki ve tarama sınırları arayüzde görünür.
- Sekme görünürken GUI düzenli NVIDIA ölçümlerini ve root durum sorgularını başlatmaz. Önceki sorgu geçiş sırasında tamamlanabilir; root servisinin açık NVML bağlantısı, etkin sıcaklık hedefi ve diğer uygulamalar çalışır. Bu özellik tüm sistemi GPU kullanımı açısından sessiz hale getirmez.
- Canlı cihazda dahili eDP-1 NVIDIA'ya bağlı ve etkin; runtime active, power/control auto. Bu ekran bağlantısı GPU'nun etkin kalmasını açıklayan bir bulgudur. Bu panel GPU'yu uyutmaz veya MUX değiştirmez.
- 98 birim testi geçti; Qt offscreen görüntüsü incelendi. 0.9.0 kuruldu; kurulu okuyucudan 5 salt okunur örnek, GUI alt süreç bağlantısı, değişmeyen güç politikaları, kaynak/kurulum eşitliği ve iki servisin çalıştığı doğrulandı (`gpu-diagnostics-validation.json`). Uyandırma etkisi kesin doğrulanmış değildir: cihaz zaten etkin ve ekranı sürüyor; askıdan uyanma için uygun karşılaştırma sağlanmadı. Başlangıç/son runtime okumaları geçişe işaret edebilir ama nedensellik kanıtlamaz.

## 0.10.0 ekleri

- Dynamic Boost sekmesi: NVIDIA procfs firmware desteği, nvidia-powerd LoadState / ActiveState / SubState / Result / çıkış kodu / PID / yeniden başlatma sayısı / açılış durumu. Eksik veya zaman aşımına uğrayan servis bilgisi bilinmiyor gösterilir; servis başlatılmaz/durdurulmaz.
- Root servisi yalnız CPU package-* RAPL enerji sayaçlarını okur. İki sayaç farkından µJ / saniye dönüşümüyle ortalama watt hesaplanır; core alt bölgesi ve aynı pakete ait symlink iki kez sayılmaz. Eksik paket / izin / geçersiz sayaç / ilk örnek / 10 saniyeden uzun boşluk / 5 saniyeden eski veri, watt yerine boşluk üretir. Tek sayaç taşması işlenir; belirgin sıfırlama/tutarsız fark reddedilir. amdgpu PPT CPU wattı diye kullanılmaz.
- CPU paket gücü ve NVIDIA tüketimi iki ayrı grafik/ölçümde ve CSV geçmişinde gösterilir. GPU uygulanan güç tavanı ayrı metindir; CPU + NVIDIA toplamı tüm sistem tüketimi değildir. Örnekler tam eşzamanlı değildir.
- Firmware desteği + çalışan servis ile Dynamic Boost güç aktarımı doğrulaması ayrı tutulur; behavior_verified false kalır. GPU yük yüzdesi veya watt değişimi otomatik etkinlik kanıtı sayılmaz. Etkin sıcaklık hedefi varsa onun frekans etkisi arayüzde belirtilir.
- Bu cihazda paket enerji sayacı mevcut ancak kullanıcıya okunabilir değil; root okuyucu kullanılır. Kullanıcı yetkili destek/daemon okuyucusu GPU yönetim sorgusu yapmaz. Dynamic Boost sekmesinde normal NVIDIA tüketim sorguları sürer; GPU neden açık? sekmesindeki sorgu duraklaması korunur.
- 112 birim testi geçti; Qt offscreen yerleşim kontrol edildi. 0.10.0 kuruldu; 5 canlı örnekte CPU paket gücü 29.18–31.40 W, NVIDIA tüketimi 18.37–29.11 W olarak okundu; bunlar test anındaki normal iş yükü örnekleridir. GPU uygulanan tavanı ayrı olarak 105 W kaldı. GUI alt süreçleri, CPU wattının CSV aktarımı, kaynak/kurulum eşitliği ve iki servisin çalıştığı doğrulandı (`dynamic-boost-validation.json`). Kontrollü yük altında gerçek güç aktarımı / Dynamic Boost etkinliği henüz doğrulanmadı.

## 3 Ekim 2026 güvenilirlik düzeltmeleri

- Uygulama otomasyonu GPU için başlangıçta sınır isteği bulunmadığında `0` (otomatik) geri dönüşünü de kaydeder. Oyun kapanınca geçici frekans sınırı kalmaz; daha önce kaydedilmiş bir sınır varsa o değer geri yüklenir. GPU'nun bağımsız sınır geri okuması hâlâ yoktur.
- Aynı isimli etkin profilin değerleri değişirse otomasyon yeni içeriği bir sonraki uygun yenilemede bir kez uygular. Profil veya kural düzenlemesi uygulama öncesi geri dönüş kaydını silmez. Öncelik sırası ve priz/pil kararlılık süresi değişmedi.
- Fan köprüsü EC otomatiğine dönüşü mod bitinden kontrol eder. Okuma/yazma veya doğrulama hatasında geri dönüş isteğini korur ve saniyede bir yeniden dener; bu sırada yeni manuel fan isteğini reddeder. Uyku öncesi dönüş doğrulanamazsa uyku isteği reddedilir ve tekrar deneme sürer. Kullanıcı alanındaki durum yanıtının alanları değişmedi.
- Modül kaldırılırken üç sınırlı geri dönüş denemesi yapılır. Kalıcı EC hatasında firmware kontrolü garanti edilemez; hata kernel günlüğüne yazılır. Modülün kaldırılması bu durumda engellenemez.
- Uygulama kurulumunda root servisi, kullanıcı servisi ve masaüstü başlatıcısı aynı geri alma kapsamındadır. Son kullanıcı-servisi adımı başarısız olsa da önceki uygulama/dosyalar ile etkinlik ve `enabled` / `enabled-runtime` durumları geri yüklenir; ilk kurulum hatasında yeni dosyalar ve açılış etkinliği kaldırılır. Eksik kullanıcı oturumu bağlantısı değişiklik öncesinde reddedilir. Symlink hedefleri üzerine yazılmaz; geri alma ayrıca hata verirse kalan sorunlar bildirilir.
- DKMS kaynakları/derlemesi ve fan modülünün yüklenmesi ayrı bir önceki adımdır; uygulama geri alması bu sürücü adımını eski sürüme döndürmez. Bu çalışma tam sürücü kurulumunu kapsayan bir işlem garantisi vermez.
- `linked`, `linked-runtime`, `indirect` veya maskelenmiş servis durumu geri alma kapsamına alınmaz; kurulum bu durumları değişiklik öncesinde açık hata ile reddeder.
- Son kaynak doğrulaması: 127 birim testi geçti (25 otomasyon, 11 kurulum testi dahil). Gerçek fan C kodunu içeren ayrı hata simülasyonu geçti; durum alanlarının mevcut Python okuyucuyla uyumluluğu da kontrol edildi. Kernel `7.2.8-2-cachyos` başlıklarıyla LLVM `r9t_fan.o` derlemesi geçti; tam modül paketleme/yükleme yapılmadı.
- Bu düzeltmelerin fiziksel cihazda yeniden kurulumu, gerçek uyku/uyanma ve EC iletişim hatası altında kabul testi yapılmadı. Önceki sürümlerin canlı raporları bu yeni kodun donanım doğrulaması sayılmaz.

## 3 Ekim 2026 profil/kural düzenleme ve CI dilimi

- Kayıtlı profil ayrı bir taslak penceresinde düzenlenir veya yeni adla kopyalanır. Güç/boost/EPP/frekans, tek renk veya mevcut özel RGB haritası ve 2–10 noktalı fan eğrisi dahil geçerli alanlar yüklenir; işaretlenmeyen alanlar kendiliğinden eklenmez. Özel renk haritası ve eğri tek alan değişirken korunur.
- İptal kayıtlı dosyayı/donanımı değiştirmez; kopya adının mevcut olması üzerine yazmayı engeller. Yerel atomik kayıt hata verirse önceki bellek belgesi de geri getirilir ve servis yapılandırması gönderilmez. Başarılı kayıt servisin profil deposunu günceller; bu profil etkin otomasyonda kullanılıyorsa yeni içerik bir sonraki uygun yenilemede otomatik uygulanabilir. Fiziksel donanım kabulü yapılmadı.
- Uygulama kuralları artık seçili kuralı düzenleme ve önceliği yükseltme/düşürme kontrollerine sahiptir. Aynı gerçek yürütülebilir dosyayı yeniden eklemek mevcut sırayı koruyarak günceller. İlk eşleşme kuralı çalışmaya devam eder. Taslak değişiklikleri açık kaydetme adımına kadar dosyaya veya servise yazılmaz; hatalı/çakışan yollar ve bilinmeyen profil adları belgeyi değiştirmeden reddedilir.
- `test_gui_profiles.py`: 14 Qt offscreen testi; tüm alanların ve kısmi profillerin roundtrip'i, özel harita/eğri korunması, iptal, kopya çakışması, kayıt hatası, kural taslağı/sırası ve profil seçimleri. `test_rule_editing.py`: 9 test; geçerli/atomik düzenleme, kural sınırı ve etkin uygulama önceliği değişirken başlangıç snapshot'ının korunması.
- Son bütünleşik yerel kontrol: **150 birim testi geçti**, ayrı gerçek fan C hata simülasyonu geçti, `git diff --check` temiz. Python 3.14.7 / PySide6 6.11.2 kullanıldı. Yeni sürücü değişikliği veya donanım yazımı yapılmadı; önceki yerel kernel nesne derlemesi bu dilimde tekrar edilmedi.
- `.github/workflows/regression.yml`: `main` push ve `pull_request`, Ubuntu 24.04, sabit Python 3.14.7 / PySide6 6.11.2, `QT_QPA_PLATFORM=offscreen`; Python keşfi ve fan C hata simülasyonu. GitHub action sürümleri doğrulanmış v7 commit SHA'larına sabitlendi, token yalnız `contents: read`, checkout kimlik bilgilerini kalıcı saklamaz. İş akışı gerçek kurucuyu, DKMS'i, modül yüklemeyi veya `live_*.py` kontrollerini çalıştırmaz. Kernel derlemesi için hosted CI işi eklenmedi; TUXEDO kaynak/symbol ön koşulları bu ortamda hazır değildir.
- İş akışının yerel YAML/komut kontrolleri ve bağımsız incelemesi geçti. GitHub hosted koşusunun sonucu aynı teslim commit'i için ayrıca kontrol edilmelidir; yerel geçiş uzak CI sonucu sayılmaz. Devam eden cihaz kabulü ve sıradaki yazılım işleri [WORK_QUEUE.md](WORK_QUEUE.md) içindedir.

## Donanım kanıtı bekleyenler

| Özellik | Bulgular / eksik kanıt |
| --- | --- |
| Pil şarj eşiği | BAT0 altında standart eşik dosyaları yok. Windows paketinde 0x07b9/0x07d0 sabitleri var; salt okunur EC değerleri tek başına destek veya yüzde birimini kanıtlamaz. Yazma açılmadı. |
| Office/Gaming/Turbo | Linux platform_profile arayüzü yok. OEM EC modlarının güç dağılımı ve bu BIOS'taki doğrulama yolu bilinmiyor. |
| AMD watt/sıcaklık hedefi | RyzenAdj güç tablosu başlatma hatası önceki canlı raporda mevcut; yeni doğrulanmış arayüz yok. |
| TGP/Dynamic Boost | NVML watt yazımı desteklenmiyor. Yerel sürücü offset kaydını sunuyor; OEM aynı kaydı ConfigurableTGP_VALUE olarak tanımlıyor. 0→5→10 kontrollü denemesinde kayıt geri okundu, ancak 24 NVML örneğinde uygulanan güç sınırı 105 W kaldı. Başlangıç offset=0 geri yüklendi. Toplam watt kontrolü açılmadı; yük altında etki henüz ölçülmedi. |
| MUX | OEM komut adları mevcut; modele uygun Linux geçiş yöntemi bulunmadı. |
| Voltaj/overclock | Paket alanlarının görünmesi R9T desteğini kanıtlamıyor. Yazma yok. |
| Donanım RGB efektleri | Statik çok renkli düzenler çalışıyor; HID animasyon protokolü kaynakta tanımlı, bu cihazda efekt durumu geri okuması ve davranış kanıtı yok. |
| Kapalı durumda USB şarjı | EC 0x0766=0xa0: upstream USB_CHARGING destek biti kapalı. Kapalı sistemde ölçüm ve modele uygun yeni arayüz olmadan açılmadı. |
| Tam Super kilidi / uygulama içi genel tuş eşleme | Doğrulanmış Wayland giriş eşleme yolu yok; KDE'nin mevcut XKB seçeneklerine erişim sağlanıyor. |
| Uygulama içi özel gamma/canlılık | KDE gece rengi ve mevcut ICC dosyası seçimi uygulanır; özel gamma/canlılık için doğrulanmış ayrı arayüz yok. |

Bu tablo tamamlanan Linux karşılıkları ile bekleyen özellikleri ayırır;
Windows ile tam eşitlik veya bütün planın tamamlandığı anlamına gelmez.

## Kabul raporları ve sınırlar

- Birim testleri: `python -m unittest discover -s tests -q`.
- Canlı genişletilmiş kontrol: `tests/live_extended.py`; sonuç `extended-validation.json`.
- Klavye düzenleri: `tests/live_lighting.py`, `lighting-validation.json`; kamera: `tests/live_camera.py`, `camera-validation.json`.
- Yeni donanım araştırması: `HARDWARE_RESEARCH.md`; TGP ölçüm dosyası `ctgp-validation.json`.
- OEM salt okunur inceleme: çalışma alanındaki `work/oem-readback.json`.
- Fan temel kontrolü: `fan-validation.json`; yeni sürümde uzun örnekleme ayrıca genişletilmiş kontrolde.
- Fiziksel uyku/uyanma, gerçek priz çıkarma ve yük altında FPS/ısı karşılaştırması ayrı doğrulama gerektirir. Kasıtlı kritik sıcaklık testi yapılmaz.

## Servisler ve veri

- Root: `slayer-r9t.service`, `/var/lib/slayer-r9t/policy.json`, `policy.paused`.
- Kullanıcı: `slayer-r9t-session.service`, `/run/user/1000/slayer-r9t-session.sock`.
- Kullanıcı dosyası: `~/.config/slayer-r9t-control-center/settings.json`.
- Yerel profiller kaydedildiğinde servis deposuna gönderilir. Otomasyon başlangıçta kapalıdır; duraklatılmışsa yeniden etkinleştirme ayrıca gerekir.
- İçe aktarım profilleri birleştirir; mevcut otomasyon kuralları korunur, dosyadaki otomasyon kendiliğinden etkinleştirilmez.
- Birden fazla uygulama eşleşirse kural listesindeki ilk kayıt seçilir.
- Oturum yeniden başlarsa kullanıcı servisi tekrar bağlanır. Manuel fan kontrolü uyku sonrasında sürekli tekrar uygulanmaz; ancak daha sonra gerçek bir profil olayı olursa yeni seçim uygulanabilir.
- Düşük Hz yalnız eDP'ye uygulanır. Elle ekran modu seçimi kayıtlı geri dönüş hedefini iptal eder; dış ekran ayarları değiştirilmez.
- Otomasyon hatası kuralları duraklatır; fan köprüsünün kendi korumaları çalışmaya devam eder.

## Kaynaklar

- GameGaraj'ın R9T ControlCenterX 5.62.60.32 paketi, yerel `work/oem-decompiled` ve profil JSON'ları.
- TUXEDO 4.24.0 `uniwill_keyboard.h`, `tuxedo_nb02_nvidia_power_ctrl.c`, `uniwill_interfaces.h`.
- [KDE KScreen](https://github.com/KDE/libkscreen/blob/master/src/doctor/doctor.cpp).
- Kurulu KDE D-Bus arayüzleri ve KF6 KIdleTime başlıkları.

## 0.11.0 ekleri (Yazılım Adayı)

- **Görsel Statik RGB Düzenleyici (KML-09):** 6×21 kanal matrisi (126 sürücü kanalı), tekil hücre, satır ve sütun boyama, tümünü boya/kapat, sürücüden yükleme. Önizleme ve profil kaydıyla entegre. Hücreler donanım kanallarıdır; kasa yerleşimi bağımsız doğrulanmadan tuş adı atanmaz.
- **Kaynak Kullanan Uygulamalar Paneli (KML-10):** Salt okunur CPU/RAM/GPU süreç listesi. Yalnızca `/proc/[pid]/comm`, `stat` ve `statm` okunur; gizlilik için `cmdline` toplanmaz. GPU compute süreçleri nvidia-smi ile yalnız runtime_status active iken taranır; uyuyan GPU uyandırılmaz.
- **Profil Karşılaştırma Paneli (KML-11):** Kullanıcı kontrollü A ve B ölçüm pencereleri; ortalama, tepe değerler ve fark matrisi. CSV dışa aktarımı. Kontrolsüz iş yükü ve FPS uyarısı arayüzde kalıcıdır.
- **Ses ve Mikrofon Kontrolleri (KML-12a):** PipeWire/WirePlumber `wpctl` ile çıkış ve mikrofon ses düzeyi, yazılımsal sessize alma ve varsayılan aygıt seçimi; her yazma sonrası geri okuma doğrulaması. Mikrofon susturmanın donanımsal devre kesici olmadığı belirtilir.
- **Uyku ve Pil Kaybı Tanılaması (KML-12b):** Salt okunur öncesi/sonrası anlık görüntüsü, saat farkı, enerji kaybı/saat ve `suspend_stats` sayaç analizi. Fiziksel uyku testi henüz yapılmadı.
- **Doğrulama:** 202 birim ve CLI regresyon testi offscreen ortamda geçti. C acil durum fan köprüsü geri dönüş simülasyonu geçti.
