# Slayer R9T Kontrol Merkezi — 0.10.0

Program uygulama menüsünde **Slayer R9T Kontrol Merkezi** adıyla açılır.
Kurulu dosyalar `/usr/local/lib/slayer-r9t/` altındadır.

## Kullanım

- **Genel durum:** CPU/GPU sıcaklığı ve kullanımı, GPU gücü, pil seviyesi; RAM, SSD sıcaklığı, pil kapasite sağlığı ve döngü sayısı. Bulunamayan sensör `—` olarak gösterilir.
- **Güç ve işlemci:** Linux güç profili ve AMD P-State CPU boost. Seçim ile sistemden okunan etkin değer ayrı gösterilir; ayar ancak Uygula düğmesiyle değişir.
- **Fanlar:** gerçek CPU/GPU RPM, ayrı %50–100 hedefler, maksimum soğutma ve fan eğrileri.
- **Serin çalışma:** hazır Sessiz/Ofis, Serin Dengeli, Serin Oyun profilleri; CPU ve GPU üst frekans sınırları. Voltaj ayarı bulunmaz.
- **Klavye:** tek renk RGB ve parlaklık. Donanım 50 parlaklık adımı kullanır; örneğin %51 isteği %52 olarak uygulanabilir. Arayüz gerçek değeri gösterir.
- **Profiller:** güç, boost ve RGB seçimlerini birlikte kaydetme/uygulama, silme, JSON içe/dışa aktarma. Aynı adla içe aktarım o profilin değerlerini günceller. Uygulama hatasında önceki ayarlar geri yüklenir; geri yükleme doğrulaması başarısızsa ayrıca bildirilir.
- **Pil:** kapasite, sağlık, voltaj, akım, güç; şarj sınırı ancak standart arayüz mevcutsa açılır. Bu cihazda henüz doğrulanmadı.
- **Ekran:** bağlı ekranlara göre mevcut Hz/mod, parlaklık, gece rengi ve ICC dosyası seçimi.
- **Otomasyon:** açılış, priz/pil ve gerçek yürütülebilir dosyaya göre profil; düşük Hz ve klavye ışığı zaman aşımı. Varsayılan kapalıdır.
- **Cihazlar:** Wi-Fi, Bluetooth, uçak modu, touchpad ve eksik OEM desteğinin nedenleri. Klavyede Fn Lock ve KDE Caps Lock eşleme seçenekleri bulunur.
- **Tanılama:** model, BIOS, çekirdek, destek durumu ve işlem geçmişi. JSON raporu dışa aktarılabilir.

İşlem sonucu otomatik yenilemelerle silinmez. Sensör ve servis işlemleri arayüzü
bloke etmeden yürür. Arayüzü kapatmak donanım servisini kapatmaz.

Ayar dosyası `~/.config/slayer-r9t-control-center/settings.json` altında, atomik
olarak ve yalnız kullanıcı erişimiyle kaydedilir. Eski fan profilleri bu dosyada
arşivlenir. Bozuk ayar dosyası `.invalid-...` adıyla korunur.

**0.8.0:** Serin çalışma sekmesindeki Sıcaklık hedefi, fanları ve CPU/GPU frekans sınırlarını sıcaklığa göre yönetir. Durdurunca önceki ayarları geri yükler. Kesin sıcaklık garantisi vermez; GPU sınırının bağımsız geri okuması yoktur. Sensör/fan hatası veya uyku sonrası kontrol durur. GUI kapansa da servis çalışır. Ayrıntılar: [durum belgesi](docs/IMPLEMENTATION_STATUS.md).

**0.9.0:** GPU neden açık? sekmesi ekran bağlantısını, güç durumunu ve GPU aygıtını açık tutan görünür süreçleri gösterir. Bu sekmede düzenli NVIDIA/servis durum sorguları duraklar; donanım ayarları değişmez. Aygıt bağlantısı gerçek GPU iş yükü anlamına gelmez. Yetki kısıtları gösterilir; bu cihazdaki askıdan uyandırma etkisi henüz doğrulanmadı.

**0.10.0:** Dynamic Boost sekmesinde firmware desteği, nvidia-powerd sağlığı ve CPU paket / NVIDIA güç grafikleri bulunur. CPU paket wattı root servisi tarafından enerji sayacı farkından hesaplanır. Bu ölçümler kesin Dynamic Boost etkinliği veya güç aktarımı kanıtı değildir. CPU paket gücü Ölçüm geçmişi CSV dosyasına da eklenir.

## Doğrulanan destek ve sınırlar

| İşlev | Durum |
| --- | --- |
| Linux güç profili | Sistemden geri okunarak doğrulandı |
| CPU boost | Tüm CPU politikalarından geri okunarak doğrulandı |
| RGB / parlaklık | Tüm klavye LED arayüzlerinden geri okunarak doğrulandı |
| Birleşik profil | Gerçek cihazda güç + boost + RGB ile doğrulandı |
| CPU frekans sınırı | Tüm CPU politikalarından geri okunarak doğrulandı |
| GPU frekans sınırı | Sürücü komutu kabul edildi; sınırın bağımsız geri okuması yok |
| CPU/GPU undervolt | Doğrulanmış voltaj arayüzü bulunmadı; kapalı |
| Sensörler | Salt okunur; bulunamayan veri gösterilmez |
| Elle fan / fan eğrisi | R9T köprüsüyle ayrı hedefler ve RPM geri bildirimi |
| Otomatik fan komutu | R9T köprüsünde mod biti geri okunarak doğrulanır |
| OEM performans / GPU güç limiti / pil şarj sınırı | Uygulanmadı; Linux güç profili bu işlevleri kapsamaz |

