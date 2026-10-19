# Faz 5 — Paketleme: ara durum

Tarih: 2026-10-19. Ayrıntılı kurulum/geçiş sırası: `packaging/arch/README.md`. Paketler derlendi, denetlendi ve 04:17'de `tools/migrate-to-packages.sh` ile **kuruldu** (aşağıda).

## Paketler

| Paket | Kaynak | Doğrulama |
|---|---|---|
| `tuxedo-drivers-r9t-dkms` 4.24.0 | GitHub v4.24.0 arşivi (sha256 `890d419b…`) + `driver/tuxedo-drivers/tuxedo-drivers-4.24.0-r9t.patch` + kurulu paketten alınan `dkms.conf` | makepkg geçti. `/usr/src/tuxedo-drivers-4.24.0` kurulu ağaçla `diff -r` boş; dosya listesi `pacman -Ql tuxedo-drivers-dkms` ile aynı (yalnız lisanslar fazla); udev/modprobe dosyaları bayt bayt aynı |
| `slayer-r9t-fan-dkms` 0.2.0 | `driver/r9t_fan` | makepkg geçti; `prepare()` dkms.conf ve `MODULE_VERSION` sürümünü pkgver ile karşılaştırır. Derleme dosyaları kurulu `/usr/src/slayer-r9t-fan-0.2.0` ile aynı (fazladan README) |
| `r9t-studio` 0.1.0.rN | depo çalışma ağacı (izlenen dosyalar) | makepkg geçti (`0.1.0.r63.g1b1d9a3-1`, 6,7 MB). `check()`: 103 test geçti (workspace 98 + UI 5). Birimler ve masaüstü girişinde `/usr/local` kalmadı (`/usr/lib/r9t-studio`, `/usr/share/doc`). Paket ikililerinde `ldd` eksik kütüphane yok; r9td/r9tctl/r9t-session yalnız libc/libgcc_s/libm. Paketteki `r9tctl` çalışan r9td ile konuştu (denetim, kamera true) |

## Bulgular

- **Kurulu tuxedo-drivers elle yamalanmış:** `pacman -Qkk tuxedo-drivers-dkms` → `ite_8291.c` boyut/SHA256 uyuşmazlığı (RGB yaması sonradan elle). DMI yaması ise paketin kendisinde (paketçi "Unknown Packager", kaynağı bulunamadı). Paket güncellenirse ikisi de giderdi: RGB'yi r9td `multi_max_intensity` kontrolüyle kapatırdı, DMI giderse tuxedo modülleri ve onlara bağlı r9t_fan yüklenmeyebilirdi. `tuxedo-drivers-r9t-dkms` bunu kalıcı yapar.
- **Kurulum betiği** commit sonrası gölgeyi yeniden etkinleştiriyordu (docs/14, düzeltildi).
- **Waybar modülü** `on-click: r9t-ui` diye var olmayan bir komut çağırıyordu → `r9t-studio` (paket ve dev kurulumu `/usr/bin|/usr/local/bin/r9t-studio` bağlantısı kurar).
- `r9t-switch` / `r9t-switch-user` yolları kendi konumlarından türetir; birimleri `systemctl cat` ile arar (dev `/etc` ve paket `/usr/lib` ikisinde de çalışır).

## Kurulum (2026-10-19 04:17, kullanıcı `sudo tools/migrate-to-packages.sh`)

- Yedek: `/var/lib/r9t/migrate-backup-20261019-041731.tar` (iki `/usr/src` ağacı). Her pacman işleminde snapper ön/son anlık görüntüsü alındı (453–460).
- `tuxedo-drivers-dkms` → `tuxedo-drivers-r9t-dkms`: DKMS üç kernelde kuruldu (7.2.9, lts 6.18.55, sahipsiz 7.2.5). Yeni modüllerde DMI eşleşmesi var (`tuxedo_keyboard`, `tuxedo_compatibility_check` içinde "GAME GARAJ"), `multi_max_intensity` 255.
- Manuel `slayer-r9t-fan` 0.2.0 DKMS kaydı → `slayer-r9t-fan-dkms`: üç kernelde kuruldu, `r9t_fan` sürüm 0.2.0 (yüklü modül de 0.2.0). Uyarı: dkms 3.4 `CLEAN` alanını eskimiş sayıyor (işlevsel değil).
- Geliştirme kurulumu kaldırıldı → `r9t-studio` paketi; `r9t-switch --commit` yeniden uygulandı. Sonuç: `r9td` active/enabled `/usr/lib/systemd/system/r9td.service`, denetim, oturum `hyprland:`; `r9td-shadow` ve `slayer-r9t` disabled; `r9t-session` `/usr/lib/r9t-studio/r9t-session`'dan çalışıyor (kullanıcı birimi yeniden etkinleştirildi). Arayüz `/usr/bin/r9t-studio` → `/usr/lib/r9t-studio`, tepsi ipucu dolu, menü girişi `/usr/share/applications`.
- `pacman -Qkk`: üç pakette 0 değiştirilmiş dosya. `/usr/local/lib/r9t-studio`, `/usr/local/bin/r9t*`, `/etc/systemd/{system/r9td,user/r9t-session}.service` yok. Kalan (zararsız): `/etc/systemd/system/r9t-shadow-compare.service` (devre dışı, Faz 2 aracı).

