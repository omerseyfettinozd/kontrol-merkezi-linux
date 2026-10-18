# Faz 3b — r9t-session ve Waybar

Tarih: 2026-10-18. Crate: `crates/r9t-session`. Unit: `packaging/systemd-user/r9t-session.service` (kurulu, etkin değil).

## Canlı doğrulama (Hyprland 0.56, kullanıcı oturumu)

| İş | Sonuç |
|---|---|
| Soket `$XDG_RUNTIME_DIR/r9t/session.sock` | 0600, aynı uid (SO_PEERCRED); bilinmeyen alan `INVALID_PARAMS` |
| Hz | Modlar `j/monitors`'tan (60/240/300); `keyword monitor` yalnız Hz'i değiştirir (konum/ölçek/dönüş korunur); 300→240→300 geri okundu; listede olmayan 144 Hz reddedildi |
| Parlaklık | logind `Session.SetBrightness` (`nvidia_0`), geri okuma |
| Ses / mikrofon | `wpctl` set + geri okuma |
| Wi-Fi / BT / uçak | `/dev/rfkill` (`CHANGE_ALL`), `/sys/class/rfkill` geri okuması; uçak modunda hata olursa önceki durum geri yüklenir |
| GPU süreçleri | NVML G+C listesi yalnız dGPU `active` iken; Hyprland/Xwayland/hyprpolkitagent kapatılamaz; kendi süreçleri pidfd + başlangıç zamanı kontrolüyle SIGTERM |
| Oyun algılama | `.socket2` `openwindow`/`closewindow`; `steam_app_*`, `.exe`, gamescope, Steam kütüphanesi ve Wine/Proton exe; `steam_app_424242` sınıflı pencere r9td'ye `GameStarted`/`GameStopped` olarak ulaştı |
| r9td kaydı | Kontrol modunda `session_register` polkit + logind `GetSessionByPID` doğrulamasıyla kabul (`hyprland:2`); gölge modda reddediliyor, 5 sn'de yeniden deneniyor; bağlantı kopunca kayıt düşüyor |
| Ses-reaktif RGB | `pw-record` (varsayılan çıkışın monitörü) → 7 bant (Goertzel) → 126 kare, ≤ 20 FPS `rgb_frame`; sessizlikte klavye karardı, kapatınca kayıtlı harita geri yüklendi |
| Idle | `ext-idle-notify-v1`; süre çalışma anında değişir (`set_idle_timeout`) |
| Hz politikası | `power-watch.sh` çalışırken Hz'e dokunmaz (karar 6.3); aksi halde pilde en düşük Hz, prizde geri |
| `r9tctl waybar --follow` | Anlık görüntü akışı; r9td yeniden başlatılınca 2.0 sn'de geri (Faz 3a) |
| `r9tctl migrate` | `settings.json` → 3 profil (`legacy_fan_profiles` atıldı) → `configure`; `apply_profile "Oyun - CPU Serin 3500"` 3500 MHz + boost + EPP + serin eğri |

## Plandan sapma

- **Ses:** pipewire-rs yerine `wpctl` + `pw-record` (aynı PipeWire sunucusu). Gerekçe: C bağlaması/clang derleme bağımlılığı yok; ses-reaktif efekt yalnız PCM enerjisi istiyor. Ses-reaktif yakalama yalnız açıkken çalışan bir alt süreçtir.

## Kullanıcı onayı bekleyenler (dotfiles)

- Hyprland başlatma satırı: `packaging/hyprland/r9t-session.conf` (eski 50. satırın yerine; kalıcı geçişte).
- Waybar modülü: `packaging/waybar/r9t.jsonc` → `~/dotfiles/config/waybar/config.jsonc`.
- `power-watch.sh` kararı (6.3): r9t-session devralsın mı?
