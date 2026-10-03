# Fan kontrolü — 0.5.0

## Çalışan yöntem

R9T BIOS `N.1.19GAR06`, EC proje kimliği `0x1a` üzerinde iki ayrı fan hedefi
ve gerçek RPM geri bildirimi çalıştı. Eski TUXEDO evrensel fan tablosu yolu
bu firmware'de hedefleri korumuyordu. Yeni R9T köprüsü, belgelenmiş tam fan
modunu (`0x0751` bit 6) ve sabit CPU/GPU PWM kayıtlarını (`0x1804` / `0x1809`)
kullanır. İlk mod geçişinde firmware kısa süre tam hıza çıkar; köprü hedefleri
saniyede bir yenileyerek geçiş sonrasında korur.

RPM, güncel Linux Uniwill sürücüsündeki adres ve byte sırasıyla okunur:
CPU `0x0464–0x0465`, GPU `0x046c–0x046d`, big endian. OEM paketi ikinci fanın
alt byte adresini farklı tanımladığı için Linux kaynağıyla karşılaştırılarak
düzeltildi. Önceki ham EC değeri göstergesi RPM değildi.

## Panel

- EC otomatik fan modu.
- Maksimum soğutma: iki fan tam hız.
- Ayrı CPU/GPU fan hedefleri: %50–100.
- Hazır Dengeli / Serin fan eğrileri; maksimum soğutma profili.
- Dört noktalı kullanıcı eğrisi; CPU/GPU sıcaklıklarından ayrı hedefler.
- Gerçek RPM, seçilen hedef ve doğrulama durumu.

PWM yüzdesi RPM yüzdesi değildir. Eğri noktaları arasında doğrusal geçiş
hesaplanır. Fan eğrisi en geç 90°C’de %100 ile bitmelidir. Kontrol %50 altına
inmez; fan kapatma yoktur. 0.6 sürümünde fan hedefi/eğrisi birleşik profile
kaydedilir ve kullanıcı seçerse açılış/uygulama/priz-pil kurallarıyla uygulanır.
Arayüz kapanınca donanım servisi ve seçilen fan modu çalışır.

## Koruma ve doğrulama

`r9t_fan` modülü yalnız `GAME GARAJ / SLAYER R9T`, EC kimliği `0x1a` üzerinde
yüklenir. Genel EC adresi/değeri yazma API'si yoktur. Sabit fan komutları root
sysfs arayüzünde tutulur; masaüstü kullanıcısı erişimi sınırlı servis üzerinden
verilir. Mod, PWM ve RPM en az iki örnekte kontrol edilir. İlk altı saniye hedef
“doğrulama sürüyor” durumunda kalır. Devam eden ölçümler hedefleri doğrulamazsa
15 saniyelik hata süresinden sonra otomatiğe dönüş denenir.

- Servis köprüye düzenli kontrol yenilemesi gönderir. Yenileme kesilirse
  15 saniyelik lease biter ve kernel işi EC otomatiğine döner. İş planlama ve
  EC erişimi nedeniyle dönüş anı tam 15 saniye olmak zorunda değildir.
- CPU ≥95°C veya GPU ≥87°C olduğunda iki fan %100'e zorlanır.
- Sıcaklık verisi geçersizse veya firmware fan modunu değiştirirse kontrol
  bırakılır ve hata gösterilir.
- Servis/modül kapanışında otomatiğe dönüş komutu gönderilir.
- Uyku/hibernation öncesinde kernel notifier manuel kontrolü bırakır. Uyku
  dönüşünde önceki manuel profil kendiliğinden yeniden uygulanmaz.

Kernel/EC bütünüyle kilitlenirse yazılım zaman aşımı çalışamayabilir; bu katman
firmware'in kendi korumasının yerini tutmaz. RPM geri bildirimi mekanik fan
arızasını tamamen dışlayamaz. Koruma sıcaklıklarına bilerek çıkılmadı; uyku ve
hibernation geçişleri canlı olarak denenmedi.

## Cihazda ölçülen örnekler

| Hedef | CPU RPM | GPU RPM |
| --- | ---: | ---: |
| CPU %80 / GPU %70 | yaklaşık 4750 | yaklaşık 4230 |
| CPU %60 / GPU %90 | yaklaşık 3730 | yaklaşık 5110 |
| Maksimum soğutma | yaklaşık 5700 | yaklaşık 5550 |

Değerler bu cihazdaki kısa kontrol oturumuna aittir; her yükte aynı RPM veya
sıcaklık düşüşü garantisi değildir. Servis kabul raporu
[fan-validation.json](fan-validation.json) dosyasında.

Manuel hedefler, Serin eğrisi ve eğriden maksimum soğutmaya geçiş servis
üzerinden doğrulandı. Kontrol yenilemesi kesildiğinde köprünün EC otomatiğine
dönüşü canlı ölçüldü. Fan panelindeki gerçek düğmeye tıklama, etkin hedefler
ve son doğrulama mesajı ayrıca sınandı. Üç kurulu kernel için DKMS derlemesi
başarılı; çalışma ölçümleri yalnız mevcut `7.2.8-2-cachyos` kernelinde yapıldı.

## DKMS

Kaynak `fan-driver/` içindedir. `install-fan-driver.py`, root sahipli kaynakları
`/usr/src/slayer-r9t-fan-0.1.0` altına kopyalar. TUXEDO sürücüsünün ilgili
kernel için `Module.symvers` dosyasını kullanır; kernelin GCC/Clang seçimine göre
derler. TUXEDO sürücüsü olmadan modül derlenemez. Yeni kernel veya TUXEDO paket
sürümünde derleme/çalışma tekrar doğrulanmalıdır.

## Araştırma kaynakları

- [GameGaraj R9T üretici kontrol merkezi](https://www.gamegaraj.com/suruculer/laptop/game-garaj-slayer-r9t-5070-amd-ryzen-9-8945hx-rtx5/?id=4324): paket 5.62.60.32; EC sabitleri salt okunur olarak incelendi, Windows kodu çalıştırılmadı.
- [Linux Uniwill kaynak kodu](https://github.com/torvalds/linux/blob/master/drivers/platform/x86/uniwill/uniwill-acpi.c): sıcaklık, PWM ve RPM adresleri/byte sırası.
- Yerel `tuxedo-drivers` 4.24.0: `set_full_fan_mode`, `direct_fan_control`, `uw_set_fan_auto`.
