# İleride

Fazların kapsamı dışında kalan fikirler. Bir faza alınmadan önce plana (`04_ana_plan.md`) işlenir.

## Faz 5 — Paketleme girdileri

- **Kurulum ön kontrolü** (kaynak: snapshot 2fc5d4b `outputs/deployment.py`): kalıcı değişiklikten önce bütün hataları toplayan `--check [--json]`. DMI modeli, araçlar, iki systemd yöneticisi, servis düzeni (masked/linked reddi) ve kernel başlıkları denetlenir; EC'ye erişilmez.
- **DKMS arşivli geri alma** (kaynak: 2fc5d4b `outputs/driver_install.py`): güncellemeden önce kayıtlı bütün kernel/mimari çiftleri `dkms mktarball` ile arşivlenir; hata halinde `ldtarball` ile yeniden derlemeden geri yüklenir. İkincil kernelde hata da geri alma nedenidir. Kayıt uyarısı, `broken` durum ya da başka sürüm varsa değişiklik başlamaz.
- **Kaldırma** (kaynak: 2fc5d4b `outputs/uninstall-r9t.py` ve codex `uninstall_plan.py`): yol bileşenlerinde symlink reddi (no-follow dizin tanımlayıcıları), bekleyen CPU/GPU kurtarma kaydı ya da doğrulanamayan fan auto dönüşü varsa silme yok, kişisel veriler yalnız `--purge` ile.
