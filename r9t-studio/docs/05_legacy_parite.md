# Eski Sistem Paritesi ve Birleştirme Kaydı

Tarih: 2026-10-18. Kapsam: Faz 0.2 (birleştirme) ve Faz 0.3 (davranış envanteri). Plan: `04_ana_plan.md`.

## 1. Faz 0.2 — Dağınık Kopyaların Birleştirilmesi

### Yedekler

- `../r9t-legacy-yedek-20261017.bundle` (`git bundle create --all`, doğrulandı): tüm dallar, `refs/codex/snapshots/*` ve `refs/codex/turn-diffs/*` dahil.
- Snapshot dalları: `backup/snap-43697e5`, `backup/snap-dd5091e`, `backup/snap-2fc5d4b`, `backup/snap-6f089e3`, `backup/snap-c4def1e`.
- Çalışma ağacındaki 0.11.1 oyun modu WIP'i `legacy/0.11.1-game-mode` dalında (418f685, taban 3eae153). Yalnız şu 7 yol commit'lendi; `r9t-studio/` girmedi: `outputs/slayer_r9t/{gui,telemetry,__init__,games}.py`, `outputs/tests/test_games.py`, `outputs/tests/test_gui_game_mode.py`, `outputs/docs/IMPLEMENTATION_STATUS.md`. İçerik çalışma ağacıyla bayt bayt doğrulandı.

### Birleştirme sırası (`legacy/merge-0.12`, taban `main`)

| Adım | Commit | Sonuç | Test |
|---|---|---|---|
| (b) codex dalı `--no-ff` | 127099a | Çakışmasız | 277 geçti, ama çıkışta SIGSEGV (aşağıda) |
| Test düzeltmesi | 88f1b52 | `test_compare.py` widget'ları Qt üzerinden siler | 277, 20/20 temiz |
| dd5091e cherry-pick | fd2bba7 | Çakışmasız; mesaj açıklayıcı yapıldı | 291 |
| 2fc5d4b | — | **Alınmadı** (gerekçe aşağıda) | — |
| 43697e5 cherry-pick | 05d7180 | Yalnız `IMPLEMENTATION_STATUS.md` ekleri çakıştı; ikisi de tutuldu | 294 |
| (a) WIP `--no-ff` | b3954ee | `IMPLEMENTATION_STATUS.md` çakışması + semantik çakışma (aşağıda) | 305 |
| Sürüm | cf82bb7 | `__version__ = "0.12.0"`, README başlığı, `live_*.py` sürüm listelerine 0.12.0 | 305, 20/20 temiz |

Test komutu: `cd outputs && PYTHONPATH=. PYTHONDONTWRITEBYTECODE=1 QT_QPA_PLATFORM=offscreen python -m unittest discover -s tests -q`. `live_*.py` ve `R9T_LIVE_CHECK=1` çalıştırılmadı. `git diff --check main legacy/merge-0.12` temiz.

### Bulunan ve düzeltilen sorunlar

1. **Çıkışta SIGSEGV (önceden vardı, birleşmeyle kötüleşti).** Testler "OK" yazdıktan sonra süreç 139 ile çıkıyordu: main'de 2/10, codex birleşmesinden sonra 10/10. Çekirdek dökümü: `QObject::~QObject` ← `PySide::destroyQCoreApplication` ← `runCleanupFunctions` (atexit). Ebeveynsiz `ComparePanelWidget` nesneleri Python referans sayımıyla serbest bırakılıyor, alt nesne sarmalayıcıları shiboken kaydında kalıyor ve PySide bunları çıkışta ikinci kez siliyordu. Çözüm: testlerde `dispose(w)` (`close` + `deleteLater` + `sendPostedEvents(DeferredDelete)`). `test_gui_sweet_spot.py` aynı yöntemi zaten kullanıyordu. Ürün kodunda bu widget'lar ana pencereye bağlı olduğundan sorun yalnız testlerde.
2. **Semantik çakışma: Sweet Spot kapısı × oyun modu düğmeleri.** 43697e5, `update_buttons` içinde seçili hazır profilin fan/EPP gereksinimini bütün `'thermal'` düğmelerine uyguluyordu. WIP ise aynı yetenekle 7 yeni düğme (senaryo ×4, oyun modu ×2, oyun profili kaydet) ekliyordu. Sonuç: açılır kutudaki seçim ilgisiz düğmeleri kapatıyordu ve `test_gui_sweet_spot` yanlış düğmeye tıklıyordu. Çözüm: uygulama düğmesi `'thermal_preset'` yeteneğini alır; seçime bağlı kapı yalnız ona uygulanır. Regresyon testi: `test_selected_preset_gate_does_not_block_other_thermal_actions`.

