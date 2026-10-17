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
