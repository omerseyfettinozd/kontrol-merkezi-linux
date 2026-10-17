# R9T Studio — Ana Plan (Kesin Yol Haritası)

Tarih: 2026-10-17. Durum: **onaylandı, Faz 0 sürüyor** (0.0 ve 0.1 tamam). Sürüm: 2 (3 bağımsız incelemeden geçti: donanım/güvenlik, uygulanabilirlik, devir).

## 0. Önce Bunu Oku

- **Tek geçerli plan bu dosyadır.** Kanıtlar `03_dogrulama_raporu.md`'dedir.
- `01_mevcut_sistem_analiz_raporu.md` (= `analiz_raporu.md`) ve `02_yeni_nesil_mimari_plani.md` (= `mimari_ve_uygulama_plani.md`) Gemini'nin yazdığı birebir kopyalardır. Yalnız arka plan bilgisi sayılır.
- `r9t-studio/CLAUDE.md` Faz 0.1'de (2026-10-17) bu plana göre yeniden yazıldı; Gemini'nin hatalı şartnamesi kaldırıldı.
- **Çelişki olursa öncelik sırası:** bu dosya > `03_dogrulama_raporu.md` > canlı ölçüm > CLAUDE.md.
- CLAUDE.md'deki "../outputs/slayer_r9t/ değiştirme" kuralı Faz 0.2 birleştirmesini engellemez.

**Kullanıcı kararı (2026-10-17): "En mükemmel sonuç."**
- Yeni sistem Rust ile sıfırdan yazılır.
- Eski Python sistemi kalıcı geçişe (Faz 4 sonu) kadar günlük kullanımda kalır ve yedek olarak çalışır.
- Eski koda yeni özellik veya yama eklenmez. Yalnız dağınık kopyalar birleştirilir. Eski unitlere yalnız systemd drop-in'i eklenebilir.

**Çalışma kuralları**
- Claude **sudo çalıştırmaz ve şifre istemez.** Root gereken adımlar için `r9t-studio/tools/` altına betik yazar ve içeriğini gösterir. Kullanıcı betiği kendi terminalinde ya da Claude Code'da `! sudo …` ile çalıştırır. Betik sonucunu `docs/` altındaki hedef dosyaya yazar.
- Donanıma yazan her adımdan önce kullanıcı onayı alınır. Kabul ancak fiziksel ölçümle verilir.
- Git komutları depo kökünden (`/home/chaple/Belgeler/kontrol merkezi linux`) çalıştırılır. `git add -A` / `git add .` kullanılmaz; dosyalar yol yol eklenir. Push yalnız kullanıcı onayıyla, yalnız `origin`'e yapılır. `private-archive`'a push yapılmaz.
- Her faz sonunda testler geçer, kanıt `docs/` altına yazılır ve proje notu `/home/chaple/second-brain/300-Projects/Kontrol-Merkezi-Linux.md` güncellenir.

---

## 1. Doğrulanmış Donanım Gerçekleri (canlı sistem, 2026-10-17)

