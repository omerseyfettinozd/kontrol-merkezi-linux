# Faz 2 — r9td Gölge Modu: Kurulum ve Kabul

Tarih: 2026-10-18. Kurulum: `tools/install-r9td-dev.sh` (geri alma `tools/uninstall-r9td-dev.sh`). Unit: `packaging/systemd/r9td-shadow.service`. Grup: `r9t` (GID 945, sysusers). Polkit: `packaging/polkit/org.r9t.studio.policy`.

## Ölçütler

| # | Ölçüt | Sonuç |
|---|---|---|
| a | 24 sa kesintisiz; eşleşen örneklerin %99'u Faz 1 toleransında | **Sürüyor.** `r9t-shadow-compare.service` 2026-10-18 01:45'te başladı (dakikada bir örnek, `/var/lib/r9t/shadow-compare.jsonl`). Özet: `python3 -I tools/shadow-compare.py --summary /var/lib/r9t/shadow-compare.jsonl` |
| b | Boşta CPU < %0.5, RSS < 20 MB; status p99 < 2 ms | CPU **%0.478** (180 sn, NVML açık), **%0.433** (`--no-nvml`). RSS **6.9 MB** (`--no-nvml`); NVML açıkken 32.8 MB (sapma aşağıda). `r9tctl bench 2000`: p50 0.123 ms, **p99 0.214 ms** |
| c | Eski servisin status süresi en fazla %10 uzun; lease 0'a düşmez, error=-110 yok | 40 örnek: kapalı ort. 1266.5 / med. 1266.4 ms, açık ort. 1356.8 / med. 1362.8 ms → **ort. +%7.1, med. +%7.6**; p90 +%11.5, en kötü tek örnek 2038 ms (r9td'nin `status` okumasıyla çakışma). Lease/-110 24 sa kayıtta izleniyor |
| d | r9td EC bütçesi ≤ 6 işlem/sn | Gölge modda ~1.4 işlem/sn (2 sn'de 2 tuxedo_io + 30 sn'de bir `status`). Kontrol modu ayarlarıyla ölçüm 3.2/sn |
| e | runtime_status ≠ active iken NVML çağrısı yok | Birim testleri (`gpu::tests::suspended_gpu_never_touches_nvml`, `missing_runtime_status…`) ve `nvml_skipped_inactive` sayacı. Bu makinede ekran dGPU'ya bağlı olduğundan dGPU hiç uyumuyor; 10 dk `runtime_active_time` karşılaştırması **uygulanamaz** |
| f | `systemd-analyze security` ≤ 4.0 | **1.4 OK** |
| g | r9t grubu dışındaki kullanıcı reddedilir | Soket `root:r9t 0660`: grupsuz kullanıcı `Permission denied`; grup üyesi (setpriv ile) okuyabiliyor; uygulama düzeyinde de grup kontrolü (`UNAUTHORIZED`). `id` çıktısında r9t, kullanıcının **yeniden oturum açmasından sonra** görünecek |

Diğer doğrulamalar: abonelik akışı (1 Hz `snapshot` bildirimi), 64 KiB üstü istek `TOO_LARGE`, gölge modda kontrol çağrısı `READ_ONLY` ve denetim günlüğüne kayıt, geçmiş durdurmada kaydedilip açılışta geri yükleniyor.

## Ölçümle bulunan ve düzeltilenler

- Çekirdek CPU maliyeti (root, `docs/11` ölçümü): tuxedo_io okuması 0.75 ms, r9t_fan `status` 8.8 ms, 15 hwmon 1.9 ms (acpitz ACPI `_TMP` üzerinden EC okur), NVML tam örnek 0.93 ms. → acpitz okunmuyor; NVMe 10 sn, diğer yavaş sensörler ve CCD 5 sn; NVML saat/bellek 5 sn; CPU limit/boost/EPP 10 sn; duty yalnız EC manueldeyken hızlı yoldan.
- Seccomp `~@privileged` soket `chown`'unu engelledi (SIGSYS); `SystemCallFilter=@chown` geri açıldı.
- Geride kalan abonenin bağlantısı artık gerçekten kesiliyor (yalnız iletim durmuyordu).

## Plandan sapmalar

1. **RSS:** NVML init libcuda'yı yükler ve ~24–26 MB'ı kalıcı olarak tutar (`nvmlShutdown` sonrası da). r9td'nin kendisi 6.9 MB. Hedef "NVML hariç < 20 MB" olarak ölçüldü; NVML'siz çalışma için `--no-nvml`.
2. **Gölge modda EC sıklığı:** tuxedo_io 2 sn, `status` 30 sn (plan: 1 sn / 10 sn). Gerekçe ölçüt (c): EC tek mutex'li kanal; 1 Hz'te eski servisin status süresi %17.5 uzadı. GPU sıcaklığı NVML'den 1 Hz. Kontrol modunda plan değerleri (1 sn / 3 sn) geçerli.
3. **Gölge modda eski servisten RPM:** eski soket kullanıcıya ait 0600; gölge unit yetkisiz (`CapabilityBoundingSet=`) olduğundan bağlanamaz. Eski servis fanları manuel sürerken r9td `status` okumaz ve RPM `null` olur. 24 sa kayıt bunu ayrıca gösterecek.
