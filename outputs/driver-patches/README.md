# R9T yerel sürücü değişiklikleri

`tuxedo-drivers-4.24.0-r9t.patch`, resmi TUXEDO v4.24.0 `src/` dosyaları ile bu cihazın `/usr/src/tuxedo-drivers-4.24.0` kurulu kaynakları arasındaki farktır. R9T DMI/başlangıç uyarlamasını ve kernel 7.2 RGB yoğunluk değişikliğini korur. Resmi kaynak kopyasına `patch --dry-run -p1` kontrolü geçti; yeni makinede derleme/kurulum doğrulaması yapılmadı.

Patch upstream depo kökünden `patch -p1` ile uygulanır. DKMS paketinin kaynakları `src/` katmanını kaldırarak kurması halinde paketleme yolları buna göre uyarlanmalıdır. Kurulum betiği bu upstream yamayı otomatik uygulamaz; uyarlanmış TUXEDO sürücüsü fan köprüsünün ön koşuludur.

Kaynak: https://github.com/tuxedocomputers/tuxedo-drivers/tree/v4.24.0 . Upstream lisansı `TUXEDO-LICENSE` içinde korunmuştur. Önceki `../ite8291-kernel72.patch` RGB değişikliğinin ayrı kaydıdır; aynı değişiklik iki kez uygulanmaz.
