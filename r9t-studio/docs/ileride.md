# İleride

Fazların kapsamı dışında kalan fikirler. Bir faza alınmadan önce plana (`04_ana_plan.md`) işlenir.

## Faz 5 — Paketleme girdileri

- **[yapıldı 2026-10-19: `r9tctl check`]** **Kurulum ön kontrolü** (kaynak: snapshot 2fc5d4b `outputs/deployment.py`): kalıcı değişiklikten önce bütün hataları toplayan `--check [--json]`. DMI modeli, araçlar, iki systemd yöneticisi, servis düzeni (masked/linked reddi) ve kernel başlıkları denetlenir; EC'ye erişilmez.
- **[gereksiz: pacman -U önceki paket + snapper]** **DKMS arşivli geri alma** (kaynak: 2fc5d4b `outputs/driver_install.py`): güncellemeden önce kayıtlı bütün kernel/mimari çiftleri `dkms mktarball` ile arşivlenir; hata halinde `ldtarball` ile yeniden derlemeden geri yüklenir. İkincil kernelde hata da geri alma nedenidir. Kayıt uyarısı, `broken` durum ya da başka sürüm varsa değişiklik başlamaz.
- **[yapıldı 2026-10-19: `r9tctl legacy-remove`]** **Kaldırma** (kaynak: 2fc5d4b `outputs/uninstall-r9t.py` ve codex `uninstall_plan.py`): yol bileşenlerinde symlink reddi (no-follow dizin tanımlayıcıları), bekleyen CPU/GPU kurtarma kaydı ya da doğrulanamayan fan auto dönüşü varsa silme yok, kişisel veriler yalnız `--purge` ile.

## Sonra

- **[yapıldı 2026-10-19]** **r9td'nin %50 tabanı:** protokol %25, r9td modülün `min_duty`'sine kıstırıyor.
