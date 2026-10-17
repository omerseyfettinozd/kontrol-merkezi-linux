# r9t_fan 0.2.0 (DKMS)

Kaynak: `outputs/fan-driver/r9t_fan.c` (sertleştirilmiş sürüm). 0.1.0'a göre değişenler:
- `automatic()` mod biti 0x0751 bit 6'nın gerçekten temizlendiğini geri okuyarak doğrular. Başarısızsa `auto_pending` kalır ve worker 1 sn'de bir yeniden dener; manuel PWM yazımı devam etmez.
- Uyku: `PM_SUSPEND_PREPARE`'da önce `suspended=true`, sonra worker iptali ve auto. Uyku sırasında `control` yazımı `-EBUSY` döner. Auto başarısızsa uyku `NOTIFY_BAD` ile reddedilir.
- Modül boşaltılırken auto en çok 3 kez denenir.
- `MODULE_VERSION("0.2.0")`.

`status` satırı v1 ile bayt bayt aynıdır. Kurulum `tools/install-fan-0.2.0.sh`, geri alma `tools/rollback-fan-0.1.0.sh`. Hata simülasyonu: `python3 test_fallback.py`.