### Kurulum/kaldırma tasarımı seçimi: codex (`uninstall_plan.py`), 2fc5d4b alınmadı

İki snapshot da 3eae153'ten ayrılıp aynı sorunu (kurulum ön kontrolü, sürücü geri alma, kaldırma) farklı çözüyor ve `install-r9t.py`'nin aynı fonksiyonlarını farklı biçimde yeniden yapılandırıyor.

- **Codex dalı (seçildi):** `prepare_application` uygulamayı sürücü adımından önce hazırlar ve denetler; `r9t-uninstall-plan.py --dry-run` **salt okunur** kaldırma envanteri üretir (executor yok, `removable` alanları hep false); `build.sh` LLVM tespiti.
- **2fc5d4b (alınmadı):** `install-r9t.py --check` ön kontrolü (`deployment.py`), DKMS `mktarball/ldtarball` arşivli sürücü geri alma (`driver_install.py`), **dosya ve DKMS kaydı silen** `uninstall-r9t.py`.

Gerekçe:
1. Codex dalı birleşmiş ve testleri geçiyor; 2fc5d4b'yi üstüne almak `install-r9t.py`'de iki tasarımın elle kaynaştırılması demek. Bu, plandaki "eski koda yeni özellik eklenmez" kuralına aykırı yeni kod yazmak olur.
2. 2fc5d4b'nin sürücü geri alması ve kaldırıcısı `slayer-r9t-fan/0.1.0`'a sabit. Faz 0.8 sürücüyü `tools/` betikleriyle DKMS 0.2.0'a taşıyor; o noktadan sonra 2fc5d4b kurucusu ya reddeder ya da ikinci bir sürücü yolu olur.
3. Eski sistem Faz 5'te yeni paketlemeyle kaldırılacak. Eski kodda dosya silen bir kaldırıcının bulunması, salt okunur envanterden daha risklidir.
4. Faydalı fikirler (ön kontrol, arşivli DKMS geri alma) `ileride.md`'ye Faz 5 paketleme girdisi olarak yazıldı.

**Bilerek çıkarılan testler (31, hepsi 2fc5d4b):**
- `test_deployment.py` (14): `PreflightTests` ×5, `UninstallTests` ×9.
- `test_driver_installer.py` (13): `DriverInstallerTests` ×13.
- `test_installer.py` (4): `test_driver_build_failure_restores_previously_active_services`, `test_driver_rollback_failure_keeps_services_stopped_and_reports_backup`, `test_full_install_restores_driver_before_starting_previous_services`, `test_hardware_release_failure_prevents_driver_update`.

Snapshot `backup/snap-2fc5d4b` dalında ve bundle'da duruyor.

### 6f089e3

Birleştirilmedi (plan §4 0.2-d). Elle değerlendirme: `outputs/docs/AI_LAPTOP_ADAPTATION_GUIDE.md` main'dekiyle aynı. README'si main'den eski (0.10.0, 112 test, CI bölümü yok). Alınacak bir şey yok.

### Kabul

- Test kimliği birleşimi (dosya::sınıf::test): main 209, WIP 219, codex 277, 43697e5 212, dd5091e 223, 2fc5d4b 240 → birleşim **335**.
- `legacy/merge-0.12`: **305** = 335 − 31 (2fc5d4b) + 1 (yeni regresyon testi). main, WIP, codex, 43697e5 ve dd5091e testlerinin hepsi var.
- Tam paket 20 ardışık çalıştırmada çıkış kodu 0.
- Kurulu kopyalara dokunulmadı: `/usr/local/lib/slayer-r9t` (0.10.1), `~/.local/lib/slayer-r9t` (0.11.0).
- **Bekleyen:** main'e merge ve `origin`'e push kullanıcı onayıyla. Birleşik sürümün yedek olarak kurulması ayrı bir karar (plan §6.4).

### Açık not (6.4 kararı için)

