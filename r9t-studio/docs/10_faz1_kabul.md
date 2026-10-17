# Faz 1 — Protokol ve HAL: Kabul

Tarih: 2026-10-18.

| Ölçüt | Sonuç |
|---|---|
| `cargo test --workspace` | 73 test geçti (protocol 21 + fuzz 3, hal 27, r9td 22) |
| `cargo clippy --workspace --all-targets -- -D warnings` | temiz |
| Protokol fuzz ve limitler | `crates/r9t-protocol/tests/fuzz.rs`: rastgele bayt ve JSON biçimli girdilerde (4000'er örnek) çözücü panik etmiyor, satır sınırı aşılmıyor. Birim testleri: batch reddi, bilinmeyen alan (üst düzey ve params), null/negatif/uzun id, sürüm, bilinmeyen yöntem, params dizi |
| `r9tctl probe` kullanıcı olarak | Root alanları `unavailable: permission` (fan status, tuxedo_io, RAPL); diğerleri okunuyor; çökme yok |
| `sudo r9tctl probe --json` ↔ eski `status` | `10_faz1_probe_karsilastirma.json`: CPU/GPU EC 71/65 = 71/65, tuxedo_io CPU 71, fan modu 0 = 0, duty 55 % = 55 %, RPM 3483/3450 ve 3506/3511 (≤ %1), NVML GPU 65 = EC 65 °C |

## Port edilen eski davranışlar (r9td kütüphanesi)

- `thermal/smoother.rs` — CurveSmoother: artış anında; düşüş 2 °C histerezis + 6 sn bekleme, güncelleme başına −%2.
- `thermal/target.rs` — sıcaklık hedefi: `clamp(100 + (T − hedef)×5, 50, 100)`, 100 MHz adımlar, −3 °C/10 sn yükseltme, 2 sn aralık, > 10 sn boşlukta acil durdurma.
- `thermal/guard.rs` — değişmez 4: 90/84 °C giriş, 85/80 °C + 10 sn bırakma; `null` sensör bırakmaz.
- `thermal/stall.rs` — değişmez 5: ölçülmüş eğrinin %50'si (≥ 500 RPM), 2 ardışık örnek ya da 10 sn status hatası → boost, sonra auto.
- `thermal/mod.rs` — termal otorite: koruma her moddan önce; `null` sıcaklıkta duty düşmez, 10 sn sürerse auto (değişmez 8).
- `policy/mod.rs` — otomasyon: uygulama (exe ya da `class:` glob) → güç kaynağı (3 sn kararlılık, belirsiz asla pil sayılmaz) → açılış; uygulama kapanınca geri yükleme.
- Presetler ve şema 3 belge doğrulaması `r9t-protocol`'de.

## Plandan sapmalar

- NVML `NVML_INIT_FLAG_NO_ATTACH` sürücü 615'te aygıt tutamacı döndürmüyor (`NOT_FOUND`). Yerine: oturum, dGPU'ya bağlı bir ekran varken açık tutuluyor (dGPU zaten uyuyamaz; okuma 0.008 ms), yoksa her okumada init → oku → kapat (~13 ms). `runtime_status` kapısı her çağrıdan önce aynen geçerli.
