# R9T Studio

GameGaraj Slayer R9T için Rust ile sıfırdan yazılan Linux kontrol merkezi: root servis `r9td`, kullanıcı ajanı `r9t-session`, CLI `r9tctl`, Tauri v2 + Svelte 5 arayüzü `ui/` ve `r9t_fan` kernel modülü (C). Proje saf Rust'tır: Python yok; C yalnız kernel modülü ve tuxedo yamasıdır, kabuk yalnız PKGBUILD/DKMS'tir. 2026-10-19'dan beri günlük kullanımda (kalıcı geçiş + Arch paketleri). Eski Python sistemi depodan kaldırıldı (etiket `legacy-0.12.0`); kurulu kopyası `/usr/local/lib/slayer-r9t` yalnız `r9t-fallback` için durur.

## Önce Oku

- **Tek geçerli plan:** `docs/04_ana_plan.md`. Kanıtlar: `docs/03_dogrulama_raporu.md`.
- `docs/01_*`, `02_*`, `analiz_raporu.md`, `mimari_ve_uygulama_plani.md` Gemini'nin raporlarıdır; yalnız arka plan. Donanım değerlerine güvenme.
- Çelişkide öncelik: `04_ana_plan.md` > `03_dogrulama_raporu.md` > canlı ölçüm > bu dosya.
- Yeni fikirler `docs/ileride.md`'ye yazılır; fazın kapsamı büyütülmez.

## Çalışma Kuralları