## Saf Rust (2026-10-19)

- Python ve kabuk araçları kaldırıldı. `r9t-switch` → `r9tctl switch`, `r9t-switch-user` → `r9tctl desktop` (paket `/usr/bin/r9t-switch*` adlarını `r9tctl`'ye bağlar; `r9tctl` adına göre yönlenir), `keymap-calibrate.py` → `r9tctl keymap-calibrate` (artık r9td `rgb_map` üzerinden; root gerekmez, RGB motoruyla yarışmaz; SIGINT/SIGTERM'de renkler geri yüklenir), `load-test.py` → `r9tctl load-test`. r9t_fan C hata testi `crates/r9t-hal/tests/r9t_fan_c_faults.rs` (C senaryoları `driver/r9t_fan/test/`); kasıtlı bozulan senaryoyu yakaladığı doğrulandı.
- Silinen (git geçmişinde): bitmiş faz araçları (gölge karşılaştırma ve birimi, probe karşılaştırma, fan RPM taraması, yetenek yoklaması, eski durum dökümü, Tauri spike), geliştirme kurulum/geçiş betikleri, elle fan sürücüsü kur/geri al/test betikleri (yerini DKMS paketi aldı), eski Python sistemi `outputs/` (etiket `legacy-0.12.0`).
- Paket: `python` bağımlılığı kalktı; birimler doğrudan `/usr/lib/r9t-studio` yolunu kullanır (sed yok); `r9t-fallback` → `r9tctl switch --fallback`.
- Masaüstü tarafı sandbox'ta (sahte systemctl) commit → commit → rollback: hyprland.conf ve menü girişi bayt bayt geri döndü. Testler: workspace 104 + UI 5, clippy temiz. CI (GitHub Actions) eski Python/Qt işinden Rust workspace + C hata testine geçti.

## İlk açılış (2026-10-19 17:57, paketlerden sonraki ilk yeniden başlatma)

