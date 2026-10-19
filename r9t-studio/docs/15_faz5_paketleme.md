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

## Kalan (Faz 5)

- [x] Paketlerin kurulumu ve doğrulaması (2026-10-19).
- [ ] Bir sonraki açılışta r9td + r9t-session + modüllerin paketten geldiğinin doğrulanması.
- [ ] Bir sonraki kernel güncellemesinde DKMS autoinstall.
- [ ] r9t_fan v2 (status2, hwmon, düşük duty taraması) — ayrı iş, fiziksel ölçüm gerektirir.
- [ ] Bir hafta günlük kullanım → eski Python sisteminin yedeklenip kaldırılması.
- [ ] Kurulum ön kontrolü, DKMS arşivli geri alma, güvenli kaldırma (`ileride.md` girdileri).