`install-r9t.py --app-only`, `/usr/src/slayer-r9t-fan-0.1.0` kaynaklarının `outputs/fan-driver/` ile aynı olmasını şart koşuyor. Birleşik dalda `r9t_fan.c` (yüklü kaynak daha eski; 61 değişen satır) ve `build.sh` (codex LLVM düzeltmesi) farklı; `Makefile` ve `dkms.conf` aynı. Bu yüzden `--app-only` şu an reddeder; tam kurulum ise sürücüyü de (0.1.0 etiketiyle, sertleştirilmiş kaynakla) değiştirir. Birleşik sürüm kurulacaksa bu, Faz 0.8 (DKMS 0.2.0) ile birlikte planlanmalı.

---

## 2. Faz 0.3 — Davranış Envanteri

Kaynak: `legacy/merge-0.12` (0.12.0). Dosya yolları `outputs/` altındadır. Root durum dizininin içeriği `docs/legacy-state-dump.json`'dadır (`sudo tools/legacy-state-dump.sh` ile üretilir).

### 2.1 Root protokolü (`slayer_r9t/protocol.py`, `service.py`)

- Soket `/run/slayer-r9t/control.sock`, 0600, sahibi masaüstü kullanıcısı (`R9T_UID`). SO_PEERCRED uid ∈ {0, R9T_UID}.
- Çerçeve: satır başına tek JSON nesnesi, **bağlantı başına tek istek**, istek ve yanıt ≤ 131072 bayt. İstemci zaman aşımı 18 sn, sunucu bağlantı zaman aşımı 2 sn.
- İstek: `{version:1, id:str(1–64), op, …}`; alan kümesi **tam eşleşmeli** (eksik ya da fazla alan reddedilir). Yanıt: `{version, id, ok, data | error}`.
- Servis tek iş parçacıklı. Döngü: `hardware.monitor()` → `automation.tick()` → watchdog → `accept` (1 sn zaman aşımı). Bir istemci bekleyen diğerlerini bloklar.
- `status` dışındaki her işlem `actions.jsonl`'a yazılır: `{time, id, operation, ok, duration_ms, message[:600]}`, 256 KB × 3 döner; açılışta son 100 olay belleğe alınır.
- **Yan etkiler:** `configure`, `automation_resume`, `automation_pause`, `session_ready`, `lighting_idle` dışındaki her işlem önce ışık boşta durumunu kaldırır ve **otomasyonu duraklatır**. Fan/güç/profil işlemleri (`auto`, `manual`, `curve`, `fan_boost`, `fan_preset`, `power`, `boost`, `thermal_preset`, `profile`) etkin sıcaklık hedefini durdurur.
- Yanıt `outcome`: `verified` (geri okundu) ya da `accepted`.

### 2.2 Root RPC komutları (22) ve r9td karşılığı

