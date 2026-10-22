# İleride

Fazların kapsamı dışında kalan fikirler. Bir faza alınmadan önce plana (`04_ana_plan.md`) işlenir.

## Faz 5 — Paketleme girdileri

- **[yapıldı 2026-10-19: `r9tctl check`]** **Kurulum ön kontrolü** (kaynak: snapshot 2fc5d4b `outputs/deployment.py`): kalıcı değişiklikten önce bütün hataları toplayan `--check [--json]`. DMI modeli, araçlar, iki systemd yöneticisi, servis düzeni (masked/linked reddi) ve kernel başlıkları denetlenir; EC'ye erişilmez.
- **[gereksiz: pacman -U önceki paket + snapper]** **DKMS arşivli geri alma** (kaynak: 2fc5d4b `outputs/driver_install.py`): güncellemeden önce kayıtlı bütün kernel/mimari çiftleri `dkms mktarball` ile arşivlenir; hata halinde `ldtarball` ile yeniden derlemeden geri yüklenir. İkincil kernelde hata da geri alma nedenidir. Kayıt uyarısı, `broken` durum ya da başka sürüm varsa değişiklik başlamaz.
- **[yapıldı 2026-10-19: `r9tctl legacy-remove`]** **Kaldırma** (kaynak: 2fc5d4b `outputs/uninstall-r9t.py` ve codex `uninstall_plan.py`): yol bileşenlerinde symlink reddi (no-follow dizin tanımlayıcıları), bekleyen CPU/GPU kurtarma kaydı ya da doğrulanamayan fan auto dönüşü varsa silme yok, kişisel veriler yalnız `--purge` ile.

## Sonra

- **[yapıldı 2026-10-19]** **r9td'nin %50 tabanı:** protokol %25, r9td modülün `min_duty`'sine kıstırıyor.
- **GPU "undervolt" (V/F ofseti + saat kilidi):** 2026-10-21 salt okunur NVML yoklaması (615.78.08): `GetGpcClkMinMaxVfOffset` −1000…+1000 MHz, `GetClockOffsets` grafik 0 (−1000…+1000), bellek 0 (−2000…+6000). Pozitif çekirdek ofseti + `SetGpuLockedClocks` tavanı = aynı MHz'i daha düşük gerilimle sürmek (LACT yöntemi). Yazma CAP_SYS_ADMIN ister (r9td'de var). Kabul için kararlılık + FPS + sıcaklık ölçümü gerekir; ofset uyku/yeniden başlatmada sıfırlanır mı ölçülmeli.
- **CPU undervolt (Curve Optimizer):** 8940HX (family 25, model 97) için `ryzenadj` güç tablosunu açamıyor (`docs/04` §PPT). Tek yol üçüncü taraf `ryzen_smu` modülüyle SMU posta kutusu; doğrulanmamış adrese yazma sayılır (değişmez 9), şimdilik kapsam dışı. Etkili yerine geçen: MHz tavanı (V/F eğrisinde gerilim de iner, `sweet_spot`).
- **systemd sıralama döngüsü:** `r9td.service` `After=power-profiles-daemon.service` + `WantedBy=multi-user.target`, Arch'ın ppd birimi ise `After=multi-user.target`. Kapanışta döngü oluşuyor (2026-10-21 18:25 ve 19:22 journal), systemd bir işi silerek kırıyor. Çözüm adayı: `After=`'dan ppd'yi çıkarmak (ppd D-Bus ile etkinleşir); kapanışta ppd geri yüklemesinin sonucu ölçülmeli.
