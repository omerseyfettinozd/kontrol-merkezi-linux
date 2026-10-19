# tuxedo-drivers R9T yaması

`tuxedo-drivers-4.24.0-r9t.patch`: resmi TUXEDO v4.24.0 `src/` ile bu makinede çalışan sürücü kaynağı arasındaki fark (`../../../outputs/driver-patches/` kopyası). İçerik: `uniwill_keyboard.h` ve `tuxedo_compatibility_check.c` içinde GAME GARAJ / SLAYER R9T DMI eşleşmesi; `ite_8291.c` içinde kernel ≥ 7.2 için RGB kanallarının `max_intensity = 255` (R9T_RGB_INTENSITY_FIX).

`dkms.conf`: kurulu `tuxedo-drivers-dkms` 4.24.0 paketinden alındı (resmi arşivde yok). Paket: `packaging/arch/tuxedo-drivers-r9t-dkms`. Lisans: `TUXEDO-LICENSE` (GPL-2.0+).

Doğrulama (2026-10-19): resmi arşiv + yama → `/usr/src/tuxedo-drivers-4.24.0` ile `diff -r` boş; paketin udev/modprobe dosyaları kurulu paketle aynı.
