# r9t_fan 0.3.0 (DKMS)

Eski sistemdeki `outputs/fan-driver/r9t_fan.c`'nin sertleştirilmiş sürümü. Paket: `packaging/arch/slayer-r9t-fan-dkms` (DKMS her kernelde derler, `modules-load.d` açılışta yükler).

## 0.3.0 (plan Faz 5 v2)

Paket 0.3.0-3: `/usr/lib/modprobe.d/slayer-r9t-fan.conf` → `min_duty=25` (ölçülmüş taban, `docs/07`). r9td komutları bu tabana yükseltir.

Paket 0.3.0-2: lease işçisi eskimiş `system_wq` yerine `system_dfl_wq` (bağsız) kuyruğunda; 7.2'nin "deprecated workqueue" uyarısı gider. Davranış ve `MODULE_VERSION` aynı.

Varsayılanlarla davranış 0.2.0 ile aynıdır: `status` satırı bayt bayt aynı, `manual` yalnız %50–100.

- **Önbellek:** EC kaynaklı alanlar (sıcaklıklar, RPM, mod baytı, PWM) `cache_ms` (varsayılan 1000, 200–5000) boyunca önbellekten verilir. Tazeleme kontrol mutex'inin dışında, ayrı bir kilitle yapılır; okuyucu lease işçisini ya da `control` yazımını bekletmez. `control`, auto dönüşü ve uyanma önbelleği geçersiz kılar; o sırada süren bir tazeleme saklanmaz (nesil sayacı). Tazeleme hatası eski önbellekle örtülmez, hata döner.
- **`status2`:** v1 alanları ve ardından `sample_age_ms cache_ms allow_low_duty min_duty cpu_floor gpu_floor stalls`.
- **hwmon** (`r9t_fan`, platform aygıtı `r9t_fan`): `fan{1,2}_input` RPM, `temp{1,2}_input` EC °C, `pwm{1,2}` 0–255, etiketler CPU/GPU. Yalnız okunur, önbellekten gelir. Kaydolamazsa modül yine yüklenir.
- **Override histerezisi:** manuel/boost'ta CPU ≥ 95 ya da GPU ≥ 87 °C → %100. Bırakma için CPU < 90 ve GPU < 82 °C olmalı ve son sıcak örnekten bu yana 10 sn geçmeli (0.2.0'da her örnekte yeniden hesaplanıyordu).
- **`min_duty`** (25–50, varsayılan 50): ölçülmüş güvenli taban, spin-up + %5 ve en az %25. Tarama bitene kadar 50 kalır. 50'nin altındaysa taban sıcaklıkla %50'ye yükselir: CPU 60→75 °C, GPU 55→70 °C. Düşüşte 3 °C histerezis uygulanır.
- **`allow_low_duty`** (yalnız test, varsayılan N): `manual` %20'ye kadar iner. Taban CPU 70→80 °C / GPU 65→75 °C arasında %20'den %50'ye yükselir. **Durma algılama:** duty < %50 iken fan iki ardışık işçi turunda 500 rpm'in altındaysa `automatic()` çalışır, `error=-32` (EPIPE) olur ve `stalls` artar.

## 0.2.0

- `automatic()` mod biti 0x0751 bit 6'nın gerçekten temizlendiğini geri okuyarak doğrular. Başarısızsa `auto_pending` kalır ve işçi 1 sn'de bir yeniden dener; manuel PWM yazımı devam etmez.
- Uyku: `PM_SUSPEND_PREPARE`'da önce `suspended=true`, sonra işçi iptali ve auto. Uyku sırasında `control` yazımı `-EBUSY` döner. Auto başarısızsa uyku `NOTIFY_BAD` ile reddedilir.
- Modül boşaltılırken auto en çok 3 kez denenir.

## Test

Gerçek `r9t_fan.c`, `test/stubs.h` içindeki bellek içi EC, iş kuyruğu, hwmon ve platform taklitleriyle derlenir; `test/harness.c` senaryoları çalışır: `cargo test -p r9t-hal --test r9t_fan_c_faults`. Modül yüklenmez, donanıma dokunulmaz. Senaryolar kasıtlı bozulmuş sürücülerle de denendi: bırakma eşiği, önbellek ya da durma algılama bozulunca test başarısız oluyor. Kernel derlemesi (W=1 uyarısız): 7.2.9 (clang) ve lts 6.18.55.