R9T'ye özel fan köprüsü, eski evrensel fan tablosu yolunun hedefleri
korumaması sorununu çözer. İlk mod geçişinde fanlar kısa süre tam hıza çıkabilir.
Mod, PWM ve gerçek RPM üzerinden doğrulama yapılır. Kontrol yenilemesi kesilirse
15 saniyelik zaman aşımı ile otomatiğe dönüş denenir. Ayrıntılar ve sınırlar:
[docs/FANS.md](docs/FANS.md).

## Kurulum ve servis

Kaynak klasörü içinde:

```sh
sudo python install-r9t.py
systemctl status slayer-r9t.service
journalctl -u slayer-r9t.service
```

Kurucu bağımlılıkları önce kontrol eder, root sahipli dosyaları hazırlar ve
servisi günceller. Fan köprüsü DKMS ile derlenip kurulur. Qt6Gui ve KF6IdleTime başlıkları kullanılarak kullanıcı oturumu boşta kalma bileşeni derlenir. Yeni servis başlayamazsa önceki dosyalar ve servis tanımı
geri yüklenir. Son önceki kurulum `/usr/local/lib/.slayer-r9t-previous` altında
korunur. Başarılı bir sonraki kurulum bu yedeği yeniler.

Servis yalnız kurulum yapan masaüstü kullanıcısı ve root için Unix soketi açar.
İstek alanları, boyutu, sürümü ve kimliği doğrulanır. İstemci dosya yolu veya
komut çalıştırma yetkisi veremez. EC arayüzü erişilemezse diğer desteklenen
ayarlar kullanılabilir; EC bağlantısı 30 saniyede bir yeniden denenir.

Root servisi açılışta başlar. Kayıtlı profil yalnız kullanıcı açılış kuralını etkinleştirmişse ve KDE oturumu hazırsa uygulanır. Kullanıcı bileşeni `systemctl --user status slayer-r9t-session.service` ile izlenir. Durma sırasında
EC otomatik komutu gönderilir. Beklenmeyen kapanışta systemd yeniden başlatır.
İşlem günlüğü `/var/lib/slayer-r9t/actions.jsonl` altında, 256 KiB boyut sınırı
ve iki dönen yedekle tutulur. Tanılama son 100 işlem kaydını gösterir.

Servisi kapatmak için:

```sh
sudo systemctl disable --now slayer-r9t.service
```

## Geliştirme ve kontroller

Python 3 + PySide6, power-profiles-daemon, NVIDIA sürücüsü ve modele özel
TUXEDO/Uniwill/ITE 8291 sürücü uyarlamaları gerekir. Genel Clevo/Tongfang
modellerine yönelik bir kurulum değildir.

```sh
PYTHONPATH=. python -m unittest discover -s tests -v
# Ayarları kısa süre değiştirir ve başlangıç ayarlarını geri yükler:
QT_QPA_PLATFORM=offscreen R9T_LIVE_CHECK=1 PYTHONPATH=. python tests/live_acceptance.py
```

48 birim kontrolü; bozuk/taşan protokol iletileri, profil doğrulaması,
ayarlardaki atomik yazma, eski profillerin korunması, geri yükleme ve RGB
parlaklık adımları için geçti. Gerçek cihaz kabul kontrolünde birleşik profil,
fan işlemlerinin reddi, arayüz, kalıcı işlem sonucu ve işlem kayıtları
kontrol edildi. Uzun süreli kullanım, uyku/uyanma ve farklı BIOS sürümleri bu
sürümde doğrulanmadı.

Modüller ve tasarım: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Sürücü düzeltmeleri

`tuxedo-drivers` 4.24.0 için R9T DMI uyarlaması ve modele özel başlangıç
koruması yerel DKMS kaynağındadır. Model sorgusundan önce donanım kimliği yeniden
sorgulanarak henüz oluşmamış Uniwill özellik verisine erişim önlenir.

`ite8291-kernel72.patch`, kernel 7.2 RGB kanal sınırını 255 olarak tanımlar;
genel parlaklık sınırı 50 kalır. Kurulu 7.2.8 ve 7.2.5 modülleri derlenmiştir.
Sürücü paketinin güncellenmesi `/usr/src` altındaki yerel yamaları değiştirebilir;
yeni paket sürümünde uyumluluk tekrar kontrol edilmelidir.

Soğutma profillerinin değerleri ve sınırları: [docs/COOLING.md](docs/COOLING.md).

0.6 kapsamı, kalıcı fan profilleri, otomasyon kuralları ve donanım kanıtı bekleyen özellikler: [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md). Windows ile tam özellik eşitliği henüz sağlanmadı.