| op | Alanlar | Eski davranış | r9td |
|---|---|---|---|
| `status` | — | Tam durum: cihaz, yetenekler, nedenler, EC, fanlar, güç profili, boost, RGB, soğutma (CPU/GPU/EPP/paket gücü/sıcaklık hedefi/presetler), features, otomasyon, sürüm. Her çağrı ~1.3 sn | Önbellekten (p99 < 2 ms), alan başına `age_ms` |
| `events` | — | Son 100 olay | `actions.jsonl`'dan |
| `auto` | — | Fan köprüsü varsa `Fans.auto()` (zaten auto ise yazmaz; yazdıktan sonra `mode==0` ve `mode_byte&0x40==0` geri okur). Köprü yoksa tuxedo_io yedeği (2.6) | Aynı doğrulama |
| `fan_boost` | — | `control ← boost`, hedef 100/100 | Aynı |
| `fan_preset` | `preset` | `balanced`/`cool` → eğri, `maximum` → boost | Aynı presetler |
| `manual` | `cpu, gpu` | int, 50–100 | Aynı (v1 tabanı %50) |
| `curve` | `points` | 2–10 nokta, 40–95 °C, %50–100, artan sıcaklık, azalmayan duty, ≤ 90 °C'de %100 ile biter | Spline'a göç (Faz 3), sınırlar aynı |
| `power` | `profile` | `powerprofilesctl set` + geri okuma | PPD D-Bus |
| `boost` | `enabled` | Bütün `policy*/boost` (yalnız `amd-pstate-epp`), hata halinde geri yükleme | Aynı |
| `thermal_preset` | `preset` | Preset ayarlarını `profile` olarak uygular | Aynı presetler (2.4) |
| `profile` | `settings` | Doğrula → anlık görüntü → sırayla uygula → hata halinde tümünü geri yükle (2.7) | Aynı, atomik |
| `rgb` | `rgb, brightness` | 126 LED'e tek renk, değişmeyen dosyaya yazmaz, geri okur | `buffer_input` ile tek kare |
| `lighting_idle` | `idle` | Idle'da parlaklığı 0'a indirir; dönüşte yalnız renk değişmemişse geri yükler | r9t-session idle olayıyla |
| `fn_lock` | `enabled` | `tuxedo_keyboard/fn_lock` + geri okuma | Aynı |
| `camera` | `enabled` | USB `5-1.4/authorized` (2b7e:c906 "FHD WebCam"); kapatmadan önce video düğümü açık mı bakar | Aynı |
| `charge_limit` | `end, start` | BAT0 eşiği yoksa reddeder (bu makinede yok) | "desteklenmiyor" |
| `temperature_target` | `cpu, gpu` | 2.5 | Aynı algoritma |
| `temperature_target_stop` | — | Kayıtlı CPU/GPU sınırlarını ve fanı geri yükler | Aynı |
| `configure` | `document` | Politika belgesini (2.8) doğrular ve `policy.json`'a atomik yazar | Aynı şema, sürümlü |
| `automation_pause` | — | `policy.paused` dosyası | Aynı |
| `automation_resume` | — | Duraklatmayı kaldırır; etkin sıcaklık hedefini durdurur | Aynı |
| `session_ready` | — | **Kalıcı** bayrak, oturum kapanınca sıfırlanmaz | `session_register` (bağlantıya bağlı) |

`status.capabilities`: `power, boost, rgb, manual_fan, curve, fan_boost, auto_fan, cpu_limit, cpu_epp, gpu_limit, undervolt(false), temperature_target, camera, rgb_map, fn_lock, charge_limit`. Hepsi `match` (DMI = GAME GARAJ / SLAYER R9T) ile çarpılır.

### 2.3 Oturum ajanı (`session.py`, `desktop.py`) → r9t-session

Soket `$XDG_RUNTIME_DIR/slayer-r9t-session.sock` (0600, aynı uid), istek ≤ 8192 bayt. `status`, `keyboard_settings`, `color_settings` dışındaki her işlem root servise `automation_pause` gönderir. 3 sn'de bir `status` yoklar (eski CPU yükünün kaynağı).

