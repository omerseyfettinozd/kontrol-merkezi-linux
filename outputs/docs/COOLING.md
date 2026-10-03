# Serin çalışma — 0.4.0

Bu bölüm voltaj değerlerini değiştirmez. CPU boost, Linux güç profili, CPU/GPU
üst frekans sınırlarıyla daha düşük güç tüketimine yönelik seçimler sunar.
Hazır profiller sıcaklık/FPS optimizasyon ölçümü yapılmış sonuçlar değildir.

| Profil | Güç modu | CPU boost | CPU üst sınırı | GPU üst sınır isteği |
| --- | --- | --- | --- | --- |
| Sessiz / Ofis | Tasarruf | Kapalı | 1800 MHz | 1200 MHz |
| Serin Dengeli | Dengeli | Kapalı | 2400 MHz | 1800 MHz |
| Serin Oyun | Dengeli | Açık | 3500 MHz | 2100 MHz |
| Sınırları kaldır | Dengeli | Açık | Otomatik | Otomatik |

“Sınırları kaldır” başlangıç ayarlarına geri alma değildir: dengeli güç modu ve
açık boost ile frekans sınırlarını kaldırır. Profil işlemleri klavye RGB ayarına
 dokunmaz. CPU/GPU frekans seçimleri kişisel profillere de kaydedilebilir.

## Destek bulguları

- CPU AMD P-State EPP sürücüsü mevcut. `scaling_max_freq`, tüm politikalarda
  yazılıp geri okunur. Boost kapalıyken sürücü boost dışı tavanı uygular.
- RTX 5070 Laptop GPU, NVML `nvmlDeviceSetGpuLockedClocks` çağrısını kabul etti.
  Alt sınır 0 ile seçildiği için GPU'nun düşük frekanslara inmesi engellenmez.
  Anlık frekans okunur; kilitlenen üst sınır için bağımsız bir getter bulunmaz.
  Arayüzde “son GPU isteği” ile “ölçülen frekans” ayrı gösterilir. GPU içeren
  profil sonucunda `verified: false` bu geri okuma eksikliğini belirtir;
  sürücü başarısı ayrıca raporlanır.
- GPU güç tavanı okunuyor fakat yazılabilir güç sınırı sorgusu NVML
  `NOT_SUPPORTED` (3) döndürdü. GPU watt limiti kontrolü eklenmedi.
- Kurulu RyzenAdj 0.19.0, Dragon Range ailesini tanıdı fakat güç/izleme tablosunu
  başlatamadı (`request_table_ver_and_size is not supported on this family`).
  Curve Optimizer veya voltaj yazma/geri okuma doğrulaması yapılmadı.
- GPU VF derate sorgusu bu GPU için desteklenmiyor. Bu özellik voltaj ayarı
  değildir; NVIDIA belgesine göre Rubin ve sonrası mimariler içindir.

## İşlem ve yaşam döngüsü

CPU ayarları ve bu servisin GPU istekleri birleşik profil geri yüklemesine
katılır. GPU işlemi sonda uygulanır. Hata durumunda geri yükleme başarısı veya
hatası bildirilir. Aynı ayarları değiştiren başka araçlarla eşzamanlı kullanım
 desteklenmiş bir senaryo değildir; dış araçların GPU kilidi okunamaz.

GPU kilidi bu servis tarafından ayarlandıysa sahiplik işareti
`/var/lib/slayer-r9t/gpu-clock-owned` altında tutulur. Normal servis kapanışında
GPU frekansları otomatiğe döndürülür. İşaret kalırsa bir sonraki servis
başlangıcında otomatik frekans komutu gönderilir. Bu davranışın beklenmeyen
kapanış senaryosu bu sürümde canlı olarak sınanmadı. CPU sınırları servis
kapanışında değiştirilmez. Arayüzü kapatmak servisi veya sınırları kapatmaz.

## Kontroller

21 birim kontrolü geçti. Dört hazır profil gerçek donanımda sırasıyla uygulandı;
CPU sınırları geri okundu, GPU komutları kabul edildi ve anlık GPU frekansları
raporlandı. Başlangıç güç/boost/CPU sınırı/GPU isteği geri yüklendi; RGB değişmedi.
Sonuçlar [cooling-validation.json](cooling-validation.json) dosyasında.
Yük altında sıcaklık düşüşü, FPS, uzun süreli kararlılık ve uyku/uyanma ölçülmedi.

```sh
R9T_LIVE_CHECK=1 PYTHONPATH=. python tests/live_cooling.py
```

## Kaynaklar

- [Linux AMD P-State belgeleri](https://www.kernel.org/doc/html/latest/admin-guide/pm/amd-pstate.html)
- [NVIDIA SMI: GPU clock locking, power limit ve VF derate](https://docs.nvidia.com/deploy/nvidia-smi/index.html)
- [RyzenAdj ve modele göre destek durumu](https://github.com/FlyGoat/RyzenAdj/wiki/Supported-Models)