| Bileşen | Gerçek değer | Yazılım yolu ve kural |
|---|---|---|
| Cihaz | GAME GARAJ SLAYER R9T, BIOS N.1.19GAR06 | DMI `product_name` |
| CPU | AMD Ryzen 9 8940HX (Dragon Range, Zen 4, 16C/16T, V-Cache yok) | cpufreq `scaling_max_freq`, `boost`, amd-pstate EPP. RAPL `package-*` (root, 0400) |
| iGPU | Radeon 610M (`0000:07:00.0`) | amdgpu hwmon `power1_input` = **APU paket PPT**. iGPU watt'ı sayılmaz |
| dGPU | RTX 5070 Laptop (GB206M, `10de:2d18`, `0000:01:00.0`, 8 GB, sürücü 615.78) | NVML. `GetPowerManagementLimit` **N/A**. Okunanlar: `GetEnforcedPowerLimit` (şu an 105 W, Dynamic Boost ile değişir), default 50 W, constraints 5–115 W; hepsi salt okunur. Kontrol: `SetGpuLockedClocks` / `ResetGpuLockedClocks` (en fazla 3090 MHz). NVML fan alanı N/A |
| Ekran | eDP-1 2560x1600, modlar 60/240/300 Hz, şu an 300 Hz, scale 1 | **NVIDIA kartına bağlı** (`card1-eDP-1`). dGPU bu yapılandırmada D3cold'a girmez (`runtime_suspended_time=0`). Arka ışık `nvidia_0` (logind SetBrightness) |
| Fan / EC | DKMS paketi `slayer-r9t-fan` 0.1.0 (`/usr/src/slayer-r9t-fan-0.1.0`, 3 kernel dizininde, srcversion 471F13C24BB67AC993237C1). `outputs/fan-driver/r9t_fan.c` daha yeni ve sertleştirilmiş (81 satır fark), ama onun dkms.conf'u da 0.1.0 | `/sys/module/r9t_fan/parameters/status` (0400 root) ve `control` (0200 root). Bir EC işlemi ~57 ms. `status` 11–23 EC işlemi yapar ve kernel mutex'ini tutar. `manual` yalnız %50–100 kabul eder |
| tuxedo_io | `/dev/tuxedo_io` (0600 root) | **Önce zorunlu:** `0x8008EC06` R_HWCHECK_UW, sonuç 1 olmalı. Atlanırsa tuxedo-drivers 4.24.0'da `uw_feats` başlatılmamış kalır ve kernel oops riski doğar. Ardından `0x8008EF01` model ID (beklenen 0x1A) ve `0x8008EF00` arayüz dizesi ('uniwill_wmi' içermeli). Okuma: `EF10` CPU fan duty (EC 0x1804, 0–200), `EF11` GPU duty (0x1809), `EF12` CPU temp (0x043e), `EF13` GPU temp (0x044f), `EF14` mod (0x0751). Yazma yalnız acil durumda: `0xF014` W_UW_FANAUTO. Okuma hataları sürücüde kontrol edilmez; 0 ya da >115 değerler geçersiz sayılır. RPM ioctl'i yoktur |
| RGB | 126 LED `rgb:kbd_backlight`, `_1`..`_125`. Sürücü **ite_8291** (USB HID 048D:600B). 6×21 ızgara, kanal = satır×21+sütun | `brightness` **global** (0–50; tek LED'e yazmak hepsini değiştirir). Tuş rengi `multi_intensity` (0–255, yerel R9T_RGB_INTENSITY_FIX yaması, `multi_max_intensity` 255). Kare: `/sys/bus/hid/devices/0003:048D:600B.*/controls/buffer_input`=1 → 126× `multi_intensity` → `buffer_input`=0 (tek USB yazımı). r9td açılışta, çıkışta ve her hata yolunda `buffer_input=0` yazar |
| Fn-Lock, cTGP | `/sys/devices/platform/tuxedo_keyboard/fn_lock`, `/sys/devices/platform/tuxedo_nvidia_power_ctrl/ctgp_offset` (WMI, ~57 ms) | Önbelleklenir. cTGP 0→5→10 denemesinde NVML limiti 105 W'ta kaldı (features.py:57); **etkisi yok** kabul edilir, UI'da yoktur |
| Güç profili | power-profiles-daemon D-Bus (`org.freedesktop.UPower.PowerProfiles`, eski adı `net.hadess.PowerProfiles`): power-saver/balanced/performance. `platform_profile` **yok** | Bu makinede yalnız EPP'yi değiştirir |
| Pil | BAT0: `charge_now/full/full_design` (µAh), `current_now`, `voltage_now`, `voltage_min_design`, `cycle_count` (0 döner, güvenilmez) | W = current×voltage. Wh = charge×voltage_min_design. **Şarj eşiği yok** |
| Diğer sensörler | k10temp, 2× nvme (3+2 sensör), 2× spd5118, mt7921_phy0, acpitz, 2× ucsi USB-C PD, AC0 | Etiket ve aygıt bağlantısıyla eşlenir, hwmon indeksiyle değil |
| Ryzen PPT | ryzenadj kurulu. Getter yalnız 55 W/100 °C döndü; STAPM/Fast/Slow okunamadı | **Desteklenmiyor.** Watt preseti yok. İleride yalnız deneysel ve varsayılan kapalı olabilir |
| Masaüstü | Hyprland 0.56 (şu an), KDE Plasma 6 (ikinci oturum) | Hyprland'da `graphical-session.target` etkinleşmez. hyprland.conf:50 `import-environment` satırı. Tepsi: Waybar StatusNotifierWatcher. Polkit ajanı: hyprpolkitagent |
| Mevcut kullanıcı otomasyonu | `~/dotfiles/bin/power-watch.sh` (hyprland.conf:43): AC/pil Hz değişimi ve duvar kağıdı duraklatma | r9t-session devralır, ya da varlığını tespit edip Hz'e dokunmaz (açık karar 6.3) |
| Diğer servisler | nvidia-powerd aktif (Dynamic Boost; `scaling_max_freq`'e de yazabilir). tccd kapalı. Eski: `slayer-r9t.service` (enabled, `ExecStopPost=… --auto-once`, `Restart=on-failure`, `WatchdogSec=20`), `slayer-r9t-session.service` (user, enabled ama Hyprland'da başlamıyor) | nvidia-powerd korunur ve çakışması izlenir |
| Kernel / Secure Boot | linux-cachyos 7.2.9, linux-cachyos-lts 6.18.55 (ayrıca paketi olmayan bir 7.2.5 modül dizini var). Secure Boot kapalı | DKMS iki kernelde test edilir. MOK isteğe bağlı |
| Araç zinciri | rust 1.99.0, nodejs 26.11.1, npm 12.2.0, webkit2gtk-4.1 2.54.1, libayatana-appindicator 0.6.0, libpipewire 1.6.9, polkit 127. **tauri-cli yok** (pacman'da da yok) | Tauri CLI proje içine npm devDependency olarak kurulur |

**Eski sistemin konumları**
- Root soketi `/run/slayer-r9t/control.sock`: satır başına tek JSON, `version=1`, en fazla 128 KB.
- Durum dizini `/var/lib/slayer-r9t/` (0700 root): `policy.json`, `actions.jsonl`, `temperature-target-recovery.json`, `gpu-clock-owned`.
- Kullanıcı ayarları `~/.config/slayer-r9t-control-center/settings.json`.
- Oturum soketi `$XDG_RUNTIME_DIR/slayer-r9t-session.sock`.

**Ölçülmüş eski profiller (port edilir, değerler korunur)**
- `quiet`: power-saver, boost kapalı, CPU 1800 / GPU 1200 MHz
- `balanced_cool`: balanced, boost kapalı, 2400 / 1800
- `gaming_cool`: balanced, boost açık, 3500 / 2100
- `stock`: balanced, boost açık, sınır yok (0/0)
- `sweet_spot` (snapshot 43697e5): 4900 / 2475, EPP balance_performance, boost açık, Serin fan. Kısa sentetik testte iyiydi; CS2 uzun oyunda CPU 90–91 °C çıktı
- `Oyun - CPU Serin 3500` (kullanıcı profili): 3500 / 2475, EPP balance_performance, Serin fan. Uzun oyun için bu kullanılıyor

---

## 2. Hedef Mimari

```text
┌──────────────────────── Kullanıcı oturumu ───────────────────────┐
│  r9t-ui (Tauri v2 + Svelte 5)        r9tctl        Waybar modülü │
│   Rust tarafı iki istemci tutar:                                  │
│    donanım → r9td      masaüstü → r9t-session                     │
│                                                                   │
│  r9t-session (systemd --user, Rust)  $XDG_RUNTIME_DIR/r9t/session.sock │
│   • Hyprland IPC / KDE KScreen   • Hz, parlaklık, gece ışığı      │
│   • PipeWire (ses düzeyi, ses-reaktif RGB kareleri)               │
│   • ext-idle-notify   • oyun/pencere olayları                     │
│   • süreç sonlandırma (pidfd, aynı uid, kara liste, onay)         │
└────────────────────────────────┬─────────────────────────────────┘
          /run/r9td/r9td.sock (0660 root:r9t, SO_PEERCRED + polkit)
┌────────────────────────────────┴──────────── root ───────────────┐
│  r9td (Rust, Tokio)                                               │
│   • Sampler: tek okuyucu, kaynak başına sıklık, alan başına yaş   │
│   • Snapshot önbelleği + 300 sn ring buffer (kalıcı) + pub/sub    │
│   • Termal otorite: preset / eğri / PID hedef / max boost         │
│     + histerezisli kullanıcı alanı koruması                       │
│   • Güç: cpufreq, boost, EPP, PPD D-Bus, GPU locked clocks        │
│   • RGB motoru (≤ 20 FPS, buffer_input ile toplu yazım)           │
│   • Politika motoru (AC/pil udev, oyun ve idle olayları)          │
│   • Uyku inhibitor'ı, güvenli geri dönüş, denetim günlüğü         │
└────────────────────────────────┬─────────────────────────────────┘
   r9t_fan (0.2.0 → isteğe bağlı v2) · tuxedo_io · ite_8291 · NVML · hwmon · cpufreq
```

**İlkeler**
1. **Root servis masaüstüne asla bağlanmaz** ve `/run/user`'a dokunmaz. Masaüstüyle ilgili her şey r9t-session'dadır.
2. **Tek okuyucu, tek yazıcı.** EC'yi yalnız r9td'nin sampler'ı okur. Fana yalnız termal otorite yazar. RPC'ler önbellekten cevap verir.
3. **Güvenlik katmanlıdır:** politika (r9td) → kullanıcı alanı koruması (r9td) → kernel (lease, thermal override, uyku) → firmware (EC auto).
4. **Yetenek yoklaması:** Donanımda kanıtlanmamış hiçbir özellik UI'da aktif görünmez (şarj eşiği, PPT, cTGP).
5. **Sabit değer yazılmaz.** Hz modları, GPU limitleri ve model adı çalışma anında okunur.
6. **Oturum durumu bağlantının ömrüne bağlıdır.** Eski sistemdeki kalıcı `session_ready` bayrağı kullanılmaz.

### Workspace ve Depo

```text
r9t-studio/
├── Cargo.toml                 # workspace
├── crates/
│   ├── r9t-protocol/          # serde tipleri, JSON-RPC, sürüm, Snapshot (alan başına age_ms, null = yok)
│   ├── r9t-hal/               # trait'ler: Ec, FanCtl, Nvml, Hwmon, Power, Cpufreq, Rgb, Battery, Sleep. Gerçek + sahte
│   ├── r9td/                  # root servis
│   ├── r9t-session/           # kullanıcı ajanı
│   └── r9tctl/                # CLI, probe, migrate, waybar --follow
├── ui/                        # Tauri v2 + Svelte 5 (src-tauri = soket istemcisi)
├── spikes/                    # atılabilir denemeler (Faz 0.4)
├── tools/                     # kullanıcının sudo ile çalıştırdığı betikler
├── driver/                    # r9t_fan 0.2.0/v2, tuxedo-drivers yerel yaması
├── packaging/                 # sysusers, systemd unitleri, polkit, PKGBUILD, DKMS
├── data/keymap-tr-q.json      # 126 indeks → tuş (Faz 0.6)
└── docs/
```

Depo: `r9t-studio/` mevcut `kontrol-merkezi-linux` deposunda (monorepo) durur. Karar 2026-10-17'de verildi (§6.1). İlk commit yalnız `r9t-studio/CLAUDE.md` ve `r9t-studio/docs/` dosyalarını içerir ve eski kod birleştirme commit'lerinden ayrıdır. Kök `.gitignore`'a şunlar eklenir: `r9t-studio/target/`, `r9t-studio/ui/node_modules/`, `r9t-studio/ui/src-tauri/target/`, `r9t-studio/spikes/**/node_modules/`, `r9t-studio/spikes/**/target/`.

### Teknoloji Kararları

| Konu | Seçim |
|---|---|
| Async | Tokio |
| D-Bus | `zbus` (PPD, logind, polkit `CheckAuthorization`) |
| NVML | `nvml-wrapper` (libnvidia-ml.so.1 çalışma anında yüklenir), `InitFlags::NO_ATTACH`; runtime_status kapısı HAL'dadır |
| Uyku | logind `Inhibit("sleep", "delay")` + `PrepareForSleep` |
| AC/pil | `tokio-udev` (power_supply uevent); yoklama yalnız yedek |
| Ses | `pipewire` (pipewire-rs), yalnız r9t-session'da. Derleme bağımlılığı: clang |
| Hyprland IPC | `tokio::net::UnixStream` ile doğrudan, ek crate yok |
| Çerçeve | NDJSON (satır başına JSON). İstek ≤ 64 KB, yanıt ≤ 1 MB. JSON-RPC 2.0, batch yok, `deny_unknown_fields`, `protocol_version` |
| Akış | `subscribe` / `unsubscribe`. İstemci 256 mesaj geride kalırsa bağlantı kesilir |
| UI | Tauri v2 + Svelte 5. Faz 0.4 başarısız olursa Qt/QML |

### Yetki

- Okuma (telemetri, geçmiş, yetenekler): grup `r9t`.
- Normal kontrol (preset, fan modu, RGB, profil, Hz politikası): polkit `org.r9t.control`, `allow_active=yes`.
- Tehlikeli (varsayılan altı termal limit, PWM tabanı, deneysel PPT): polkit `org.r9t.dangerous`, `auth_admin_keep`.
- r9td çağıranın pid'ini SO_PEERCRED ile alır ve pidfd ile sabitler. Ardından `CheckAuthorization(unix-process{pid,start-time,uid}, AllowUserInteraction)` sorar. Aktif koltuk logind `GetSessionByPID` + `Session.Active` ile belirlenir.

### Durum ve Yapılandırma Konumları

- `/etc/r9t/r9td.toml`: yönetici ayarları (deneysel özellikler, güvenlik eşikleri). RPC ile değiştirilemez.
- `/var/lib/r9t/` (StateDirectory, 0750 root:r9t):
  - profiller ve politika/kurallar,
  - `actions.jsonl` denetim günlüğü (256 KB × 3 döner; `events` RPC'si buradan beslenir),
  - `temp-target-recovery.json`,
  - `history.bin` (ring buffer; duruşta yazılır, açılışta 300 sn'den yeniyse geri yüklenir).
- `~/.config/r9t-studio/`: yalnız UI tercihleri.
- Günlük: journald (`SyslogIdentifier=r9td`).

### Protokol Paritesi

**Eski r9td komutları (22, kaynak: `outputs/slayer_r9t/protocol.py` FIELDS)**
- `status`, `events`, `auto`, `fan_boost`, `fan_preset`, `manual`, `curve`
- `power`, `boost`, `thermal_preset`, `profile`
- `rgb`, `lighting_idle`, `fn_lock`, `camera`
- `charge_limit`: "desteklenmiyor" döner
- `temperature_target`, `temperature_target_stop`
- `configure`, `automation_pause`, `automation_resume`
- `session_ready`: yerini bağlantıya bağlı `session_register` alır

**Eski oturum ajanı komutları → r9t-session**
- `mode`, `brightness`, `nightlight`, `color_settings`/`icc`, `touchpad`, `keyboard_settings`/`remap`, `wifi`, `bluetooth`, `airplane`, otomasyon bildirimi.
- Faz 0.3 envanterinde her biri "taşınır / KDE'ye özel kalır / bırakılır" diye işaretlenir.
- Wi-Fi, BT ve uçak modu rfkill + NetworkManager/BlueZ D-Bus ile masaüstünden bağımsız yapılır.

**Yeni komutlar**
- Senaryo/oyun modu **tek atomik sunucu komutudur**. Eski GUI'deki komut düşürme hatası böylece tekrarlanmaz.
- Diğerleri: `history`, `capabilities`, `subscribe`.

### r9t-session Sözleşmesi

- **Soket:** `$XDG_RUNTIME_DIR/r9t/session.sock` (0600; SO_PEERCRED uid kendi uid'i olmalı).
- **Kayıt:** r9t-session r9td'ye bağlanır ve `session_register{logind_session_id, desktop, pid}` gönderir. r9td, pid'in o logind oturumuna ait olduğunu doğrular. Bağlantı koparsa r9td oturumu yok sayar.
- **Hyprland'da başlatma:** tek satır `exec-once = systemctl --user import-environment WAYLAND_DISPLAY XDG_CURRENT_DESKTOP HYPRLAND_INSTANCE_SIGNATURE && systemctl --user start r9t-session.service`. Bu satır eski 50. satırın yerine geçer ve kullanıcı onayıyla eklenir.
- **KDE'de başlatma:** `WantedBy=graphical-session.target`, `PartOf=graphical-session.target`.
- **Kapanma:** `.socket2.sock` EOF olursa ya da logind oturumu kapanırsa temiz çıkar. `Restart=on-failure` kullanılır; EOF ile çıkış başarı sayılır.
- **İki oturum aynı anda açıksa** yalnız logind'de `Active=yes` olan oturumun backend'i sürülür. VT değişiminde backend de değişir.
- **KDE tespiti:** `XDG_CURRENT_DESKTOP` / `KDE_FULL_SESSION` ve **sahibi olan** (NameHasOwner) `org.kde.KScreen`. Kısa async zaman aşımları kullanılır; IPC kabul yoluna bloklayan çağrı konmaz.
- **Oyun algılama:** `openwindow`/`activewindowv2` olayında pid j/clients'tan alınır. Olay başına bir `/proc/<pid>/exe` okuması yapılır. Ayrıca sınıf kuralları vardır (`steam_app_*`, wine/proton).
- **Hz:** Mevcut satır j/monitors'tan okunur, yalnız Hz değiştirilir. Tamamlanma monitor olayıyla ya da yeniden sorguyla doğrulanır. Modlar dinamik gösterilir (60/240/300).

### Performans Hedefleri (ölçülebilir)

| Metrik | Hedef |
|---|---|
| Önbellekli `status` RPC | p99 < 2 ms |
| EC hızlı yol | Sıcaklık ve duty 1 Hz, tuxedo_io EF12/EF13/EF10/EF11 ile (4 işlem/sn) |
| EC tam yol | r9t_fan `status` (RPM, mod, lease, thermal): auto'da 10 sn, manuel/eğri modunda 3 sn'de bir |
| EC toplam bütçe | Kernel worker dahil ortalama ≤ 12 işlem/sn. Manuel modda worker ~5 işlem/sn, lease yenilemesi ~3 işlem/sn yapar. Ölçülerek doğrulanır |
| Aynı alan | Aynı turda iki kaynaktan okunmaz |
| NVML | Yalnız `runtime_status == active` iken çağrılır (init dahil). Skaler değerler 1 Hz, süreç listesi 2–5 sn ya da talep üzerine. Tam örnek ~0.5 ms |
| r9td boşta | CPU < %0.5 bir çekirdeğin, RSS < 20 MB |
| `history` RPC | p99 < 20 ms. Pencere ilk karede r9td'nin çalıştığı süre kadar (en çok 300 sn) geçmiş çizer |
| RGB karesi | Faz 0.5'te ölçülür. Efektler ≤ 20 FPS; statikken, pilde ve idle'da 0 FPS |
| systemd güvenlik | `systemd-analyze security r9td` ≤ 4.0 |

---

## 3. Güvenlik Değişmezleri (her fazda geçerli)

1. **Lease.** Fan kontrolünde kernel lease'i (15 sn) korunur. r9td lease'i yalnız kontrol döngüsünden ve taze bir örnekle (yaşı < 5 sn) hesaplanmış hedefle yeniler; bu 3 sn'de bir olur. Ayrı bir keepalive görevi yoktur; örnek eskiyse lease yenilenmez. r9td `WatchdogSec=10` ile çalışır ve watchdog bildirimi de kontrol döngüsünden gönderilir. Açılışta `status` mode≠0 gösterirse r9td önce `auto` yazar, doğrular, sonra politikayı uygular.
2. **Duruşta geri dönüş.** r9td durunca (`ExecStopPost` + iç kapanış) fanlar auto'ya döner, CPU/GPU sınırları kayıtlı varsayılana geri yüklenir ve `buffer_input=0` yazılır. "Kayıtlı varsayılan" eski servis durduktan **sonra** okunur.
3. **Kernel thermal override (v1/0.2.0).** Manuel ya da boost modunda kernel worker her 1 sn'de EC sıcaklığına bakar (0x043e CPU, 0x044f GPU). CPU ≥ 95 °C ya da GPU ≥ 87 °C ise iki fan %100'e çıkar. Histerezis yoktur ve kullanıcı alanından kapatılamaz. Auto modda koruma EC firmware'indedir.
4. **Kullanıcı alanı koruması (r9td).** Her şeyden önce uygulanır. CPU ≥ 90 °C ya da GPU ≥ 84 °C ise %100. Bırakma için CPU < 85 °C ve GPU < 80 °C olmalı ve 10 sn geçmiş olmalıdır. Tüm termal kararlar EC sıcaklıklarıyla verilir. k10temp Tctl yalnız gösterim içindir; aradaki fark Faz 0.5'te ölçülür.
5. **PWM tabanı ve durma algılama.** v1/0.2.0'da kernel tabanı %50'dir. Ölçülmüş eşik (Faz 5) gelene kadar daha düşüğü istenmez. Durma algılama: duty > 0 iken art arda 2 `status` örneğinde RPM, o duty için Faz 0.7'de ölçülen RPM'in %50'sinin altındaysa (en az 500), ya da manuel/eğri modunda `status` 10 sn boyunca hata dönerse önce `fan_boost`, ardından `auto` yazılır ve olay kaydedilir. Algılama gecikmesi v1'de ≤ 10 sn, v2'de ≤ 3 sn olur.
6. **Tek yazıcı.**
   - `r9td.service`'te `Conflicts=slayer-r9t.service`; eski unite drop-in eklenir: `/etc/systemd/system/slayer-r9t.service.d/r9t-conflicts.conf` → `Conflicts=r9td.service`.
   - `r9t-session.service`'te `Conflicts=slayer-r9t-session.service`.
   - Gölge mod ayrı bir unittir (`r9td-shadow.service`). Conflicts= ve ExecStopPost içermez. Yazmayı sistem düzeyinde engeller: `ReadOnlyPaths=/sys`, `CapabilityBoundingSet=` (boş), tuxedo_io ve NVIDIA aygıtları için yalnız okuma izni.
   - r9td kontrol moduna geçmeden önce `/run/slayer-r9t/control.sock`'a bağlanmayı dener. Yanıt gelirse kontrolü reddeder.
   - Kendi kilidi: `/run/r9td/control.lock` (flock). Eski koda kilit eklenmez.
7. **Uyku.** r9td logind'den `delay` türü bir sleep inhibitor alır. `PrepareForSleep(true)` gelince lease yenilemeyi ve RGB efektini durdurur, `auto` yazar, mode=0 ve `mode_byte & 0x40 == 0` olduğunu doğrular, sonra kilidi bırakır (en fazla 2 sn). Kernel notifier ikinci katmandır. 0.2.0'da uyku sırasında `manual` yazmak -EBUSY döner; bu hata sayılmaz. `PrepareForSleep(false)` sonrasında 3 sn beklenir, art arda 2 geçerli EC örneği alınır, ardından politika yeniden uygulanır.
8. **Geçersiz sensör.** Okunamayan, aralık dışı (EC'de < 1 ya da > 115) ya da D3'teki bir kaynağın değeri Snapshot'ta `null` + `age_ms` olarak tutulur; **asla 0 °C yazılmaz.** Termal otorite `null` sıcaklıkla duty düşürmez. Mevcut duty'yi korur, durum 10 sn sürerse `auto` yazar. r9t_fan `status`'un -ERANGE dönmesi "fan kontrolü yok, EC auto" olarak gösterilir.
9. **Yazma sınırları.** Doğrulanmamış EC adresine yazılmaz. Eşleşmesi doğrulanmamış sürücü zorla yüklenmez. tuxedo_io'da hwcheck yapılmadan başka bir ioctl çağrılmaz.
10. **Kabul kanıtı.** Donanım kabulü yalnız fiziksel ölçümle olur. Mock ya da CI sonucu kabul sayılmaz.

---

## 4. Fazlar

### Faz 0 — Hazırlık, Birleştirme ve Yoklamalar

**Donanıma yazan adımlar (0.6, 0.7, 0.8) için ortak kural:**
- Önce eski servisin otomasyonu `automation_pause` ile duraklatılır ve sıcaklık hedefi `temperature_target_stop` ile kapatılır.
- Fan ve LED yazımları **eski servisin RPC'si üzerinden** yapılır (`manual`, `rgb`). Eski servis bir işlemi desteklemiyorsa servis durdurulur ve doğrudan yazılır.
- Adım bitince `auto` ve `automation_resume` gönderilir; başlangıç durumu geri yüklenir.
- Betikler `tools/` altındadır. Eski kodu değiştirmezler.

| # | İş | Kim | Kabul |
|---|---|---|---|
| 0.0 | **Depo kararı:** monorepo (varsayılan) ya da ayrı repo; uzak deponun özel/açık durumu. `.gitignore` eklemeleri. `r9t-studio/` ilk commit'i (yalnız CLAUDE.md + docs) | Kullanıcı karar verir, Claude uygular | `git status` temiz, karar bu dosyaya yazılmış |
| 0.1 | `r9t-studio/CLAUDE.md` baştan yazılır, kısa tutulur. İçerik: proje özeti; "tek plan `docs/04_ana_plan.md`, kanıt `docs/03_dogrulama_raporu.md`" yönlendirmesi; Bölüm 1 tablosu; workspace ağacı (r9t-session dahil); Bölüm 3 değişmezleri; çalışma kuralları (sudo yok, `git add -A` yok). Eski metindeki watt presetleri, NVIDIA güç slider'ı, 60/240 toggle, şarj eşiği, `/tmp` soketi ve "önce kernel v2" kaldırılır | Claude | Aşağıdaki "0.1 kabul komutu" boş döner; Bölüm 1–4 ile çelişki yok. Bu dosyanın §0'ındaki "CLAUDE.md geçersiz" notu kaldırılır |
| 0.2 | Eski kodu birleştir. Ayrıntı aşağıda | Claude | Ayrıntı aşağıda. Push kullanıcı onayıyla |
| 0.3 | Eski sistemin davranış envanteri: 22 RPC komutu ve alanları, oturum ajanı komutları (taşınır/kalır/bırakılır), preset değerleri, sıcaklık hedefi algoritması, fan histerezisi/kademeli düşüş, geri dönüş kaydı, settings.json ve policy.json şeması, test listesi. `/var/lib/slayer-r9t/` yalnız kullanıcının çalıştırdığı `sudo tools/legacy-state-dump.sh` ile okunur | Claude (+ kullanıcı sudo) | `docs/05_legacy_parite.md` |
| 0.4 | Tauri v2 denemesi `spikes/tauri-canvas/` altında (atılabilir). `npm i -D @tauri-apps/cli@2` ile kurulur; global kurulum ve sudo yok. Canvas + SVG + 60 FPS grafik + tepsi ikonu. Hyprland/NVIDIA'da (ve KDE'de) açılır; RSS, boşta CPU ve GPU etkisi ölçülür; `WEBKIT_DISABLE_DMABUF_RENDERER=1` ile ve onsuz denenir | Claude + kullanıcı (ekrana bakar) | `docs/08_tauri_spike.md`: ölçümler ve karar (Tauri ya da Qt/QML) |
| 0.5 | Yetenek yoklaması, **yalnız okuma**. `tools/probe-capabilities.sh` (kullanıcı sudo ile çalıştırır): `ryzenadj -i`, RAPL `name` ve `constraint_*`, r9t_fan status, tuxedo_io hwcheck + EF01 + EF00, EC ile k10temp sıcaklık farkı, dGPU `runtime_status` / `d3cold_allowed`, tek RGB kare yazım süresi ölçümü (mevcut rengi yeniden yazarak, buffer'lı ve buffer'sız). Root gerektirmeyenler de kaydedilir: `controls/buffer_input` yolu, `multi_max_intensity`, srcversion, ctgp_offset değeri. cTGP'ye yazılmaz | Claude betiği yazar, kullanıcı çalıştırır | `docs/06_yetenekler.json` |
| 0.6 | Tuş haritası kalibrasyonu: `tools/keymap-calibrate.py`. Başlangıçta 126 LED'in `multi_intensity` ve `brightness` değerleri kaydedilir, sonunda geri yüklenir. İndeksler tek tek yakılır, kullanıcı yanan tuşu söyler (ya da "LED yok") | Kullanıcı + Claude | `data/keymap-tr-q.json`: sürüm alanı, indeks → tuş kimliği, TR-Q etiketi, geometri |
| 0.7 | Fan RPM eğrisi, gözetimli: yalnız %50–100, %10 adımlarla, eski servisin `manual` RPC'si üzerinden. Ön koşullar: AC bağlı, boşta, CPU ve GPU EC sıcaklığı < 60 °C. Her adım ≥ 20 sn sürer ve ≥ 3 örnek alınır. Durdurma koşulları: CPU ≥ 80 °C ya da GPU ≥ 75 °C, art arda 2 örnekte RPM < 500, ya da `status` hatası. Her çıkışta (Ctrl-C/trap dahil) `auto` yazılır, mode=0 ve `mode_byte & 0x40 == 0` geri okunur | Kullanıcı (dinler) + Claude | `docs/07_fan_rpm_egrisi.json` (%50–100). %50 altı bu fazda **yapılamaz** (modül reddeder); Faz 5'e kalır |
| 0.8 | Sertleştirilmiş `outputs/fan-driver/r9t_fan.c` DKMS 0.2.0 olarak kurulur. Önce `driver/` altına kopyalanır; dkms.conf `PACKAGE_VERSION="0.2.0"`, `MODULE_VERSION("0.2.0")` yapılır. `tools/install-fan-0.2.0.sh` ve `tools/rollback-fan-0.1.0.sh` yazılır (0.1.0 kaynağı /usr/src'de kalır) | Claude betikleri yazar, kullanıcı çalıştırır | srcversion yeni kaynağı gösterir. Eski servisle uyum: status formatı aynı. `auto` sonrası mode=0 ve `mode_byte & 0x40 == 0`. Lease dolunca auto (≤ 16 sn). Uyku sırasında `manual` -EBUSY döner. Uyku/uyanma testi geçer. İki kernelde derlenir |

**0.1 kabul komutu** (çıktı boş olmalı):

```bash
grep -nE '9955|5070 Ti|0x8004EC|/tmp/r9td|140 ?W|charge_control_end|ryzenadj --' "/home/chaple/Belgeler/kontrol merkezi linux/r9t-studio/CLAUDE.md"
```

**0.2 Birleştirme ayrıntısı**

Kaynaklar:
- (a) Çalışma ağacındaki 0.11.1 oyun modu WIP'i: `outputs/slayer_r9t/{gui,telemetry,__init__,games}.py`, `outputs/tests/test_games.py`, `outputs/tests/test_gui_game_mode.py`, `outputs/docs/IMPLEMENTATION_STATUS.md`.
- (b) `codex/control-center-reliability-011` dalı (12 commit, 3eae153'ten ayrılmış).
- (c) `refs/codex/snapshots/*` altındaki tek commit'lik Codex görüntüleri. Bunlar dal değildir. Hepsi 3eae153 tabanlıdır:
  - 43697e5: sweet_spot
  - dd5091e: EC devri + single_instance
  - 2fc5d4b: deployment.py, driver_install.py, uninstall-r9t.py
- (d) Alınmayacaklar:
  - **6f089e3** birleştirilmez. Ortak geçmişi yoktur (0cc84ef / private-archive soyu) ve içeriği main 9dae0ba'da zaten vardır. Yalnız `AI_LAPTOP_ADAPTATION_GUIDE.md` ve README farkı elle değerlendirilir.
  - c4def1e codex dalının ucuyla aynıdır, ayrıca alınmaz.
  - `codex/docs-contribution` ve `claude/kontrol-merkezi-analiz-9aa2b6` dallarına dokunulmaz.

Adımlar:
1. **Yedek:** `git bundle create ../r9t-legacy-yedek-20261017.bundle --all`. Her snapshot için `git branch backup/snap-<sha> <sha>`.
2. WIP, yollarıyla tek tek eklenerek `legacy/0.11.1-game-mode` dalına commit'lenir. `r9t-studio/` bu commit'e girmez.
3. `main`'den `legacy/merge-0.12` dalı açılır. Sıra:
   1. (b) `--no-ff` merge
   2. dd5091e cherry-pick
   3. 2fc5d4b: yalnız codex dalında karşılığı olmayan parçalar alınır. Kurulum/kaldırma tasarımı olarak `deployment.py/driver_install.py` ile `uninstall_plan.py` arasından biri seçilir; gerekçe 05'e yazılır.
   4. 43697e5 cherry-pick
   5. (a) merge
4. `gui.py` dört kaynakta da değişiyor; çakışmalar elle çözülür. Sürüm tek yerde belirlenir (0.12.0 önerilir; iki ayrı "0.11.1" vardı).
5. Her adımdan sonra test: `cd outputs && PYTHONPATH=. PYTHONDONTWRITEBYTECODE=1 QT_QPA_PLATFORM=offscreen python -m unittest discover -s tests -q`. `live_*.py` dosyaları ve `R9T_LIVE_CHECK=1` çalıştırılmaz.
6. `/usr/local/lib/slayer-r9t` ve `~/.local/lib/slayer-r9t` kurulu kopyalarına bu fazda dokunulmaz. Birleşik sürümün kurulması ayrı bir kullanıcı onayı ister (önerilir: daha sağlam bir yedek sistem olur).

Kabul:
- Test komutu hatasız geçer.
- Her kaynaktaki test fonksiyonu adlarının birleşimi yeni dalda bulunur. Referans sayılar: main 209, WIP 219, codex dalı 277, 43697e5 212, dd5091e 223, 2fc5d4b 240. Beklenen toplam ≈ 300–335.
- Bilerek çıkarılan testler gerekçesiyle 05'te listelenir.
- Main'e merge ve push kullanıcı onayıyla yapılır.

### Faz 1 — Protokol ve HAL (donanıma yazmadan)

- `r9t-protocol`: Snapshot (alan başına `age_ms`, `null`), Profile, FanCurve, RgbFrame, Capability, RPC mesajları, hata kodları, sürüm. r9t-session sözleşmesi de burada yazılır.
- `r9t-hal`: her donanım için bir trait, gerçek + sahte implementasyon. `R9T_SYSFS_ROOT` ile sahte sysfs kökü. tuxedo_io için önce hwcheck zorunludur (değişmez 9).
- Eski davranışların port'u: preset değerleri, sıcaklık hedefi algoritması, fan histerezisi ve kademeli düşüş. Eski testlerin karşılıkları Rust'a yazılır (05'teki listeye göre).
- `r9tctl probe` ve sahte servisle iskelet.
- **Kabul:**
  - `cargo test` temiz, `cargo clippy -- -D warnings` temiz.
  - Protokol fuzz ve limit testleri (çerçeve boyutu, bilinmeyen alan, batch reddi).
  - `r9tctl probe` kullanıcı olarak çalışınca root gerektirmeyen alanları doğru basar, root alanlarını `unavailable: permission` olarak işaretler ve çökmez.
  - Kullanıcının çalıştırdığı `sudo r9tctl probe --json`, aynı anda alınan eski `status` ile uyuşur: sıcaklık ±2 °C, fan modu ve duty aynı, RPM ±%10, NVML sıcaklığı ±1 °C.

### Faz 2 — r9td Gölge Modu (salt okunur, eski servisle yan yana)

- Sampler, snapshot önbelleği, kalıcı ring buffer, pub/sub, soket, yetki ve denetim günlüğü.
- `r9td-shadow.service` (değişmez 6): hiçbir yazma yapmaz, kilit almaz.
- Gölge sampler EC'yi tuxedo_io ile 1 Hz okur (sıcaklık, duty, mod). r9t_fan `status`'u en çok 10 sn'de bir okur. Eski servis manuel/eğri/sıcaklık hedefi modundayken `status` okunmaz; RPM eski servisin `status` yanıtından alınır.
- Karşılaştırma aracı: r9td ve eski servisin değerleri yan yana kaydedilir.
- **Geliştirme kurulumu** (betikle hazırlanır, kullanıcı sudo ile uygular, geri alma betiği de yazılır):
  - `packaging/sysusers.d/r9t.conf` (`g r9t -`), `systemd-sysusers`, `usermod -aG r9t chaple` → yeniden oturum açma.
  - `r9td.socket`: `ListenStream=/run/r9td/r9td.sock`, `SocketUser=root`, `SocketGroup=r9t`, `SocketMode=0660`, `DirectoryMode=0755`.
  - `packaging/polkit/org.r9t.studio.policy`.
- **Sertleştirme:** `ProtectSystem=strict`, `ProtectHome=yes`, `PrivateTmp=yes`, `NoNewPrivileges=yes`, `StateDirectory=r9t`, `ConfigurationDirectory=r9t`, `RestrictAddressFamilies=AF_UNIX AF_NETLINK`, `DevicePolicy=closed` + gerekli `DeviceAllow`, `SystemCallFilter=@system-service`, `LockPersonality=yes`, `MemoryDenyWriteExecute=yes`.
  - **Kullanılmaz:** ProtectKernelTunables (sysfs'e yazmayı engeller), PrivateDevices (NVML ve tuxedo_io'yu gizler), ProtectKernelModules (modprobe ayrı bir `r9t-modules.service`'e alınırsa açılabilir).
- **Kabul:**
  - (a) 24 sa kesintisiz çalışır; eşleşen örneklerin %99'u Faz 1 toleransında.
  - (b) Boşta CPU ve RSS hedefte. Önbellekli status p99 < 2 ms.
  - (c) Gölge mod açıkken eski servisin status süresi, r9td yokkenki ölçümden en fazla %10 uzun. Eski servisin `lease_ms` değeri hiç 0'a düşmez ve `error=-110` görülmez.
  - (d) r9td'nin kendi EC bütçesi ≤ 6 işlem/sn.
  - (e) `runtime_status != active` iken hiç NVML çağrısı yapılmaz (birim testi + log sayacı). r9td açıkken ve kapalıyken 10 dk boyunca `runtime_active_time` farkı aynıdır.
  - (f) `systemd-analyze security r9td` ≤ 4.0.
  - (g) `id` çıktısında r9t grubu görünür; r9t grubu dışındaki bir kullanıcı soket bağlantısında reddedilir.

### Faz 3 — Kontrol ve Oturum Ajanı (yalnız deneme pencereleriyle)

**Ön koşul:** 0.8 tamamlanmış olmalı. r9td açılışta srcversion 0.2.0'ı görmezse kontrol yolunu açmaz ve salt okunur kalır.

- **3a. Termal otorite ve güç kontrolü**
  - Termal: preset, eğri, PID hedef, max boost, kullanıcı alanı koruması.
  - Güç: cpufreq, boost, EPP, PPD, GPU locked clocks.
  - r9tctl'in tüm komutları.
  - Uyku inhibitor'ı.
  - nvidia-powerd'nin `scaling_max_freq`'e yazmasına karşı geri okuma ve uyarı.
- **3b. r9t-session ve Waybar**
  - r9t-session: Hyprland/KDE backend, Hz politikası (power-watch.sh kararına göre), oyun algılama, idle, parlaklık, ses, Wi-Fi/BT, süreç sonlandırma.
  - Waybar: `~/dotfiles/config/waybar/config.jsonc`'a `custom/r9t` modülü eklenir (`"exec": "r9tctl waybar --follow"`, `"return-type": "json"`, `"restart-interval": 5`). Kullanıcı onayı gerekir.
- **3c. RGB motoru**
  - Efektler: statik, 4 bölge (keymap üzerinden sütun aralıkları), per-key, nefes, dalga, termal gradyan.
  - Ses-reaktif efekt r9t-session'dan gelen karelerle çalışır.
  - En fazla 20 FPS; pilde, idle'da ve kapak kapalıyken 0 FPS.
- **Göç:**
  - `r9tctl migrate` kullanıcı olarak `~/.config/slayer-r9t-control-center/settings.json` dosyasını okur ve çevirir: MHz/EPP alanları, 4 noktalı eğriden spline'a, exe yolu kuralları. Sonucu `configure` RPC'siyle gönderir.
  - `policy.json` r9td tarafında root olarak okunur.
- **`r9t-switch --trial <dk>`** (yalnız süreli deneme):
  1. Eski servise `temperature_target_stop` gönderilir; recovery kaydı varsa uygulanır.
  2. `gpu-clock-owned` varsa GPU locked clocks sıfırlanır.
  3. Eski servis durdurulur (ExecStopPost fanı auto'ya alır).
  4. Varsayılanlar okunur ve r9td kontrol modunda başlatılır.
  5. Süre dolunca ya da bir hata olursa eski servis otomatik geri gelir.
  6. Günlük kullanımda eski servis ve eski GUI aktif kalır.
- **Kabul (fiziksel, ayrı ayrı):**
  - Her preset uygulanır ve geri okunur.
  - Fan eğrisi yükte izlenir.
  - Sıcaklık hedefi yük altında tutar.
  - AC/pil geçişinde ve oyun açılışında politika ≤ 1 sn'de uygulanır.
  - Uyku/uyanma (değişmez 7).
  - `systemctl kill -s KILL r9td` → ExecStopPost ile ≤ 2 sn'de mode=0.
  - Test drop-in'i (`Restart=no`, ExecStopPost boş) ile manuel %60'ta `kill -9` → lease ile ≤ 16 sn'de mode=0, `error=-110`.
  - `kill -STOP` → watchdog sonrası yeniden başlatma, arada fan auto.
  - Yeniden başlatma ≤ 5 sn'de son politikayı uygular.
  - r9td yeniden başlatılınca Waybar modülü ≤ 5 sn'de geri gelir.
  - `--trial` süre dolunca eski servise temiz döner.
  - Her test sonrası mode=0 ve `mode_byte & 0x40 == 0` geri okunur.

### Faz 4 — Arayüz (6 Modül) ve Kalıcı Geçiş

1. **Genel Bakış:** üst şerit (profil, AC/pil, CPU/GPU °C ve W, Max Fan, Hz seçici), metrik kartları (CPU, GPU, DDR5, 2× NVMe, pil), ring buffer'dan dolu çift eksenli grafikler.
2. **Performans & Termal:** ölçülmüş presetler, CPU/GPU MHz tavanları, EPP, boost; sürüklenebilir spline fan eğrisi (canlı sıcaklık imleci, %50 tabanı görünür); PID hedef. PPT ve GPU güç limiti salt okunur.
3. **Klavye & RGB:** kalibre edilmiş TR-Q SVG haritası, tıkla ya da sürükleyerek boyama, hızlı gruplar, efektler. Global parlaklık ayrı bir kontroldür.
4. **Ekran, GPU & Otomasyon:** Hz (60/240/300 dinamik), parlaklık, AC/pil ve oyun kuralları (Steam listesinden tek tıkla). GPU Wake Inspector: G+C süreçleri, "ekran dGPU'da, D3cold olmaz" açıklaması, güvenli kapatma (r9t-session, onaylı).
5. **Donanım, Ses & Tanılama:** Fn-Lock, USB-C PD, PipeWire ses ve mikrofon, Wi-Fi/BT, s2idle analizi (sleepdiag port'u, s0ix istatistikleri), süreç izleyici. Şarj eşiği yalnız yetenek varsa gösterilir.
6. **Profiller & Kıyaslama:** karşılaştırma tablosu, JSON içe/dışa aktarma, 60 sn kararlılık testi.

- **Tepsi** (StatusNotifierItem, libayatana-appindicator): pencere kapanınca uygulama tepside kalır. Aktif profil ikon rengiyle, sıcaklıklar tooltip'te gösterilir. Anlık sayılar Waybar modülündedir.
- **Kabul:**
  - Her modül gerçek donanım verisiyle ekran görüntüsü ve etkileşim testinden geçer.
  - Koyu temada okunamayan metin yoktur.
  - 1180 px ve yarım ekranda (~1280 px) taşma olmaz.
  - UI boşta CPU < %1.
- **Kalıcı geçiş** (Faz 4 kabulünden sonra, kullanıcı onayıyla): `r9t-switch --commit`.
  - Eski servis ve oturum servisi disable edilir.
  - Menü başlatıcısı yeni UI'a yönlendirilir.
  - hyprland.conf exec-once satırı güncellenir.
  - Geri dönüş için `r9t-switch --rollback` kalır.

### Faz 5 — Kernel v2, Paketleme ve Eski Sistemin Kaldırılması

- **v2** ayrı bir DKMS sürümüdür (`slayer-r9t-fan` 0.3.0 ya da yeni ad). Varsayılan davranış 0.2.0 ile aynıdır.
  - v1 `status` satırı bayt bayt uyumlu kalır; ek alanlar `status2` ve hwmon'da sunulur.
  - Önbellek yalnız EC kaynaklı alanları tutar. `control_set` önbelleği geçersiz kılar. Kısa snapshot kilidi kullanılır; yenileme kontrol mutex'inin dışında yapılır. `time_before` + geçerlilik bayrağı, `sample_age_ms` alanı.
  - Düşük duty taraması için yalnız test amaçlı açılan `allow_low_duty=1` modül parametresi (varsayılan 0, alt sınır %20, kernel içi durma algılaması açık). Faz 0.7 protokolü uygulanır.
  - Güvenli taban = spin-up duty + %5, en az %25 (tuxedo FAN_ON_MIN_SPEED_PERCENT). Tarama bitene kadar taban %50 kalır.
  - Sıcaklığa bağlı sürekli taban fonksiyonu ve histerezis. Override için bırakma eşiği (ör. 90/82) ve en kısa bekleme süresi.
  - Hwmon kaydı yalnız önbellekten okur.
- **tuxedo-drivers yerel yaması** (`multi_max_intensity` / R9T_RGB_INTENSITY_FIX) `driver/` altına DKMS patch olarak alınır. r9td açılışta `multi_max_intensity` değerini kontrol eder; 50'ye dönmüşse uyarır ve değerleri ölçekler.
- **Paket:** PKGBUILD (r9td, r9t-session, r9tctl, ui; depends: libayatana-appindicator, webkit2gtk-4.1, power-profiles-daemon, polkit) + ayrı DKMS paketi. Downgrade yolu belgelenir. MOK isteğe bağlıdır (Secure Boot kapalı).
- **Eski sistemin kaldırılması:** `/usr/local/lib/slayer-r9t` ve `~/.local/lib/slayer-r9t` yedeklenip kaldırılır; eski unitler silinir.
- **Kabul:**
  - Temiz kurulum, güncelleme ve geri alma çalışır.
  - DKMS iki kurulu kernelde (7.2.x, lts 6.18.x) derlenir ve modül testi geçer.
  - Bir sonraki kernel güncellemesinde DKMS autoinstall doğrulanır.
  - v2 düşük duty taraması fiziksel olarak yapılır.
  - 1 hafta günlük kullanım.

---

## 5. Riskler

| Risk | Önlem |
|---|---|
| WebKitGTK + NVIDIA + Hyprland görüntü sorunları | Faz 0.4 denemesi; Qt/QML yedek planı |
| EC/WMI kanalının yavaşlığı (57 ms/işlem, mutex) | İki hızlı yol (tuxedo_io 1 Hz, status seyrek), bütçe ölçümü |
| tuxedo_io hwcheck atlanırsa kernel oops | HAL'da zorunlu sıra (değişmez 9), birim testi |
| Düşük PWM'de fan durması | %50 taban; v2'de ölçülmüş taban + durma algılama |
| Eski 0.1.0 modülünün zayıf geri dönüşü | Faz 3 öncesi 0.2.0 (0.8) |
| İki servis çakışması | Conflicts= drop-in'leri, gölge unit, soket yoklaması, flock |
| Geçişte arayüzsüz kalma | Faz 3 yalnız `--trial`; kalıcı geçiş Faz 4 sonunda |
| nvidia-powerd'nin `scaling_max_freq`'e yazması | Geri okuma, uyarı, politikanın yeniden uygulanması |
| tuxedo-drivers güncellemesinin yerel yamayı silmesi | DKMS patch, açılış kontrolü |
| Tuş haritası hatası | Kalibrasyon JSON'u tek kaynak |
| Birleştirmede test ya da özellik kaybı | Bundle yedeği, test adı birleşimi kontrolü |
| Kapsamın büyümesi | Faz kabul ölçütleri; yeni fikirler `docs/ileride.md`'ye |

## 6. Açık Kararlar

1. ~~**Depo** (0.0)~~ **Karar verildi (2026-10-17):** monorepo. `r9t-studio/` mevcut `kontrol-merkezi-linux` deposunda kalır; `origin` açık (public) kalır, `private-archive`'a push yapılmaz.
2. **UI teknolojisi** (0.4 sonrası): Tauri (varsayılan) ya da Qt/QML.
3. **power-watch.sh** (Faz 3b öncesi): r9t-session devralır (varsayılan) ya da kalır ve r9t-session Hz'e dokunmaz.
4. **Birleşik eski sürüm** (0.2 sonrası): yedek olarak kurulsun mu (öneri: evet)?
5. **PPT:** 0.5 sonucuna göre tamamen çıkar (varsayılan) ya da deneysel kalır.

## 7. Bir Sonraki Oturum Buradan Başlar

1. Bu dosyanın §0 bölümünü ve `03_dogrulama_raporu.md`'yi oku. Otomatik yüklenen `CLAUDE.md`'deki donanım değerlerine, soket yollarına, ioctl sabitlerine, watt presetlerine ve faz sırasına güvenme.
2. Kullanıcıya 6.1 (depo) kararını sor, ardından 0.0'ı uygula.
3. 0.1: `CLAUDE.md`'yi yeniden yaz ve kabul grep'ini çalıştır.
4. 0.2: önce `git status`, `git branch -a`, `git for-each-ref refs/codex` ile durumu yeniden doğrula. Bundle yedeğini al, sonra birleştirmeye başla. İlgisiz değişiklikleri koru.
5. 0.3 ve 0.4'e geç. 0.5–0.8 kullanıcıyla birlikte yapılır: betikler `tools/` altında, sudo'yu kullanıcı çalıştırır, fiziksel gözlem kullanıcıdadır.
