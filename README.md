# Kontrol Merkezi Linux

GameGaraj Slayer R9T için Linux/KDE kontrol merkezi. Mevcut sürüm **0.10.0**.

Güç/boost/EPP ve frekans profilleri, modele özel fan köprüsü ve fan eğrileri, yazılımla sıcaklık hedefi, RGB düzenleri, pil/ekran/cihaz kontrolleri, profil otomasyonu, ölçüm geçmişi, SSD/RAM sensörleri, GPU açık-kalma tanılaması ve Dynamic Boost durum paneli içerir.

## Kaynak ve belgeler

- [Uygulama ve kurulum](outputs/README.md)
- [Uygulanan özellikler ve doğrulama sınırları](outputs/docs/IMPLEMENTATION_STATUS.md)
- [Sıradaki özellikler](outputs/docs/NEXT_FEATURES.md)
- [Mimari](outputs/docs/ARCHITECTURE.md)
- [Yerel TUXEDO sürücü yamaları](outputs/driver-patches/README.md)

Kaynak kod `outputs/slayer_r9t/`, testler `outputs/tests/`, fan köprüsü `outputs/fan-driver/` altındadır. `outputs/docs/` canlı kabul kayıtlarını, `outputs/previews/` arayüz görüntülerini içerir. Bu dosyalar belirli cihazdaki ölçümlerdir; başka BIOS/cihaz desteği kanıtı değildir.

## Geliştirme kontrolü

Python 3 ve PySide6 kurulu ortamda:

```sh
cd outputs
PYTHONPATH=. QT_QPA_PLATFORM=offscreen python -m unittest discover -s tests -q
```

3 Ekim 2026 teslim kontrolü: **112 test geçti**. Canlı kontroller ayrı `live_*.py` dosyalarıdır; bazıları kısa süre donanım ayarlarını değiştirir. İlgili testin açıklamasını ve koruma koşullarını inceleyerek çalıştırın.

## Cihaz kapsamı

Bu depo genel Clevo/Tongfang kurucusu değildir. R9T için uyarlanmış TUXEDO/ITE sürücüleri ve DKMS araçları gerekir. Yerel sürücü değişiklikleri yamalar halinde korunur; yeni makinede temiz kurulum henüz doğrulanmadı.

GPU watt yazımı, MUX, OEM performans modları, pil şarj eşiği ve voltaj ayarı doğrulanmadığı için açılmadı. Sıcaklık hedefi kesin sıcaklık garantisi vermez. Dynamic Boost desteği ve çalışan daemon, yük altında güç aktarımı kanıtı değildir. Ayrıntılı sınırlar durum belgesindedir.

`work/` altındaki OEM paketleri/decompile araştırması, indirilmiş araçlar, derleme çıktıları ve eski sürüm paketleri Git'e dahil edilmez; yerel çalışma klasöründe korunur. İlk Git kaydı mevcut 0.10.0 durumunu tek commit olarak içerir; geçmiş geliştirme adımları sürüm notlarında anlatılır.
