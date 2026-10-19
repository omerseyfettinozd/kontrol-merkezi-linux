# Arch paketleri (plan Faz 5)

Üç paket, bu deponun çalışma ağacından derlenir (yalnız git'te izlenen dosyalar):

```bash
cd r9t-studio/packaging/arch/tuxedo-drivers-r9t-dkms && makepkg
cd ../slayer-r9t-fan-dkms && makepkg
cd ../r9t-studio && makepkg -s
sudo pacman -U */*.pkg.tar.zst      # ya da PKGDEST ile topladığın paketler
```

| Paket | İçerik |
|---|---|
| `r9t-studio` | Rust ikilileri `/usr/lib/r9t-studio/{r9td,r9tctl,r9t-session,r9t-studio}`; `/usr/bin` bağlantıları `r9tctl`, `r9t-studio`, `r9t-switch` (= `r9tctl switch`), `r9t-switch-user` (= `r9tctl desktop`); systemd birimleri (`r9td`, `r9td-shadow`, `r9t-fallback`, eski servis için `Conflicts=` drop-in, kullanıcı `r9t-session`), sysusers (`r9t` grubu), polkit, menü girişi ve simge, örnek Waybar/Hyprland parçaları `/usr/share/r9t-studio/`. `check()` bütün Rust testlerini ve r9t_fan C hata testini koşar |
| `slayer-r9t-fan-dkms` | `/usr/src/slayer-r9t-fan-<sürüm>/`; dkms paketinin pacman kancaları kurulu her kernel için derler. `tuxedo-drivers-dkms` gerektirir |
| `tuxedo-drivers-r9t-dkms` | Resmi tuxedo-drivers v4.24.0 (GitHub arşivi, sha256 sabit) + `driver/tuxedo-drivers/tuxedo-drivers-4.24.0-r9t.patch` (R9T DMI eşleşmesi, kernel ≥ 7.2'de 8 bit RGB yoğunluğu). `tuxedo-drivers-dkms`'in yerine geçer (`provides`/`conflicts`) |

Paket kurulumu hiçbir servisi başlatmaz ya da etkinleştirmez; geçiş `r9tctl switch` (root) ve `r9tctl desktop` (kullanıcı) ile yapılır. Kaldırmadan önce `pre_remove` r9td'yi durdurur (fanlar EC otomatik) ve eski servis kuruluysa onu başlatır.

## Komutlar

| Komut | İş |
|---|---|
| `sudo r9tctl switch --trial <dk>` | r9td süreli denetim; süre dolunca eski servis |
| `sudo r9tctl switch --commit` | r9td kalıcı denetçi (açılışta), eski servis/gölge kapalı |
| `sudo r9tctl switch --rollback` | eski servis geri; commit yapılmışsa açılış da geri alınır |
| `r9tctl desktop --commit` / `--rollback` / `--status` (`--dry-run`) | r9t-session, Hyprland `exec-once` satırı (yedekli), eski menü girişi |
| `r9tctl keymap-calibrate` | TR-Q tuş haritası (r9td denetimde, root gerekmez) |
| `r9tctl load-test <target\|fan> <sn> <çıktı.json>` | yük altında termal davranış |

Yeni tuxedo-drivers sürümü: yama yeni sürüme uyarlanıp `pkgver`/`sha256sums` güncellenir; yamasız resmi paket kurulursa r9td RGB'yi `multi_max_intensity` kontrolüyle devre dışı bırakır ve DMI eşleşmesi gidebileceği için fan köprüsü yüklenmeyebilir.

Geri dönüş: `sudo r9tctl switch --rollback && r9tctl desktop --rollback`, gerekirse `sudo pacman -R r9t-studio` (eski servis geri gelir). Geliştirme kurulumundan pakete geçiş 2026-10-19'da yapıldı (`docs/15_faz5_paketleme.md`; betik git geçmişinde: `tools/migrate-to-packages.sh`).
