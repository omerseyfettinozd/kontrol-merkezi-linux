# Kontrol Merkezi Linux — açık iş kuyruğu

Güncelleme: 3 Ekim 2026. Bu liste tekrar eden geliştirme oturumlarının devam noktasıdır. Uygulanan bir özellik ile canlı cihazda kabul edilmiş davranış ayrı durumlar olarak izlenir. Sürümün ayrıntılı kanıtları `IMPLEMENTATION_STATUS.md`, özellik adayları `NEXT_FEATURES.md` içindedir.

## Tamamlanan kaynak işleri

| Kimlik | İş | Kaynak doğrulaması / kalan sınır |
| --- | --- | --- |
| KML-01 | Fan otomatiğine dönüş hatasında tekrar deneme | `580f538`: gerçek C hata simülasyonu ve yerel kernel nesne derlemesi geçti; yeni kodun fiziksel kabulü bekliyor. |
| KML-02 | Oyun sonrası GPU geri dönüşü ve etkin profil güncellemesi | `580f538`: otomasyon regresyonları geçti; bağımsız GPU sınır getter'ı yok. |
| KML-03 | Kurulumun kullanıcı-servisi/masaüstü adımlarını geri almak | `580f538`: 11 mock kurulum testi geçti; DKMS/modül sürüm geri alması kapsam dışı. |

## Son tamamlanan dilim

| Kimlik | İş | Durum / kabul şartı |
| --- | --- | --- |
| KML-04 | Kayıtlı profili düzenleme ve kopyalama | Uygulandı: 14 Qt offscreen testi; taslak, kaydet/iptal, alan roundtrip ve hata halinde önceki kaydın korunması. Fiziksel cihaz kabulü ayrı. |
| KML-05 | Uygulama otomasyon kuralını düzenleme ve öncelik sırası | Uygulandı: 9 kural testi + GUI kontrolleri; aynı yürütülebilir yol güncellemesi yerini korur, ilk eşleşme önceliği kayıt ve çalışma yolunda doğrulandı. |
| KML-06 | Headless GitHub CI | Uygulandı: Python/Qt regresyonu ve gerçek fan C hata simülasyonu; bütünleşik yerel 150 test + ayrı C simülasyonu geçti. Uzak koşu için GitHub Actions'ta ilgili teslim SHA'sını kontrol et; yerel geçiş uzak koşu kanıtı değildir. |

## Sonraki işler

| Öncelik / kimlik | İş | Durum / gerekli kanıt |
| --- | --- | --- |
| 1 / KML-07 | Yeni kodun fiziksel cihaz kabulü | Cihaz oturumu gerekiyor: uyku/uyanma, gerçek priz/pil geçişi, servis kapanması/yeniden başlatma ve normal yükte geri dönüş. Donanım ayarlarını değiştiren testler otomatik gözetimsiz çalıştırılmaz. |
| 2 / KML-08 | Temiz kurulum/kaldırma ve sürücü güncellemesi | Ayrı uygun R9T ortamı gerekiyor. Sistem Python/Qt/KF6/TUXEDO/DKMS ön koşulları, hata temizliği ve kaldırma sınırları doğrulanmalı. |
| 3 / KML-09 | Görsel statik RGB düzenleyici | Yazılım dilimi olarak hazır: mevcut 6×21/126 kanalı ve bölgeleri boyama, önizleme, profil kaydı/iptal. Fiziksel tuş adları kanıt olmadan eklenmez. Donanım animasyonları ayrı araştırma. |
| 4 / KML-10 | Kaynak kullanan uygulamalar paneli | Salt okunur CPU/RAM/GPU süreç gösterimi; yetki/tarama sınırları, kişisel komut satırı toplamama ve örnek tazeliği. Otomatik işlem sonlandırma kapsam dışı. |
| 5 / KML-11 | Profil karşılaştırma | Kullanıcının başlattığı aynı yükte önce/sonra sıcaklık/RPM/güç; değişen iş yükü ve eksik ölçüm görünür olmalı. Ölçülmeyen FPS sonucu sunulmaz. |
| 6 / KML-12 | Ses/mikrofon ve uyku-pil tanılaması | Ayrı küçük dilimler: PipeWire/WirePlumber geri okuması; fiziksel uyku enerji karşılaştırması için cihaz kanıtı. |

## Donanım araştırması ve kararlar

- Pil şarj eşiği, GPU watt/TGP yazımı, MUX, OEM performans modu, voltaj ve donanım RGB animasyonu **model/firmware kanıtı bekliyor**. Mevcut adres veya enum adları bu işleri uygulanabilir hale getirmez; kontroller kapalı kalır.
- Kontrollü yükte Dynamic Boost güç paylaşımı ve sıcaklık hedefinin FPS/ısı etkisi ölçülmedi. Çalışan daemon, ölçülen tüketim veya kaynak testi fiziksel davranış kanıtı değildir.
- Tüm uygulama için lisans seçimi ayrı karar işidir; mevcut TUXEDO ve fan köprüsü lisans bildirimleri korunur. Public görünürlük tek başına uygulamanın yeniden kullanım lisansı değildir.

## Yeni ajan için devam yöntemi

1. `git status --short --branch` ve son commit'i doğrula; `IMPLEMENTATION_STATUS.md` ile bu kuyruğu oku. Kullanıcının açık yönlendirmesi ve dosya sahiplikleri önceliklidir.
2. Devam eden dilimi tamamla. Bağımsız dosyalarda işler varsa agentlara paralel böl; aynı dosyada eşzamanlı edit yapma.
3. Her sonuçta ilgili regresyonu ve `git diff --check` çalıştır; son bütünleşik kontrolde Python/Qt suite ile `fan-driver/test_fallback.py` çalıştır. GitHub koşusu, yerel test ve fiziksel kabulü ayrı raporla.
4. Cihaz/başlık/CI erişimi eksikse gerçek engeli kaydet ve sıradaki bağımsız yazılım işini seç. Henüz denenmemiş işi başarısız veya tamamlanmış sayma. Fiziksel ayar değiştiren test ve gerçek kurulum için mevcut kullanıcı yetkisinin kapsamını kontrol et.
5. Kuyruk durumunu, geçerli test sayısını ve somut devam noktasını güncelle. Commit/push için verilen görev kapsamını izle; ilgisiz değişiklikleri teslim commit'ine katma.