| Komut | Eski uygulama | Karar |
|---|---|---|
| `status` | kscreen-doctor, KWin/BlueZ D-Bus, nmcli | **Taşınır** |
| `mode` (Hz) | `kscreen-doctor output.N.mode.M` + geri okuma | **Taşınır**: Hyprland IPC / KScreen; modlar dinamik |
| `brightness` | kscreen-doctor %10–100 | **Taşınır**: logind `SetBrightness` (`nvidia_0`), masaüstünden bağımsız |
| `nightlight` | KWin NightColor (kwriteconfig6) | **KDE'ye özel kalır** |
| `icc` | kscreen-doctor ICC, `acsp` imza kontrolü | **KDE'ye özel kalır** |
| `touchpad` | KWin InputDevice D-Bus | **Taşınır**: KWin D-Bus / Hyprland `device` keyword |
| `remap` | kxkbrc `Options` (caps:escape, caps:ctrl_modifier, ctrl:swapcaps) | **KDE'ye özel kalır** (Hyprland'da kalıcılık hyprland.conf ister) |
| `keyboard_settings`, `color_settings` | `systemsettings` KCM açar | **Bırakılır** (KDE'de UI kısayolu olarak kalabilir) |
| `wifi`, `bluetooth`, `airplane` | nmcli, BlueZ D-Bus; airplane hata halinde geri alır | **Taşınır**: rfkill + NM/BlueZ D-Bus |
| Işık zaman aşımı | `r9t-idle` (KF6IdleTime) yardımcı süreci | **Taşınır**: ext-idle-notify |
| Pilde düşük Hz | eDP için aynı çözünürlükte en düşük Hz; kapanışta geri yükleme | **Taşınır** (karar 6.3: power-watch.sh) |
| Otomasyon bildirimi | `notify-send` | **Taşınır**: `org.freedesktop.Notifications` |
| `session_ready` yoklaması | 3 sn | **Bırakılır** → `session_register` |

### 2.4 Presetler (değerler korunur)

Termal (`cooling.py`):

| Anahtar | Ad | Güç | Boost | CPU MHz | GPU MHz | EPP | Fan |
|---|---|---|---|---|---|---|---|
| `quiet` | Sessiz / Ofis | power-saver | kapalı | 1800 | 1200 | — | — |
| `balanced_cool` | Serin Dengeli | balanced | kapalı | 2400 | 1800 | — | — |
| `sweet_spot` | Sweet Spot | balanced | açık | 4900 | 2475 | balance_performance | preset `cool` |
| `gaming_cool` | Serin Oyun | balanced | açık | 3500 | 2100 | — | — |
| `stock` | Sınırları kaldır | balanced | açık | 0 (oto) | 0 (oto) | — | — |

Fan (`fans.py`): `balanced` [[45,50],[60,60],[75,80],[85,100]], `cool` [[45,60],[60,75],[75,90],[85,100]], `maximum` = boost.

Kullanıcının `settings.json` profilleri (2026-10-18): `Sweet Spot - 5 Döngü` (4900/2475, balance_performance, cool), `Oyun - CPU Serin 3500` (3500/2475, balance_performance, cool; uzun oyun için kullanılan), `Oyun - Serin` (3500/2100, cool). Otomasyon kuralı yok.

Sınırlar: CPU MHz `allowed_min` (= max(1000, scaling_min+1)) ile `amd_pstate_max_freq` arası ya da 0; 0 = `cpuinfo_max_freq`. Boost kapalıyken sürücü isteği boost'suz tavana kırpar; doğrulama bunu hesaba katar. GPU 600–`nvmlDeviceGetMaxClockInfo` ya da 0 (= `ResetGpuLockedClocks`). GPU sınırının bağımsız geri okuması yok.

### 2.5 Sıcaklık hedefi (`temperature_target.py`)

- Hedef aralığı: CPU 60–85 °C, GPU 60–80 °C. Fan, CPU ve GPU frekans arayüzleri birlikte hazır olmalı; sıcaklıklar r9t_fan `status`'tan (1–115).
- Başlarken: CPU `scaling_max_freq` anlık görüntüsü ilk yazımdan **önce** `temperature-target-recovery.json`'a (0600, fsync + rename) yazılır. Servis açılışında kayıt varsa CPU sınırları geri yüklenir; kayıt geçersizse hedef başlatılmaz.
- Fan hedefi: `duty = clamp(100 + (T − hedef) × 5, 50, 100)`, `CurveSmoother` ile yumuşatılır, `manual` olarak gönderilir.
- Döngü 2 sn'de bir. İki tur arası > 10 sn ise (uyku/gecikme) acil durdurma.
- Frekans: T > hedef ve duty = 100 ise sınır 100 MHz düşer (taban: CPU `allowed_min`, GPU 600). T ≤ hedef − 3 °C, 10 sn sürerse sınır 100 MHz artar (tavan: başlangıçtaki sınır). Aradaki bölgede sayaç sıfırlanır.
- Koruma: fan modu manual değilse, mod biti düşmüşse, fan hatası varsa (thermal override dışında) ya da frekans sınırları başka bir işlemce değiştirilmişse acil durdurma: CPU/GPU geri yüklenir, fan **auto**. Normal durdurmada fan önceki moduna döner.
- `status.temperature_guaranteed` her zaman false.

### 2.6 Fan kontrolü (`fans.py`, `hardware.py`)

- `status` satırı: `mode cpu_target gpu_target cpu_rpm gpu_rpm cpu_temp gpu_temp mode_byte cpu_pwm gpu_pwm lease_ms error thermal` (tam küme şart).
- `monitor()` 2 sn'de bir: `status` okur, manual/boost'u **yeniden gönderir** (lease yenileme). `mode==0` ya da mod biti düşmüşse hata → auto.
- Eğri: `curve_target` doğrusal ara değer. `CurveSmoother`: artış hemen; düşüş için sıcaklık çapanın 2 °C altına inmeli ve 6 sn sürmeli, sonra her yenilemede en çok %2 düşer.
- `thermal=1` (kernel override) → hedef 100/100, smoother sıfırlanır, doğrulama durur.
- Doğrulama: PWM (0–200) hedef×2'ye ±6, iki RPM > 500; art arda 2 örnek ve başlangıçtan ≥ 6 sn. 15 sn boyunca sağlanmazsa auto.
- **tuxedo_io yedeği** (köprü yoksa `Platform.auto`): `EF14` modu okunur; `0x40` biti varsa **`W_UW_MODE` (0xF012) ile bit temizlenir**, sonra `W_UW_FANAUTO` (0xF014). Plan §1 yalnız 0xF014'ü listeliyor; r9td'de 0xF012 yazımı ayrıca değerlendirilmeli (Faz 1 HAL kararı).
- `Platform.status()` önce tuxedo_io `EF12/EF13/EF10/EF11`, sonra r9t_fan `status` okur (mükerrer EC okuması; `03` §2).
- `driver_io.verify_device`: DMI → `0xEC06` hwcheck = 1 → `EF01` = 0x1A → `EF00` 'uniwill_wmi' içerir. `open_device` root ister.

### 2.7 Geri dönüş kayıtları

- `apply_profile` sırası: güç → boost → CPU MHz → EPP → RGB / RGB haritası → GPU MHz → fan. Hata halinde ters yönde: güç, boost, CPU, EPP, RGB (yazıp geri okuyarak), GPU, fan (manual/eğri ise 10 sn doğrulama, olmazsa auto). Eksik geri yükleme mesaja eklenir.
- `gpu-clock-owned` (0600): GPU sınırı yazılmadan önce oluşturulur, 0'a dönünce silinir. Açılışta varsa `ResetGpuLockedClocks`.
- Duruş (`Platform.close`): sıcaklık hedefi acil durdurma → ışık idle kaldır → fan auto → GPU sınırını sıfırla → tuxedo_io auto. Unit `ExecStopPost=… --auto-once`.

### 2.8 `settings.json` ve `policy.json` şeması (ortak, `schema: 3`)

```text
{ schema: 2|3 (yazılırken 3),
  profiles: { "<ad 1–80>": Profile } (≤ 100),
  legacy_fan_profiles: {…} (şemasız eski dosya buraya arşivlenir),
  automation: { startup|ac|battery: profil adı|null,
                apps: [{executable: "/mutlak/yol", profile}] (≤ 32, tekil, ilk eşleşen kazanır),
                low_hz_on_battery: bool, lighting_timeout: 0–3600 sn } }
Profile ⊆ { power_profile: power-saver|balanced|performance, boost_enabled: bool,
            cpu_max_mhz: 0–6000, gpu_max_mhz: 0–4000, cpu_epp: performance|balance_performance|balance_power|power,
            rgb: [r,g,b] | rgb_map: 126×[r,g,b] (ikisi birden olmaz), brightness: 0–100 (renkle birlikte zorunlu),
            fan: {mode:auto|boost} | {mode:manual,cpu,gpu} | {mode:curve,points} | {mode:preset,preset} }
```

- Belge ≤ 64 KiB (JSON), dosya ≤ 128 KiB, iç içe ≤ 64 düzey. Atomik yazım (mkstemp + fsync + rename).
- Kullanıcı dosyası bozuksa `.invalid-<ns>` ekiyle saklanır; şema 2 → 3 göçünde `.schema2-backup` bırakılır.
- `policy.json` root tarafındaki kopyadır; GUI her kayıtta `configure` ile gönderir. `policy.paused` duraklatma bayrağıdır.

### 2.9 Otomasyon (`automation.py`)

- 1 sn'de bir tik. Güç kaynağı: herhangi bir `Mains`/`USB` kaynağı `online=1` → `ac`; hepsi kesin 0 → `battery`; belirsizse `None`. Değişim **3 sn** kararlı kalınca kabul edilir.
- Karar sırası: uygulama kuralları (kullanıcı uid'ine ait süreçlerin `/proc/*/exe`'si, ilk eşleşen) → güç kaynağı kuralı → açılış kuralı (bir kez).
- Yalnız `session_ready` ve duraklatılmamışken çalışır. Uygulama kuralına girerken mevcut ayarların anlık görüntüsü alınır; uygulama kapanınca geri yüklenir.
- Herhangi bir hata otomasyonu duraklatır ("yeniden etkinleştirme gerekli").

### 2.10 Bilinen eski hatalar (yeni sistemde tekrarlanmaz)

1. **GUI komut düşürme:** `ControlCenter.command` `action_busy` iken yeni komutu sessizce bırakır. Oyun Modu (`thermal_preset` + `fan_preset`), Günlük Moda Dön, senaryolar (Sessiz/Dengeli/Oyun: 2 komut, Turbo: 4 komut) yalnız **ilk** komutu uygular. 0.12.0'da da durum bu (eski koda yama yapılmadı); testler `command`'ı taklit ettiği için yakalamıyor. r9td'de senaryo tek atomik komuttur.
2. `session_ready` kalıcı bayrak; Hyprland'da oturum servisi hiç başlamıyor (`graphical-session.target`).
3. Her elle komut otomasyonu sessizce duraklatıyor.
4. `status` ~1.3 sn ve tek iş parçacıklı servis; oturum ajanının 3 sn'lik yoklaması CPU yakıyor.
5. GUI, süreç panelinde `/proc` taramasını ve `nvidia-smi`'yi UI iş parçacığında çalıştırıyor (codex dalı süreç/ses panellerini işçi havuzuna aldı).

### 2.11 Test listesi ve Rust karşılığı

Toplam 305 test, 37 dosya (`live_*.py` ve `acceptance_gui_contract.py` hariç). "Port" = davranış Faz 1–3'te Rust testine çevrilir.

| Dosya | Test | Konu | Yeni sistemdeki yeri |
|---|---|---|---|
| test_core | 15 | Protokol çerçevesi, ayar göçü, profil/RGB geri yükleme | Port: r9t-protocol, r9td profile |
| test_automation | 25 | Politika motoru | Port: r9td |
| test_power_source | 6 | AC/pil sınıflandırma | Port: r9t-hal |
| test_fans | 13 | Fan auto devri, doğrulama, eğri | Port: r9td termal otorite |
| test_extensions | 15 | Smoother, geçmiş penceresi, EPP geri yükleme, sensör etiketleri | Port: r9td + r9t-hal |
| test_cooling | 6 | CPU/GPU sınırı, GPU işaret dosyası | Port: r9t-hal + r9td |
| test_temperature_target | 15 | 2.5 | Port: r9td |
| test_lighting | 8 | 126 haritası, desenler | Port: r9td RGB |
| test_persistence_depth | 5 | JSON boyut/derinlik | Port: r9t-protocol |
| test_rule_editing | 9 | Kural düzenleme/öncelik | Port: r9td `configure` doğrulaması + UI |
| test_profile_delete_rollback | 1 | Silmede geri alma | UI (Faz 4) |
| test_dynamic_boost | 14 | RAPL paket gücü delta, nvidia-powerd sağlığı | Port: r9t-hal |
| test_sleepdiag | 12 | Uyku öncesi/sonrası analiz | Port: r9tctl + UI |
| test_gpu_diagnostics | 12 | GPU Wake Inspector | Port: r9t-session + r9td |
| test_processes, _robustness | 21 | Süreç izleyici | Port: r9t-session |
| test_audio, _async, _edge_cases | 19 | wpctl ses | Port: r9t-session (PipeWire); async kısmı UI |
| test_session_shutdown | 4 | Kapanışta ekran/ışık geri yükleme | Port: r9t-session |
| test_games | 2 | Steam kütüphanesi | r9t-session / UI |
| test_cli_diagnostics | 2 | Salt okunur CLI | Port: r9tctl |
| test_compare | 12 | A/B karşılaştırma, CSV | UI (Faz 4) |
| test_history_export | 3 | Atomik CSV | UI (Faz 4) |
| test_gui_profiles, _rgb, _fan_status, _game_mode, _sweet_spot, _new_panels | 42 | GUI sözleşmeleri | UI (Faz 4); oyun modu → r9td atomik senaryo testi |
| test_single_instance | 4 | Tek GUI süreci | UI (Tauri single-instance) |
| test_live_acceptance_restore, _opt_in, _script_syntax | 7 | Canlı test kapıları | İlke port: canlı testler `R9T_LIVE_CHECK=1` olmadan başlamaz |
| test_fan_build | 6 | build.sh LLVM/GCC | driver/ (Faz 0.8 / 5) |
| test_installer | 20 | Eski kurucu | Port edilmez (Faz 5 kendi testleri) |
| test_uninstall_plan | 7 | Salt okunur kaldırma envanteri | Port edilmez (Faz 5 fikir girdisi) |
