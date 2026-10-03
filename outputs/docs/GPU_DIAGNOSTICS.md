# GPU neden açık? — 0.9.0

Bu görünüm kanıtları birlikte gösterir; kesin tek neden ilan etmez. Bu R9T'de eDP-1, NVIDIA DRM kartına bağlı ve enabled; NVIDIA runtime durumu active. Ekranı sürmek aktif kalma nedenidir. Açık aygıt dosyaları, CUDA veya grafik hesaplamasının gerçekten çalıştığını tek başına kanıtlamaz.

Süreçler kullanıcı yetkisiyle /proc/PID/fd bağlantılarından okunur; GPU aygıtları açılmaz. Başka kullanıcı/root süreçleri erişilemeyebilir. NVIDIA minor information dosyasından, DRM/render aygıtları device bağlantısından eşlenir. nvidiactl/uvm ortak aygıtları ayrıca listelenir. Kaybolan PID ve erişim engeli ayrı raporlanır. Süreç adları düz metindir; komut satırı ve UUID kaydedilmez.

Bu sekmede GUI düzenli NVIDIA ve root durum sorgularını başlatmaz. Root servisinin açık NVML bağlantısı ve etkin sıcaklık hedefi, başka kontrol merkezi pencereleri ve diğer uygulamalar GPU'ya erişmeye devam edebilir. Panelin GPU'yu uyandırmadığı kesin olarak doğrulanmış değildir. Zaten etkin GPU'da active→active sonucu uyandırma etkisini kanıtlayamaz. İki suspended örneği de örnekler arasındaki kısa geçişleri dışlamaz.

98 birim testi: eşleme, eksik veri, yetki reddi, PID değişimi, süre/dosya sınırı, aygıt açmama ve sekme sırasında NVIDIA/servis durum sorgularının başlatılmaması. Kurulu okuyucunun 5 canlı salt okunur örneği, GUI alt süreç bağlantısı, değişmeyen güç politikaları, kaynak/kurulum eşitliği ve iki servisin çalıştığı `gpu-diagnostics-validation.json` içinde başarılı olarak raporlandı. Donanım güç ayarları değiştirilmez; uyku/MUX/FPS veya genel pil tasarrufu garantisi verilmez.

Kaynaklar:

- [NVIDIA Runtime D3](https://download.nvidia.com/XFree86/Linux-x86_64/610.43.03/README/dynamicpowermanagement.html): ekran sürme, diğer PCI işlevleri ve procfs bilgileri.
- [Linux runtime güç sysfs ABI](https://docs.kernel.org/7.1/admin-guide/abi-testing-files.html): runtime_status / control ve süre sayaçları.
- [Linux DRM KMS](https://cdn.kernel.org/doc/html/latest/gpu/drm-kms.html): bağlantı durumu anlamı.
