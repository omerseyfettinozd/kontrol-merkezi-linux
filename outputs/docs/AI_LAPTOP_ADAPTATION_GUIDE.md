# Başka laptop için Linux kontrol merkezi geliştirme rehberi

Bu dosyayı doğrudan bir yapay zekâya verebilirsiniz. Amacı, **kendi laptopunuzun Linux arayüzlerini araştırıp o modele uygun bir kontrol merkezi geliştirmek** için başlangıç sağlamaktır. Donanım sürücüsü veya bütün laptoplarda çalışacak kurulum tarifi değildir. Araştırma yöntemi taşınabilir; R9T donanım komutları taşınabilir kabul edilmez.

Referans proje: GameGaraj Slayer R9T Kontrol Merkezi 0.10.0. Araştırma ve kaynak bağlantıları 3 Ekim 2026 tarihinde kontrol edildi. Bağlantılarda `master`/`latest` zamanla değişir; yeni çalışmada kullanılan commit, sürüm ve erişim tarihi kaydedilmelidir.

## 1. Referans cihaz ve kanıtın sınırı

Projede ölçüm yapılan cihaz GAME GARAJ SLAYER R9T; Ryzen 9 8945HX, RTX 5070 Laptop GPU, BIOS N.1.19GAR06 ve EC proje kimliği 0x1a. Çalışma ölçümleri kernel 7.2.8-2-cachyos üzerinde yapıldı. Arayüz Python/PySide6, masaüstü entegrasyonu KDE/Wayland, donanım hizmeti systemd tabanlıdır. Fan ve klavye için TUXEDO 4.24.0/Uniwill/ITE8291 uyarlamaları kullanıldı.

Güç profili, CPU boost/EPP/frekans sınırları, RGB ve belirli fan işlemleri cihazdan geri okumayla kontrol edildi. GPU frekans komutu kabul ediliyor ancak ayarlanan sınırın bağımsız geri okuması yok. Pil şarj eşiği, MUX, OEM performans modları, GPU watt yazımı ve undervolt doğrulanmadığı için açılmadı. Dynamic Boost destek bilgisi ve çalışan `nvidia-powerd`, yük altında gerçek güç aktarımı kanıtı sayılmadı.

Aynı kasa, aynı CPU/GPU, aynı USB kimliği veya aynı marka **aynı EC protokolü anlamına gelmez**. BIOS/EC sürümü, anakart ve üretici uyarlamaları fark yaratır. Bu cihazın kabul raporları başka cihaz desteğinin kanıtı değildir.

## 2. Bu projede gerçekten hangi kaynaklardan yararlanıldı?

Aşağıdaki kullanım ilişkileri depo içindeki araştırma belgeleri ve uygulama dosyalarıyla izlenebilir. Bir kaynağın incelenmiş olması bütün özelliklerinin uygulanmış veya kaynak kodunun kopyalanmış olduğu anlamına gelmez.

