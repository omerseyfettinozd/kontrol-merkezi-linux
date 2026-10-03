# Dynamic Boost durumu — 0.10.0

Firmware desteği procfs Notebook Dynamic Boost alanından, daemon sağlığı systemctl show ile ayrı okunur. Destek, servis açılış ayarı ve servis çalışması farklı bilgilerdir. Bunlar aktif güç aktarımı kanıtı değildir; arayüz davranışı doğrulanmadı olarak gösterir. Sayfa servis veya donanım ayarı değiştirmez.

CPU paket gücü, root yetkisiyle package-* RAPL energy_uj farkının geçen monotonic süreye bölünmesidir. Paket ölçümü core alt bölgesiyle toplanmaz; amdgpu PPT CPU paket gücü kabul edilmez. İlk örnek, erişim sorunu, sayaç tutarsızlığı veya eski ölçüm boş bırakılır. Örnek aralığı arayüzde gösterilir. NVIDIA tüketimi nvidia-smi sürücü ölçümüdür; uygulanan güç tavanı tüketim değildir. İki farklı örnekleme yolu tam eşzamanlı değildir. CPU paket + NVIDIA watt toplamı tüm sistem wattı değildir.

Kullanıcı kendi oyununu veya uygulamasını çalıştırarak son 5 dakika CPU/GPU güç grafiğini izleyebilir; Ölçüm geçmişi sekmesinde CPU paket gücü ve GPU tüketimi CSV alınabilir. Kontrollü A/B yük ölçümü ve bağımsız Dynamic Boost etkinlik sinyali olmadan grafik değişimi güç aktarımının kanıtı sayılmaz. Firmware koşulları, diğer güç yöneticileri ve sıcaklık hedefi davranışı etkileyebilir; sıcaklık hedefi etkinse sayfa bunu belirtir.

112 birim testi: enerji birimi, taşma, sıfırlama, eski örnek, izin engeli, alt bölge/alias/multi-package, daemon başarısızlığı/zaman aşımı, destek durumu ve kısmi/NaN GPU verisi. Kurulu 0.10.0 okuyucusunda 5 canlı CPU paket / NVIDIA güç örneği, firmware desteği, çalışan daemon, GUI alt süreçleri, CSV, kaynak/kurulum eşitliği ve iki servisin çalıştığı `dynamic-boost-validation.json` içinde başarılı olarak raporlandı; yük altında güç aktarımı doğrulanmış sayılmaz.

Kaynaklar:

- [NVIDIA Dynamic Boost on Linux](https://download.nvidia.com/XFree86/Linux-x86_64/610.43.03/README/dynamicboost.html): daemon, firmware koşulları ve CPUFreq etkileşimi.
- [Linux Power Capping Framework](https://cdn.kernel.org/doc/html/latest/power/powercap/powercap.html): package enerji sayaçları, energy_uj ve max_energy_range_uj birimleri.
- [NVIDIA SMI belgesi](https://docs.nvidia.com/deploy/nvidia-smi/index.html): tüketim ve uygulanan güç tavanı alanları.
