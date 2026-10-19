# r9t_fan 0.2.0 (DKMS)

Eski sistemdeki `outputs/fan-driver/r9t_fan.c`'nin sertleştirilmiş sürümü. 0.1.0'a göre değişenler:
- `automatic()` mod biti 0x0751 bit 6'nın gerçekten temizlendiğini geri okuyarak doğrular. Başarısızsa `auto_pending` kalır ve worker 1 sn'de bir yeniden dener; manuel PWM yazımı devam etmez.
- Uyku: `PM_SUSPEND_PREPARE`'da önce `suspended=true`, sonra worker iptali ve auto. Uyku sırasında `control` yazımı `-EBUSY` döner. Auto başarısızsa uyku `NOTIFY_BAD` ile reddedilir.
- Modül boşaltılırken auto en çok 3 kez denenir.
- `MODULE_VERSION("0.2.0")`.

`status` satırı v1 ile bayt bayt aynıdır. Kurulum: `packaging/arch/slayer-r9t-fan-dkms` paketi (DKMS her kernelde derler). Hata simülasyonu: gerçek `r9t_fan.c`, `test/stubs.h` içindeki bellek içi EC ve iş kuyruğuyla derlenir, `test/harness.c` senaryoları çalışır: `cargo test -p r9t-hal --test r9t_fan_c_faults` (modül yüklenmez, donanıma dokunulmaz).