- Root gereken adım `r9tctl` alt komutu ya da paket kancası olarak yazılır (yeni Python/kabuk aracı yok) ve sonucu `docs/`'a yazılır. 2026-10-18'de kullanıcı `/goal` ile bu oturum için sudo yetkisi verdi; şifre hiçbir dosyaya, nota ya da hafızaya yazılmaz. Yetki yoksa kullanıcı `! sudo …` ile çalıştırır.
- **Uyku/uyanma testleri yalnız kullanıcı başındayken** (2026-10-18 gözetimsiz `rtcwake` s2idle'dan uyanmadı, `docs/09`).
- Donanıma yazan her adımdan önce kullanıcı onayı. Donanım kabulü yalnız fiziksel ölçümle; mock/CI kabul sayılmaz.
- Git komutları depo kökünden (`/home/chaple/Belgeler/kontrol merkezi linux`). **`git add -A` / `git add .` yok**, dosyalar yol yol eklenir. Push yalnız onayla, yalnız `origin`'e; `private-archive`'a asla. Depo: monorepo, `origin` açık (public).
- Eski unitlere yalnız systemd drop-in eklenir. Eski kod `git show legacy-0.12.0:outputs/...` ile okunur.
- Testler: `cargo test` (workspace; r9t_fan C hata testi dahil, `cc` gerekir) ve `cd ui/src-tauri && cargo test`. Kurulum: `packaging/arch/README.md` (makepkg + pacman -U); geçiş `r9tctl switch` / `r9tctl desktop`.
- Her faz sonunda testler geçer, kanıt `docs/`'a yazılır, `/home/chaple/second-brain/300-Projects/Kontrol-Merkezi-Linux.md` güncellenir.

## Donanım Gerçekleri (canlı sistem, 2026-10-17)

| Bileşen | Değer | Kural |
|---|---|---|
| Cihaz | GAME GARAJ SLAYER R9T, BIOS N.1.19GAR06 | Model adı DMI'dan okunur, sabit yazılmaz |
| CPU | Ryzen 9 8940HX (Zen 4, 16C/16T, V-Cache yok) | cpufreq `scaling_max_freq`, `boost`, amd-pstate EPP. RAPL root (0400) |
| iGPU | Radeon 610M `0000:07:00.0` | amdgpu `power1_input` = **APU paket PPT**, iGPU watt'ı değil |
| dGPU | RTX 5070 Laptop (GB206M, `10de:2d18`, `0000:01:00.0`, 8 GB) | NVML yalnız `runtime_status == active` iken; `NO_ATTACH` tutamaç vermiyor, oturum ekran dGPU'daysa açık tutulur, değilse örnekle-kapat. Güç limiti salt okunur (`GetEnforcedPowerLimit`, 5–115 W). Kontrol: `Set/ResetGpuLockedClocks` (≤ 3090 MHz). NVML fan alanı yok |
| Ekran | eDP-1 2560x1600, 60/240/300 Hz | **dGPU'ya bağlı** → dGPU D3cold'a girmez. Arka ışık `nvidia_0`. Hz modları çalışma anında okunur |
| Fan / EC | DKMS `slayer-r9t-fan` **0.2.0** yüklü (`driver/r9t_fan`, 7.2.9 + lts 6.18.55; 0.1.0 geri alma için built). Duty→RPM: %50 ≈ 3200, %100 ≈ 5700 (`docs/07`) | `/sys/module/r9t_fan/parameters/status` (0400) / `control` (0200). EC işlemi ~57 ms; `status` 11–23 işlem yapar, mutex tutar. `manual` yalnız %50–100 |
| tuxedo_io | `/dev/tuxedo_io` (0600 root) | **Önce `0x8008EC06` hwcheck (=1)**, sonra `EF01` model (0x1A), `EF00` arayüz ('uniwill_wmi'). Okuma: `EF10`/`EF11` CPU/GPU duty (0–200), `EF12`/`EF13` CPU/GPU °C, `EF14` mod. Yazma yalnız acil: `0xF014` fan auto. 0 ya da > 115 geçersiz. RPM ioctl'i yok |
| RGB | 126 LED, ite_8291 (USB HID 048D:600B), 6×21 ızgara, kanal = satır×21+sütun | `brightness` global (0–50). Renk `multi_intensity` (0–255). Kare: `controls/buffer_input=1` → 126 yazım → `buffer_input=0`. r9td açılış/çıkış/hata yolunda `buffer_input=0` yazar |
| Fn-Lock, cTGP | `tuxedo_keyboard/fn_lock`, `tuxedo_nvidia_power_ctrl/ctgp_offset` | Önbelleklenir. cTGP'nin etkisi yok, UI'da yok |
| Güç profili | power-profiles-daemon D-Bus: power-saver/balanced/performance | `platform_profile` yok; yalnız EPP'yi değiştirir |
| Pil | BAT0 `charge_*` (µAh), `current_now`, `voltage_now`, `voltage_min_design` | W = akım×gerilim. `cycle_count` güvenilmez. **Şarj eşiği yok** |
| Diğer sensörler | k10temp, 2× nvme, 2× spd5118, mt7921_phy0, acpitz, 2× ucsi, AC0 | Etiket ve aygıt bağıyla eşle, hwmon indeksiyle değil |
| Ryzen PPT | ryzenadj okuması eksik | **Desteklenmiyor**, watt preseti yok |
| Masaüstü | Hyprland 0.56, KDE Plasma 6 | Hyprland'da `graphical-session.target` yok. `~/dotfiles/bin/power-watch.sh` Hz otomasyonu yapıyor |
| Servisler | nvidia-powerd aktif; eski `slayer-r9t.service`, `slayer-r9t-session.service` | nvidia-powerd `scaling_max_freq`'e yazabilir, geri okunur |
| Kernel | linux-cachyos 7.2.9, lts 6.18.55, Secure Boot kapalı | DKMS iki kernelde test edilir |
| Araçlar | rust 1.99, node 26, webkit2gtk-4.1, pipewire 1.6, polkit 127 | tauri-cli yok → proje içine `npm i -D @tauri-apps/cli@2` |

Eski sistem: root soketi `/run/slayer-r9t/control.sock` (NDJSON, `version=1`), durum `/var/lib/slayer-r9t/`, ayarlar `~/.config/slayer-r9t-control-center/settings.json`. Ölçülmüş profiller (MHz tavanı + EPP + boost + fan) plan §1'dedir.

## Mimari

```text
r9t-studio/
├── Cargo.toml          # workspace
├── crates/
│   ├── r9t-protocol/   # serde tipleri, JSON-RPC 2.0 (NDJSON), Snapshot (alan başına age_ms, null = yok)
│   ├── r9t-hal/        # donanım trait'leri, gerçek + sahte; R9T_SYSFS_ROOT
│   ├── r9td/           # root servis, Tokio; soket /run/r9td/r9td.sock (0660 root:r9t)
│   ├── r9t-session/    # systemd --user ajanı: Hyprland/KDE, Hz, PipeWire, idle, süreç sonlandırma
│   └── r9tctl/         # CLI: probe, migrate, waybar --follow, switch/desktop (geçiş), keymap-calibrate, load-test
├── ui/                 # Tauri v2 + Svelte 5 (soket istemcisi Rust tarafında)
├── driver/             # r9t_fan (C, DKMS) + test/ (C hata senaryoları); tuxedo-drivers yaması
├── packaging/          # systemd, polkit, sysusers, arch/ (PKGBUILD'ler)
├── data/keymap-tr-q.json
└── docs/
```

- Root servis masaüstüne ve `/run/user`'a dokunmaz; masaüstü işleri r9t-session'dadır.
- Tek okuyucu (r9td sampler), tek yazıcı (termal otorite). RPC'ler önbellekten cevap verir.
- Kanıtlanmamış yetenek UI'da aktif görünmez. Sabit değer yazılmaz.

## Güvenlik Değişmezleri

1. **Lease:** kernel lease 15 sn. Yalnız kontrol döngüsü, yaşı < 5 sn örnekle, 3 sn'de bir yeniler. `WatchdogSec=10`. Açılışta mode≠0 ise önce `auto`.
2. **Duruş:** fanlar auto, CPU/GPU sınırları kayıtlı varsayılana, `buffer_input=0` (ExecStopPost + iç kapanış).
3. **Kernel override:** manuel/boost'ta CPU ≥ 95 °C ya da GPU ≥ 87 °C → %100.
4. **Kullanıcı alanı koruması:** CPU ≥ 90 / GPU ≥ 84 °C → %100; bırakma CPU < 85 ve GPU < 80 °C + 10 sn. Kararlar EC sıcaklığıyla; Tctl yalnız gösterim.
5. **PWM tabanı %50** (ölçüm gelene kadar). Durma algılama: duty > 0 iken 2 örnekte RPM < ölçülmüş değerin %50'si (≥ 500) ya da 10 sn `status` hatası → `fan_boost`, sonra `auto`.
6. **Tek yazıcı:** `Conflicts=` (iki yönlü drop-in), salt okunur gölge unit `r9td-shadow.service`, eski sokete yoklama, `/run/r9td/control.lock` flock.
7. **Uyku:** logind `delay` inhibitor; `PrepareForSleep(true)` → döngüyü/RGB'yi durdur, `auto`, mode=0 ve `mode_byte & 0x40 == 0` doğrula (≤ 2 sn). Uyanınca 3 sn + 2 geçerli örnek.
8. **Geçersiz sensör** `null` + `age_ms`; asla 0 °C. `null` ile duty düşmez; 10 sn sürerse `auto`.
9. **Yazma sınırları:** doğrulanmamış EC adresine yazma yok, zorla sürücü yükleme yok, hwcheck'siz tuxedo_io ioctl'i yok.
10. **Kabul** yalnız fiziksel ölçümle.
