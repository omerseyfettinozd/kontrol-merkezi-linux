# Faz 5 — Paketleme: ara durum

Tarih: 2026-10-19. Ayrıntılı kurulum/geçiş sırası: `packaging/arch/README.md`. Bu adımda yalnız paketler derlendi ve içerikleri denetlendi; **hiçbiri kurulmadı**, sistem `install-r9td-dev.sh` kurulumuyla ve kalıcı geçişle (docs/14) çalışmaya devam ediyor.

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

## Kalan (Faz 5)

- [ ] Paketlerin kurulumu (README'deki sıra, kullanıcı sudo ile) ve kurulum sonrası doğrulama: dkms iki kernelde derler, r9td/r9t-session/arayüz yeni yoldan çalışır.
- [ ] Bir sonraki kernel güncellemesinde DKMS autoinstall.
- [ ] r9t_fan v2 (status2, hwmon, düşük duty taraması) — ayrı iş, fiziksel ölçüm gerektirir.
- [ ] Bir hafta günlük kullanım → eski Python sisteminin yedeklenip kaldırılması.
- [ ] Kurulum ön kontrolü, DKMS arşivli geri alma, güvenli kaldırma (`ileride.md` girdileri).
