# R9T donanım araştırması — 2026-10-03

Cihaz: GAME GARAJ SLAYER R9T, BIOS N.1.19GAR06, EC proje kimliği 0x1a,
8945HX + RTX 5070 Laptop, NVIDIA 615.71.09.

## Bu sürümde açılan kontroller

### Statik çok renkli klavye

ITE8291 (048d:600b) sürücüsü 6 × 21 multicolor LED kanalı sunuyor.
Sürücü kayıt sırası satır/sütun olarak kaynakta tanımlı. Sabit dahili klavye
kimliği ve eksiksiz kanal listesi olmadan harita yazılmaz. 126 RGB üçlüsü ve
%0–100 parlaklık kabul edilir; kullanıcı dosya yolu veya HID paketi gönderemez.
Üç hazır düzen, profil saklama, önizleme metni, otomasyon anlık görüntüsü ve
zaman aşımı geri dönüşü eklendi. Her kanalın renk ve parlaklık geri okuması yapılır.

`lighting-validation.json`: üç düzende 126 kanal eşleşti, boşta kapatma/geri açma
haritayı korudu, başlangıçtaki turuncu %100 düzen geri yüklendi. Görsel renk
ölçümü ve fiziksel tuş konumlarını bağımsız etiketleme bu kontrolün kapsamına dahil değil.
HID donanım animasyonları ayrı ve henüz açılmadı.

### FHD/IR kamera

Orijinal pakette `Command/disableWebcam.ps1` ve `enableWebcam.ps1` Windows
PnP aygıtını devre dışı bırakıyor/etkinleştiriyor; kamera için bu yolda EC yazımı yok.
Dahili USB aygıtı `5-1.4`, kimlik `2b7e:c906`, ürün `FHD WebCam`.
Yalnız bu eşleşme üzerinden USB `authorized` alanı yönetilir. Önce açık video
aygıtını kullanan süreçler kontrol edilir. Kullanımda kapatma reddedilir.
FHD ve IR video aygıtlarının kaldırılması/geri gelişi ve authorized geri okuması
birlikte doğrulanır. Bu işletim sistemi aygıt erişimi kontrolüdür; BIOS kilidi değildir.

`camera-validation.json`: kullanımda ret, kapatmada dört video düğümünün
kalkması, açmada dört düğümün dönmesi geçti. İlk açık durum geri yüklendi.

## Şarj sınırı — salt okunur inceleme

Standart BAT0 eşik arayüzü yok. OEM ECSpec tanımları üst alan 0x07b9,
alt alan 0x07d0. Bu cihazda ikisi de 0. Araştırılan platform alanı 0x07c3=254,
ROMID[0]=255, ROMID[1]=32, ROMID[2]=2, 0x087f=0.
Başka 8945HX/5070 Ti modelindeki firmware araştırması 0x07c3/ROMID koşulları
bildiriyor; bunun N.1.19GAR06 için geçerli olduğu kanıtlanmadı. ROMID bölgesi
bütünüyle boş da değil. Ürün kimliği veya platform alanı değiştirilmedi.

0x078e bit3 açık: TUXEDO'nun sabit charging_profile desteği işareti.
0x07a6=0: aday varsayılan high_capacity profili. Bu alan yüzde üst sınırı ile
aynı işlev değil; voltaj/şarj davranışını R9T firmware kanıtı olmadan değiştirmek
ve bunu %60/%80 sınırı diye sunmak doğru olmaz.

Sonraki gerekli kanıt: bu BIOS/EC sürümünün şarj döngüsü veya OEM çalışma
izindeki model seçimi; ardından fiziksel şarj akımı ve eşikte durma ölçümü.

## TGP — kontrollü yazma ve geri dönüş

TUXEDO Control Center kaynak kodu ctgp_offset dosyasını kullanıyor; arayüzde
baz güç + offset olarak hesaplıyor. OEM paketimiz aynı kaydı ConfigurableTGP_VALUE
diye adlandırıyor. Genel TUXEDO hesaplaması tek başına R9T ölçeği sayılmadı.

Başlangıç EC: kontrol=7, offset=0, TPP=255, Dynamic Boost alanı=25.
Offset 0→5→10, her adım sekiz kez NVML okunarak sınandı. EC geri okuması
her adımda eşleşti; NVIDIA enforced limit 24 örnekte 105 W kaldı.
Başlangıç offset=0 geri yüklendi. Tam örnekler `ctgp-validation.json`.
Bu masaüstü kullanımında bir kontrol denemesidir; GPU yükü/benchmark yapılmadı,
yük altında hiçbir etki olmadığı sonucu çıkarılmaz. Watt ayarı açılmadı.
`nvidia-powerd.service` zaten enabled/active; Dynamic Boost daemonu kapatılmadı.

## Diğer bulgular

- 0x0742=0x22: upstream FAN_TURBO_SUPPORTED biti kapalı. Bu, Office/Gaming/Turbo
  için genel EC komutlarını doğrudan R9T OEM güç modu diye açmaya yeterli değil.
- 0x0766=0xa0: upstream USB_CHARGING biti kapalı; kapalı sistem USB şarjı açılmadı.
- DeviceBase64.dll, DoudouBase64 yerel codec DLL'i; ismi model destek tablosu kanıtı değil.
- OEM hizmetinin ilgili yöntem gövdeleri sanallaştırılmış/obfuscate edilmiş.
  Enum adları veya genel örnek profil watt değerleri R9T desteği sayılmadı.

## Kaynaklar

- [GameGaraj R9T 5070 resmi indirme sayfası](https://www.gamegaraj.com/suruculer/laptop/game-garaj-slayer-r9t-5070-amd-ryzen-9-8945hx-rtx5/?id=4324), ControlCenterX 5.62.60.32 yerel çıkarımı.
- [TUXEDO ITE8291 kaynak kodu](https://github.com/tuxedocomputers/tuxedo-drivers/blob/master/ite_8291/ite_8291.c), yerel 4.24.0 sürücüsü.
- [TUXEDO TGP daemon adaptörü](https://github.com/tuxedocomputers/tuxedo-control-center/blob/master/src/service-app/classes/NVIDIAPowerCTRLListener.ts).
- [Linux Uniwill sürücüsü](https://github.com/torvalds/linux/blob/master/drivers/platform/x86/uniwill/uniwill-acpi.c).
- [Başka model üzerinde şarj firmware araştırması](https://github.com/losewayy/uniwill-ec-charge-limit), R9T desteğinin kanıtı değildir.