| Kullanılan kaynak | Sağladığı bilgi / bu projede karşılığı | Depodaki kanıt ve ilgili uygulama |
| --- | --- | --- |
| [GameGaraj R9T resmi sürücü/Control Center sayfası](https://www.gamegaraj.com/suruculer/laptop/game-garaj-slayer-r9t-5070-amd-ryzen-9-8945hx-rtx5/?id=4324) | ControlCenterX 5.62.60.32 paketindeki EC tanımları, profil JSON'ları ve kamera PowerShell betikleri yerel olarak incelendi. Kamera denetiminin Windows PnP üzerinden yapıldığı görüldü. OEM enum/alan adları tek başına Linux desteği sayılmadı. | `HARDWARE_RESEARCH.md`, `FANS.md`, `IMPLEMENTATION_STATUS.md`; `slayer_r9t/features.py`, `fan-driver/r9t_fan.c`. OEM paketleri ve çıkarılmış/decompile edilmiş dosyalar depoya dahil değildir. |
| [TUXEDO tuxedo-drivers v4.24.0](https://github.com/tuxedocomputers/tuxedo-drivers/tree/v4.24.0), [Uniwill tanımları](https://raw.githubusercontent.com/tuxedocomputers/tuxedo-drivers/v4.24.0/src/uniwill_keyboard.h) | Uniwill WMI/EC erişimi, fan otomatik/tam modları ve destek bitleri. Genel fan tablosu R9T'de hedefleri korumadığı için sınırlı modele özel köprü geliştirildi. DMI/başlangıç uyarlaması yama halinde tutuldu. | `FANS.md`, `IMPLEMENTATION_STATUS.md`; `slayer_r9t/driver_io.py`, `fan-driver/r9t_fan.c`, `driver-patches/tuxedo-drivers-4.24.0-r9t.patch`. |
| [TUXEDO ITE8291 v4.24.0](https://raw.githubusercontent.com/tuxedocomputers/tuxedo-drivers/v4.24.0/src/ite_8291/ite_8291.c) | LED kanal sırası, 6×21 statik RGB düzeni ve parlaklık ölçeği. 126 kanal geri okundu; fiziksel tuş konumları bağımsız etiketlenmedi. HID animasyonlarının kaynakta bulunması bu cihazda destek kanıtı kabul edilmedi. | `HARDWARE_RESEARCH.md`; `slayer_r9t/lighting.py`, `hardware.py`, `ite8291-kernel72.patch`. |
| [Linux upstream Uniwill sürücüsü](https://github.com/torvalds/linux/blob/master/drivers/platform/x86/uniwill/uniwill-acpi.c) | Fan sıcaklık/PWM/RPM tanımları ve byte sırası. OEM paketindeki ikinci fan RPM tanımı Linux kaynağıyla karşılaştırıldı. | `FANS.md`; `fan-driver/r9t_fan.c`. |
| [TUXEDO NVIDIA güç sürücüsü](https://raw.githubusercontent.com/tuxedocomputers/tuxedo-drivers/v4.24.0/src/tuxedo_nb02_nvidia_power_ctrl/tuxedo_nb02_nvidia_power_ctrl.c), [TUXEDO Control Center adaptörü](https://github.com/tuxedocomputers/tuxedo-control-center/blob/master/src/service-app/classes/NVIDIAPowerCTRLListener.ts) | `ctgp_offset` ve baz güç/offset ayrımı araştırıldı. EC geri okuması ile gerçek NVIDIA güç etkisinin farklı kanıtlar olduğu görüldü. R9T watt kontrolü açılmadı. | `HARDWARE_RESEARCH.md`, `ctgp-validation.json`; `slayer_r9t/features.py`. |
| [Linux AMD P-State](https://www.kernel.org/doc/html/latest/admin-guide/pm/amd-pstate.html) | CPU politikaları, boost, EPP ve frekans sınırı araştırması. Tüm CPU politikalarından doğrulama yapılır. | `COOLING.md`; `slayer_r9t/cooling.py`, `hardware.py`. |
| [NVIDIA SMI belgesi](https://docs.nvidia.com/deploy/nvidia-smi/index.html), [Dynamic Boost](https://download.nvidia.com/XFree86/Linux-x86_64/610.43.03/README/dynamicboost.html), [Runtime D3](https://download.nvidia.com/XFree86/Linux-x86_64/610.43.03/README/dynamicpowermanagement.html) | GPU ölçümleri/frekans işlemleri, daemon/firmware desteği ve GPU'nun etkin kalma nedenleri. Kullanım, tüketim, uygulanan güç tavanı ve destek bildirimi ayrı gösterilir. Kendi NVIDIA sürümünüzün belgeleriyle yeniden karşılaştırın. | `COOLING.md`, `DYNAMIC_BOOST.md`, `GPU_DIAGNOSTICS.md`; `slayer_r9t/cooling.py`, `dynamic_boost.py`, `gpu_diagnostics.py`. |
| [Linux Power Capping Framework](https://docs.kernel.org/power/powercap/powercap.html) | Paket enerji sayaçları ve µJ farkından watt hesaplama. Paket/core bölgelerini iki kez toplamama, taşma ve eskimiş örnek kontrolü. | `DYNAMIC_BOOST.md`; `slayer_r9t/power_monitor.py`. |
| [KDE KScreen kaynak kodu](https://github.com/KDE/libkscreen/blob/master/src/doctor/doctor.cpp), [KF6 KIdleTime](https://api.kde.org/kidletime-index.html), kurulu KDE D-Bus arayüzleri | Ekran modları, masaüstü ayarları ve kullanıcı oturumu boşta kalma bilgisi. Root donanım hizmetinden ayrı kullanıcı hizmeti. | `IMPLEMENTATION_STATUS.md`, `ARCHITECTURE.md`; `slayer_r9t/desktop.py`, `session.py`, `session-idle.cpp`. |
| [RyzenAdj destek araştırması](https://github.com/FlyGoat/RyzenAdj/wiki/Supported-Models) | Yerel 0.19.0 denemesinde güç tablosu başlatılamadı. CPU ailesinin tanınması kullanılabilir güç/voltaj kontrolü kanıtı sayılmadı. | `COOLING.md`; ilgili CPU watt/undervolt kontrolleri kapalı. |

### Kod kopyalama ve lisans

`fan-driver/r9t_fan.c` başlığında `GPL-2.0` belirtilir. TUXEDO v4.24.0 kaynaklarının lisansı `GPL-2.0+` olarak korunur: [upstream lisans](https://raw.githubusercontent.com/tuxedocomputers/tuxedo-drivers/v4.24.0/LICENSE), depoda `driver-patches/TUXEDO-LICENSE`. Bu lisanslar kendiliğinden bütün uygulamaya verilmiş bir lisans anlamına gelmez. Bu rehber hazırlanırken depo kökünde tüm uygulamayı kapsayan `LICENSE` bulunmuyordu; güncel kök lisans ve dosya başlıklarını yeniden inceleyin.

Public görünürlük kaynak kodun her amaçla yeniden kullanım izni değildir. Kopyalama/dağıtım öncesinde uygulama, kernel modülü, yamalar ve bağımlılıkların lisanslarını ayrı kaydedin; telif bildirimlerini koruyun. [GitHub lisans açıklaması](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/licensing-a-repository). OEM Windows paketlerine ait DLL/decompile çıktılarının yeniden dağıtım iznini varsaymayın; araştırma bulgularını ve üretici bağlantısını kaydedin.

## 3. Başka model için önerilen araştırma yolu

Bu bölüm bir öneridir; listelenen her kaynağın R9T geliştirmesinde kullanıldığı iddia edilmez.

1. **Kimlik ve mevcut destek:** Tam model, BIOS, CPU/GPU, kernel, masaüstü ve etkin sürücüler. Marka yerine PCI/USB kimliği, DMI model eşleşmesi ve sürücü yeteneklerini esas alın. Üreticinin tam model sürücü sayfasını ve Linux destek açıklamalarını bulun.
2. **Önce standart salt okunur arayüzler:** `/sys/class/hwmon`, `/sys/class/power_supply`, CPUFreq policy dosyaları, LED/backlight sınıfları ve DRM bağlantıları. Dosyaların bulunması, birimi ve gerçekten hangi donanımı temsil ettiği birlikte kontrol edilir. `hwmon0`, `BAT0`, `card1` gibi numaralar genel olarak sabit değildir.
3. **Mevcut kernel sürücüsü:** İlgili üreticinin `drivers/platform/x86/` sürücüsü, DMI eşleşmeleri, WMI GUID'leri ve kernel belgeleri. [Linux WMI](https://docs.kernel.org/wmi/index.html) araştırmaya giriş sağlar. ACPI tabloları gerekirse yerel olarak incelenir; bilinmeyen ACPI yöntemleri deneme amaçlı çalıştırılmaz. EC dump/scan veya register yazımı başlangıç envanterinin parçası değildir.
4. **CPU:** [CPUFreq](https://docs.kernel.org/admin-guide/pm/cpufreq.html) üzerinden etkin `scaling_driver`, desteklenen governor/EPP ve politika sınırları. AMD P-State yalnız uygun AMD sisteminde; Intel veya başka CPU için kendi sürücü belgesi. Her CPU için aynı watt/boost/undervolt yöntemi varsayılmaz.
5. **Sensör ve pil:** [hwmon sysfs](https://docs.kernel.org/hwmon/sysfs-interface.html) ve [power_supply sınıfı](https://docs.kernel.org/power/power_supply_class.html). Sıcaklık ölçekleri, µV/µA/µW/µWh gibi birimler ve etiketler kontrol edilir. Şarj eşiği ancak sürücünün gerçek eşik arayüzü veya tam modele ait doğrulanmış başka yöntem varsa araştırılır. Şarj profili, yüzde sınırı ve batarya sağlık değeri ayrı kavramlardır.
6. **GPU:** NVIDIA varsa onun NVML/SMI ve kurulu sürüm belgeleri; AMD/Intel GPU için ilgili kernel sürücüsü ve desteklenen kullanıcı araçları. MUX/hibrit grafik geçişi ayrı bir özellik. GPU yönetim sorguları cihazı uyandırabilir; pil/uyku tanılamasında sorgunun etkisi ölçülür.
7. **Masaüstü:** KDE ise KScreen/KWin/KIdleTime; GNOME veya başka masaüstü için kendi desteklenen arayüzleri. X11 ve Wayland farklıdır. Kullanıcı D-Bus'u root bağlamından taklit edilmez. Wi-Fi/Bluetooth için kurulu NetworkManager/BlueZ/rfkill API'leri araştırılır; ses gerekirse PipeWire/WirePlumber tarafında ele alınır.
8. **OEM'e özgü fan/RGB:** Ancak üretici ve anakart ailesi eşleşiyorsa TUXEDO/Uniwill/Clevo gibi ilgili kaynakları inceleyin. Başka modelin topluluk projesi bir araştırma ipucudur; kendi modelinizin desteği veya register anlamı için yeterli kanıt değildir.

Her bulgu için şu kayıt tutulmalıdır:

| İşlev | Model/firmware | Arayüz ve birim | Kaynak/sürüm | Salt okunur kanıt | Yazma ve geri okuma | Fiziksel davranış | Karar |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Örnek: CPU sınırı | Kullanıcının modeli | CPUFreq policy, kHz | Kernel belgesi + sürücü | Mevcut değerler | Henüz denenmedi | Henüz ölçülmedi | Salt okunur |

Kararlar: `yok`, `bilinmiyor`, `salt okunur`, `komut kabul edildi`, `geri okuma doğrulandı`, `davranış doğrulandı`. Eksik kanıtı çalışan özellik gibi sunmayın.

## 4. Paylaşılabilir donanım özeti hazırlama

Aşağıdaki envanter root gerektirmeden seçilmiş model/sürücü alanlarını okur; ayar değiştirmez. İzin verilmeyen değerler `null` kalır. NVIDIA yönetim sorgusu, EC/HID erişimi ve ACPI komutu çalıştırmaz. Çıktı paylaşılmadan önce kullanıcı tarafından gözden geçirilmelidir; özel OEM model dizelerinde beklenmedik kimlik bilgisi olabilir.

```sh
python3 - <<'PY'
import glob, json, os, platform
from pathlib import Path

def read(path):
    try:
        return Path(path).read_text().strip()[:256]
    except (OSError, UnicodeError):
        return None

dmi = '/sys/class/dmi/id/'
data = {
    'kernel': platform.release(),
    'architecture': platform.machine(),
    'desktop': os.environ.get('XDG_CURRENT_DESKTOP'),
    'session_type': os.environ.get('XDG_SESSION_TYPE'),
    'dmi': {key: read(dmi + key) for key in
            ('sys_vendor', 'product_name', 'bios_version')},
    'cpufreq': [], 'hwmon': [], 'power_supply': [],
}
for path in sorted(glob.glob('/sys/devices/system/cpu/cpufreq/policy*')):
    data['cpufreq'].append({key: read(path + '/' + key) for key in
        ('scaling_driver', 'scaling_available_governors',
         'energy_performance_available_preferences',
         'cpuinfo_min_freq', 'cpuinfo_max_freq')})
for path in sorted(glob.glob('/sys/class/hwmon/hwmon*')):
    data['hwmon'].append({'driver_name': read(path + '/name'),
        'fields': sorted(Path(p).name for p in glob.glob(path + '/*_input'))})
for path in sorted(glob.glob('/sys/class/power_supply/*')):
    data['power_supply'].append({'type': read(path + '/type'),
        'charge_start_interface': Path(path, 'charge_control_start_threshold').exists(),
        'charge_end_interface': Path(path, 'charge_control_end_threshold').exists()})
print(json.dumps(data, ensure_ascii=False, indent=2))
PY
lspci -nn
lsusb
```

Dağıtım ve sürümünü, CPU/GPU ürün adlarını, kullanılan masaüstü sürümünü ve ilgili driver sürümlerini ayrıca ekleyin. `lspci`/`lsusb` yoksa sırf envanter için root script kurmak gerekmez; mevcut sistem araçları veya kullanıcı tarafından seçilmiş ürün kimlikleri yeterlidir.

**Paylaşmayın:** seri numarası, DMI UUID, disk/USB/batarya seri numarası, MAC/IP, Wi-Fi SSID, hostname, kullanıcı adı, kişisel dosya yolları, SSH anahtarı/token, süreç komut satırları/ortam değişkenleri, ham EDID veya tam sistem günlükleri. `dmidecode`, `lsusb -v`, `inxi` ayrıntılı raporu, `journalctl` ve ACPI dump çıktıları otomatik olarak public eklenmez. Gerekli bölüm seçilip temizlenir. Örnek/test verilerinde gerçek kullanıcı profili ve uygulama yolları yerine yapay veriler kullanılır.

## 5. AI'nın uygulama tasarımı ve donanım yazma sınırı

- GUI ve çoğu ölçüm kullanıcı yetkisiyle çalışsın. Yetki gerektiren dar işlemler ayrı servis/adaptörde olsun; sabit işlem listesi, alan/tür/değer sınırları ve istemci kimlik kontrolü bulunsun.
- RPC üzerinden serbest shell komutu, dosya yolu, EC adresi/değeri veya HID paketi kabul edilmesin. Model eşleşmesi ve yetenek keşfi başarısızsa ilgili kontrol kapalı olsun.
- R9T'nin EC offsetleri, bit maskeleri, ölçekleri, fan yüzdeleri, USB yolu, termal eşikleri veya DMI allowlist'i başka modele **asla kopyalanarak etkinleştirilmesin**. Bu rehberde register yazma tarifi verilmemesi bilinçlidir. Referans repo ayrıntıları yalnız kaynak analizi içindir.
- Kernel modülü/EC yazımı ancak hedef model ve firmware için anlamı, birimi, geri dönüş yolu ve uyku/arıza davranışı kanıtlanınca ele alınsın. DMI/ürün/ROM kimliğini değiştirerek sürücünün destek kontrolü aşılmasın. Rastgele register taraması/yazımı, firmware flash veya voltaj denemesi yapılmasın.
- Fan işi için geçerli sıcaklık/RPM geri bildirimi, süreli kontrol sahipliği, servis ölümü ve uyku halinde otomatiğe dönüş gerekir. Dönüş komutunun başarısızlığını gizlemeyin. Firmware koruması yazılımla değiştirilemez; bilerek kritik sıcaklığa çıkmak kabul testi değildir.
- Her işlemde başlangıç durumu kaydedilsin, komut kabulü/geri okuma/fiziksel etki ayrı raporlansın. Geri alma hatası görünür olsun. Harici güç yöneticileriyle ayar yarışı araştırılsın; donanım dosyası sürekli yeniden yazılmasın.
- Desteklenmeyen sensör `0` veya uydurma değer yerine bilinmiyor gösterilsin. Enerji/tüketim/güç tavanı, CPU paket gücü/tüm cihaz tüketimi, PWM/RPM ve boost desteği/etkisi birbirine karıştırılmasın.

R9T mimarisinden alınabilecek fikirler: kullanıcı/root ayrımı, sınırlı protokol, atomik ayar kaydı, geri alma, eski ölçümü ayırma, otomasyonun başlangıçta kapalı olması ve her özelliğin kanıt seviyesini gösterme. R9T uygulamasının kendi kabul edilmemiş alanları da bulunduğundan referans kod her koşulda doğru kabul edilmez.

## 6. Beklenen teslimler ve kabul kapıları

1. **Araştırma teslimi:** temizlenmiş envanter, kaynak/sürüm listesi, özellik-destek matrisi, eksik kanıtlar. Donanım yazımı içermez.
2. **Salt okunur prototip:** sensörler ve yetenekler; eksik sensör, izin hatası, kaybolan aygıt, bozuk/eski ölçüm testleri. Yanlış modelde yazma kontrolleri açılmamalı.
3. **Standart kontrollere geçiş:** yalnız doğrulanmış API üzerinden bir özellik; başlangıç kaydı, geri okuma, hata/geri alma ve servis yaşam döngüsü testleri. Mock testi gerçek donanım desteği olarak raporlanmaz.
4. **Model özel köprü:** gerekliyse ayrı sürücü, dar allowlist ve sabit komutlar; kullanılan kernel üzerinde derleme. Derleme geçmesi çalışma kanıtı değildir. Lease/sensör kaybı/iletişim kaybı/geri dönüş hata senaryoları kontrol edilmeli.
5. **Kullanıcının cihaz kabulü:** kısa kontrollü işlem ve başlangıca dönüş; ardından normal kullanım, priz/pil, fiziksel uyku/uyanma, yeniden başlatma, servis/UI kapanması. FPS/ısı/pil iyileşmesi iddiası için aynı yükte önce/sonra ölçümü gerekir.
6. **Dağıtım:** temiz kurulum, hata halinde geri alma, kaldırma, sürücü/kernel güncellemesi, belge tutarlılığı, lisans ve kişisel veri taraması. Sadece denenmiş model/firmware kombinasyonları destek listesine eklenir.

Her kapı `geçti`, `kaldı`, `denenmedi` veya `engelli` olarak kayıt edilir. Yazma işlemi canlı donanım testine geçmeden önce kullanıcı testin etkisini, geri dönüşünü ve koşullarını görmelidir. Denenmeyen kapı başarılı sayılmaz.

## 7. Yapay zekâya doğrudan verilecek görev

Bu rehberle birlikte aşağıdaki metni ve temizlenmiş donanım özetini gönderin. Alanları kendi cihazınıza göre doldurun:

```text
Laptopum için Linux kontrol merkezi geliştirmek istiyorum.
Model: [tam üretici/model]
CPU / GPU: [ürünler]
BIOS / kernel / dağıtım: [sürümler]
Masaüstü ve oturum: [ör. KDE/Wayland]
İstediğim kontroller: [fan, RGB, güç, ekran, pil, ...]
Temizlenmiş donanım özeti: [seçilmiş alanlar]

Ekli rehberi araştırma ve kabul yöntemi olarak kullan.
Önce cihazın mevcut Linux desteğini salt okunur olarak incele; üretici
paketlerini, tam model kernel sürücülerini ve resmi API belgelerini bul.
Her kaynak için URL, sürüm/commit, model ilişkisi ve elde edilen bilgiyi yaz.
R9T offset/bit/termal eşik/USB yollarını cihazıma taşıma ve kimlik kontrollerini
aşma. Bilinmeyen EC/ACPI/HID yazımı, firmware veya voltaj denemesi yapma.

Çıktı: (1) kişisel veri içermeyen donanım özeti, (2) özellik-destek matrisi,
(3) kaynak ve lisans envanteri, (4) önce salt okunur prototip,
(5) her yazma işlemi için model kanıtı/geri okuma/geri alma planı,
(6) kullanıcı GUI + dar yetkili servis mimarisi, (7) birim/hata/cihaz kabul
kontrolleri, (8) kurulum/kaldırma ve destek sınırlarını açıklayan README.
Komut kabulü, ayar geri okuması ve fiziksel davranışı ayrı raporla.
Kanıtlanmayan özelliği kapalı bırak ve eksik kanıtı söyle.
Donanım ayarlarını değiştiren testlerin kapsamını çalıştırmadan önce açıkla.
```

Bu dosya tek başına yöntem ve kaynak başlangıcı sağlar. Referans uygulama ayrıntıları için aynı depodaki `outputs/docs/ARCHITECTURE.md`, `IMPLEMENTATION_STATUS.md`, `FANS.md`, `COOLING.md`, `HARDWARE_RESEARCH.md`, `GPU_DIAGNOSTICS.md` ve `DYNAMIC_BOOST.md` okunabilir. Yeni cihazın verisi ve doğrulaması olmadan AI'nın verdiği destek iddiası yeterli değildir.