- **Paketten gelenler doğru:** `r9td` `/usr/lib/systemd/system/r9td.service` → `/usr/lib/r9t-studio/r9td`; `r9t-session` `/usr/lib/systemd/user/r9t-session.service`, Hyprland `exec-once` satırıyla başladı. DKMS kaynakları paketlere ait (`/usr/src/tuxedo-drivers-4.24.0` → `tuxedo-drivers-r9t-dkms`, `/usr/src/slayer-r9t-fan-0.2.0` → `slayer-r9t-fan-dkms`); `/lib/modules/*/updates/dkms/*.ko.zst` DKMS çıktısı olduğu için pacman'e ait değil (beklenen). tuxedo modülleri yüklü, kernel 7.2.9.
- **Hata 1, `r9t_fan` yüklenmedi → r9td salt okunur:** günlükte `r9td: kontrol açılmadı, salt okunur: r9t_fan 0.2.0 gerekli, yüklü: None`; `r9tctl status` → `authority.state = refused`. Fanlar EC otomatikte kaldı (güvenli taraf), ama R9T Studio fanlara dokunamadı. Neden: modülü açılışta eski `slayer-r9t.service` yüklüyordu (`ExecStartPre=-/usr/bin/modprobe r9t_fan`). Kalıcı geçişten (`873b2e1`) sonra o servis kapalı ve modülü yükleyen bir şey kalmadı. Hata ilk yeniden başlatmada ortaya çıktı. r9td `ProtectKernelModules=yes` ile çalıştığı için modülü kendi yükleyemez. **Düzeltme:** `slayer-r9t-fan-dkms` 0.2.0-2, `/usr/lib/modules-load.d/slayer-r9t-fan.conf` dosyasını kurar (`r9t_fan`; softdep `uniwill_wmi tuxedo_keyboard`). r9td zaten `After=systemd-modules-load.service`.
- **Hata 2, tepsi simgesi yok:** arayüz (`r9t-studio`) girişte hiç başlatılmıyordu; Hyprland'de `exec-once` satırı, autostart girdisi ya da birim yoktu. Önceki oturumdaki simge, arayüzün elle açılmasından kalmıştı. `--hidden` ve SNI bekleme desteği vardı, bağlantısı eksikti. **Düzeltme:** kullanıcı birimi `r9t-studio-tray.service` (`r9t-studio --hidden`, `WantedBy=r9t-session.service`). Hyprland satırı ortamı içe aktarıp r9t-session'ı başlattığında tepsi de başlar; KDE'de graphical-session.target üzerinden başlar. `r9tctl desktop --commit` etkinleştirir, `--rollback` kapatır, `--status` gösterir. Tepsideki "Çık" temiz çıkıştır ve birim kapalı kalır.
- **Not:** tuxedo modüllerini (`tuxedo_io` dahil) hâlâ eski, sahipsiz `/etc/modules-load.d/slayer-r9t.conf` yüklüyor. Eski sistem kaldırılırken bu dosya silinmeden önce içeriği bir pakete taşınmalı (r9td `/dev/tuxedo_io` kullanır).
- Kalan eski kayıt: `dkms status` → `slayer-r9t-fan/0.1.0` üç kernelde `built` (kurulu değil). Kaynak `/usr/src/slayer-r9t-fan-0.1.0` sahipsiz; derlenmiş kopyalar yalnız `/var/lib/dkms/slayer-r9t-fan/0.1.0/` altında. Yüklü `r9t_fan` 0.2.0'a ait; `kernel-*` bağlantıları `0.2.0`'ı gösteriyor. Temizleme (yalnız 0.1.0'ın DKMS ağacını ve kaynağını siler):

  ```bash
  sudo dkms remove slayer-r9t-fan/0.1.0 --all
  sudo rm -r /usr/src/slayer-r9t-fan-0.1.0
  dkms status slayer-r9t-fan           # yalnız 0.2.0 … installed
  modinfo -F version r9t_fan           # 0.2.0
  ```

  0.1.0 kaynağı git geçmişinde ve `legacy-0.12.0` etiketinde durur.

## Düzeltme sonrası açılış (2026-10-19 18:44)

Kurulan: `r9t-studio 0.1.0.r68.gb44b715`, `slayer-r9t-fan-dkms 0.2.0-2` (snapper 463/464, DKMS üç kernelde yeniden kuruldu), `systemctl --user enable r9t-studio-tray`. Öncesinde `dkms remove slayer-r9t-fan/0.1.0 --all` ve `/usr/src/slayer-r9t-fan-0.1.0` silindi; `dkms status` yalnız 0.2.0.

- `systemd-modules-load`: `Inserted module 'r9t_fan'` (18:44:18), r9td 18:44:20'de başladı → `authority.state = control`, oturum `hyprland:`, fan 3489/3517 rpm (EC otomatik).
- `r9t-studio-tray.service` active (18:44:32, `/usr/lib/r9t-studio/r9t-studio --hidden`), SNI kaydı `org.kde.StatusNotifierItem-1723-1`. Simgenin Waybar'da görünmesi kullanıcının gözüne kalıyor.
- Gözlem: gizli arayüz WebKit süreçleriyle ~330 MB kullanıyor (pencere hiç açılmadan). Tepsi için WebView'i ilk gösterime kadar ertelemek `ileride.md` adayı.

## tuxedo modülleri pakete, tepsi belleği (2026-10-19 akşam)

- **tuxedo modülleri:** `tuxedo-drivers-r9t-dkms` 4.24.0-2, `/usr/lib/modules-load.d/tuxedo-drivers-r9t-dkms.conf` dosyasını kurar (`tuxedo_io`, `uniwill_wmi`). Bağımlılık olarak `tuxedo_keyboard` ve `tuxedo_compatibility_check` gelir. WMI aygıtları (ABBC0F6B/6D/6F/72) bu modülleri udev'den de yükletir, ama udev eşzamansızdır. r9td ise `systemd-modules-load`'dan sonra başlar ve `/dev/tuxedo_io`'yu hemen arar. Paket kurulunca sahipsiz `/etc/modules-load.d/slayer-r9t.conf` gereksiz kalır; eski sistem kaldırılırken silinir. Eski servis yedekte kalırsa modülleri yine kendisi yükler (`ExecStartPre=modprobe`).
- **Tepsi belleği:** pencere `tauri.conf.json` içinde `visible: false` olarak tanımlıydı. Bu yüzden girişte hiç açılmasa da WebKit süreçleri başlıyordu: birimin cgroup'u ~330 MB, WebKitNetworkProcess + WebKitWebProcess. Pencere artık ilk gösterimde oluşturuluyor (`show_main` → `WebviewWindowBuilder`); tepside dururken WebKit süreci yok. Ölçüm (`npx tauri build --no-bundle` ikilisi, RSS toplamı):

  | Durum | WebKitWebProcess | RSS (MB) |
  |---|---|---|
  | Gizli (`--hidden`), pencere hiç açılmadı | 0 | 93 |
  | Pencere açık / kapatıldıktan sonra (3 döngü) | 1 | ~600, sabit |

  İlk açılış ~340 ms. **Kapatınca yok etme denendi, bırakıldı:** her aç-kapa bir WebKitWebProcess'i geride bırakıyordu (3 döngüde 1 → 5 süreç, ~250 MB/döngü). Kapatma yine gizler. `--view`, pencere henüz yokken ikinci örnekten gelirse saklanır ve `app_info` ile bir kez okunur. Üç durum ekranda denendi: ilk örnek `--hidden --view performance`, pencere yokken ikinci örnek `--view keyboard`, gizli pencereye `--view profiles`. Üçü de doğru görünümü açtı.

## Açılış: eski modül dosyası olmadan (2026-10-19 19:54)

Kurulan: `tuxedo-drivers-r9t-dkms 4.24.0-2`, `r9t-studio 0.1.0.r70.g3f67f52` (snapper 465/466). Eski `/etc/modules-load.d/slayer-r9t.conf` yedeğe taşındı: `/var/lib/r9t/slayer-r9t.modules-load.conf.bak`.

- `systemd-modules-load` yalnız paket dosyalarıyla `tuxedo_io`, `uniwill_wmi` ve `r9t_fan`'ı yükledi (19:54:41). `/dev/tuxedo_io` hazırdı, r9td 19:54:43'te başladı → `state: control`, fan 3208/3175 rpm. Başarısız birim yok.
- Tepsi: `r9t-studio-tray` active, SNI kayıtlı, **WebKit süreci yok**. Birimin cgroup belleği 77 MB (önce ~330 MB), RSS 56 MB.
- Kurulum sırasında pacman'in "Loading new kernel modules" kancası yeni dosyayı DKMS yeniden kurulumundan önce işledi (`tuxedo_keyboard.ko.zst: No such file`). Modüller bellekteydi, etkisi olmadı; bu yalnız aynı işlemdeki sıralama.

## r9t_fan 0.3.0 ve araçlar (2026-10-19 gece; kod, donanımda denenmedi)

- **`e49a8ef` r9t_fan 0.3.0** (`driver/r9t_fan/README.md`): EC önbelleği (`cache_ms`, kontrol mutex'i dışında tazeleme, nesil sayacıyla geçersiz kılma), `status2`, hwmon (`fan/temp/pwm`, yalnız okunur), override histerezisi (95/87 devreye, 90/82 + 10 sn bırakma), `min_duty` (25–50, varsayılan 50) ve sıcaklık rampası, test için `allow_low_duty` (%20) ve kernel içi durma algılaması (`error=-32`). Varsayılanlarla 0.2.0 davranışı: v1 `status` bayt bayt aynı, `manual` %50–100.
  - C düzeneği: önbellek, histerezis, taban, durma ve hwmon senaryoları. Üç kasıtlı bozma (bırakma eşiği, önbellek, durma) testte yakalandı. İlk denemede yakalanmayan bırakma eşiği bozması, senaryoya "10 sn geçti ama hâlâ sıcak" adımı eklenince yakalandı.
  - Kernel derlemesi `W=1` ile uyarısız: 7.2.9 (clang) ve lts 6.18.55.
  - r9td ve probe artık bilinen sürümler listesini kabul ediyor (`0.2.0`, `0.3.0`); bilinmeyen sürümde yine salt okunur kalıyor. r9td açılışta `allow_low_duty=0` yazıyor.
  - Paket: `slayer-r9t-fan-dkms` 0.3.0-1, `conflicts=('r9t-studio<0.1.0.r72')`.
- **Tasarım kararı (test modu rampası):** makine boşta 66–68 °C'de duruyor. `min_duty` rampası (CPU 60→75 °C) test modunda %20'ye inmeyi engellerdi. Bu yüzden `allow_low_duty` rampası tarama durdurma eşiklerine kaydırıldı: CPU 70→80 °C, GPU 65→75 °C; 80/75 °C'de taban %50.
- **`beb0ca9` `r9tctl fan-sweep`:** Faz 0.7 protokolü %45→20 için; r9td'nin `control.lock`'ını alır. Taban önerisi max(25, durma + 10). Plan "spin-up + 5" diyor; burada fan yalnız tarama sırasında durur ve kernel onu hemen auto'ya döndürür, bu yüzden yerine geçen daha temkinli ölçü bu.
- **`8e233de` `r9tctl legacy-remove`** ve `desktop --remove-legacy`: yedekli kaldırma. 7 gün kapısı var, sembolik bağı ve pakete ait yolu reddeder. Kaldırmadan sonra fallback fanları EC otomatikte bırakır; rollback/trial geri yükleme komutunu söyleyip reddeder.
- **`95e1796` `r9tctl check`:** salt okunur sağlık denetimi. Canlıda hepsi ✓; tek uyarı sahipsiz 7.2.5 kernel kalıntısı.
- **DKMS arşivli geri alma** (`ileride.md`): paketlerle gereksizleşti. Bir önceki paket `pacman -U` ile geri kurulur (DKMS kancaları yeniden derler) ve her pacman işleminde snapper anlık görüntüsü alınır.

## Kalan (Faz 5)

- [x] Paketlerin kurulumu ve doğrulaması (2026-10-19).
- [x] Açılışta r9td + r9t-session + modüllerin paketten geldiğinin doğrulanması (2026-10-19; iki eksik bulundu ve düzeltildi, yukarıda).
- [x] Düzeltilmiş paketlerin kurulumu ve bir sonraki açılışta `r9t_fan` + r9td denetimi + tepsinin kendiliğinden geldiğinin doğrulanması (2026-10-19 18:44, aşağıda).
- [ ] Bir sonraki kernel güncellemesinde DKMS autoinstall (`r9tctl check` ile görülür).
- [x] r9t_fan v2 kodu (0.3.0) ve tarama aracı (2026-10-19, `e49a8ef`, `beb0ca9`).
- [x] r9t_fan 0.3.0 kuruldu, açılışta doğrulandı (21:18): `r9tctl check` hepsi ✓, hwmon `sensors`'ta `r9t_fan` (fan/temp/pwm), r9td `control`.
- [x] Düşük duty taraması fiziksel olarak (2026-10-19, kullanıcı başında): %20'ye kadar durma yok, öneri `min_duty=25` (`07_fan_rpm_egrisi.md`).
- [x] `min_duty=25` (kullanıcı kararı 2026-10-19): fan paketi 0.3.0-3 `modprobe.d` ile uygular. Protokol tabanı %25, r9td her komutu modülün `min_duty`'sine yükseltir (0.2.0'da 50). Durma tablosuna %20–45 ölçümleri eklendi; arayüz tabanı yetenek bilgisinden (`probe.readings.fan.duty_floor`) alıyor. Hazır fan presetleri değişmedi.
- [ ] Yeni paketlerin kurulumu ve açılışta `min_duty=25` + r9td tabanının doğrulanması.
- [ ] Bir hafta günlük kullanım → `sudo r9tctl legacy-remove --yes` (en erken 2026-10-26; araç `8e233de`).
- [x] Kurulum ön kontrolü (`r9tctl check`), güvenli kaldırma (`legacy-remove`); DKMS arşivli geri alma pacman + snapper ile gereksiz (2026-10-19).
- [x] tuxedo modüllerinin paketten yüklenmesi; eski `/etc/modules-load.d/slayer-r9t.conf` emekli (2026-10-19).
