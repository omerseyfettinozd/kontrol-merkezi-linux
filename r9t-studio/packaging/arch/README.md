# Arch paketleri (plan Faz 5)

Üç paket, bu deponun çalışma ağacından derlenir (yalnız git'te izlenen dosyalar):

```bash
cd r9t-studio/packaging/arch/r9t-studio && makepkg -s
cd ../slayer-r9t-fan-dkms && makepkg
cd ../tuxedo-drivers-r9t-dkms && makepkg
```

| Paket | İçerik |
|---|---|
| `r9t-studio` | `/usr/lib/r9t-studio/{r9td,r9tctl,r9t-session,r9t-studio,r9t-switch,r9t-switch-user}`, `/usr/bin` bağlantıları, systemd birimleri (`r9td`, `r9td-shadow`, `r9t-fallback`, eski servis için `Conflicts=` drop-in, kullanıcı `r9t-session`), sysusers (`r9t` grubu), polkit, menü girişi ve simge, örnek Waybar/Hyprland parçaları `/usr/share/r9t-studio/` |
| `slayer-r9t-fan-dkms` | `/usr/src/slayer-r9t-fan-<sürüm>/`; dkms paketinin pacman kancaları kurulu her kernel için derler. `tuxedo-drivers-dkms` gerektirir |
| `tuxedo-drivers-r9t-dkms` | Resmi tuxedo-drivers v4.24.0 (GitHub arşivi, sha256 sabit) + `driver/tuxedo-drivers/tuxedo-drivers-4.24.0-r9t.patch` (R9T DMI eşleşmesi, kernel ≥ 7.2'de 8 bit RGB yoğunluğu). `tuxedo-drivers-dkms`'in yerine geçer (`provides`/`conflicts`). Ürettiği `/usr/src/tuxedo-drivers-4.24.0` ağacı ve udev/modprobe dosyaları bugün kurulu (elle yamalanmış) paketle bayt bayt aynıdır |

Paket kurulumu hiçbir servisi başlatmaz ya da etkinleştirmez; geçiş yine `r9t-switch` ile yapılır. Kaldırmadan önce `pre_remove` r9td'yi durdurur (fanlar EC otomatik) ve eski servis kuruluysa onu başlatır.

## Geliştirme kurulumundan pakete geçiş

Tek komut (paketler `packaging/arch/out/` içinde derlenmiş olmalı; `--dry-run` yalnız gösterir):

```bash
sudo r9t-studio/tools/migrate-to-packages.sh
```

Betik aşağıdaki adımları sırayla yapar, her birinden sonra DKMS/servis durumunu doğrular ve r9t-studio adımında hata olursa geliştirme kurulumunu geri yükleyip kalıcı geçişi yeniden uygular. Elle yapmak için:

`tools/install-r9td-dev.sh` her şeyi `/usr/local` ve `/etc/systemd` altına koyar; `/etc` altındaki birimler paketin `/usr/lib/systemd` birimlerini gölgeler. Geçiş sırası (kalıcı geçiş yapılmışsa da güvenli; aradaki birkaç saniyede fanlar EC otomatiktedir):

1. Paketleri derle (yukarıdaki üç komut).
2. Elle kurulmuş DKMS kaydını kaldır (pacman aynı yolu sahiplenecek): `sudo dkms remove slayer-r9t-fan/0.2.0 --all && sudo rm -rf /usr/src/slayer-r9t-fan-0.2.0`. Modül yüklü kalır; yeni paket hemen yeniden derler.
3. Geliştirme kurulumunu, durumu silmeden kaldır: `sudo r9t-studio/tools/uninstall-r9td-dev.sh` (`/var/lib/r9t` ve `r9t` grubu korunur; eski servis geçici olarak geri gelir).
4. `sudo pacman -U tuxedo-drivers-r9t-dkms-*.pkg.tar.zst slayer-r9t-fan-dkms-*.pkg.tar.zst r9t-studio-*.pkg.tar.zst` (pacman `tuxedo-drivers-dkms`'i kaldırmayı sorar: evet; içerik aynı olduğu için modüller aynı kaynaktan yeniden derlenir)
5. Kalıcı geçiş yapılmışsa yeniden: `sudo r9t-switch --commit` (masaüstü tarafı aynı kalır; `r9t-switch-user --status` ile bak).

Yeni tuxedo-drivers sürümü: yama yeni sürüme uyarlanıp `pkgver`/`sha256sums` güncellenir; yamasız resmi paket kurulursa r9td RGB'yi `multi_max_intensity` kontrolüyle devre dışı bırakır ve DMI eşleşmesi gidebileceği için fan köprüsü yüklenmeyebilir.

Geri dönüş: `sudo pacman -R r9t-studio` (eski servis geri gelir) ve gerekirse `tools/install-r9td-dev.sh`. Eski Python sisteminin kaldırılması bu paketlerin bir hafta sorunsuz kullanımından sonradır (plan Faz 5).
