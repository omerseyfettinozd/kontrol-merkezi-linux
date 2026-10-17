#!/usr/bin/env python3
"""Faz 0.7: duty → RPM eğrisi (%50–100, %10 adım) eski servisin RPC'si üzerinden.

Kullanıcı olarak çalışır (eski servis soketi kullanıcıya aittir; sudo gerekmez):
    python3 -I r9t-studio/tools/fan-rpm-sweep.py [--step-seconds 24]

Güvenlik:
- Ön koşul: AC bağlı, CPU EC < 75 °C ve GPU EC < 70 °C (plandaki 60 °C'den gevşetildi:
  tarama yalnız %50–100 yazar, auto'daki ~%53'ten az soğutmaz; docs/07 gerekçesi).
- Durdurma: CPU ≥ 80 °C, GPU ≥ 75 °C, art arda 2 örnekte RPM < 500, ya da status hatası.
- Her çıkışta (hata, Ctrl-C dahil) `auto` gönderilir, mode=0 ve mode_byte & 0x40 == 0 geri okunur.
- Otomasyon başlangıçta duraklatılmamışsa duraklatılır ve sonunda geri açılır.
Çıktı: r9t-studio/docs/07_fan_rpm_egrisi.json
"""
import argparse
import json
import signal
import statistics
import sys
import time
from pathlib import Path

sys.path.insert(0, '/usr/local/lib/slayer-r9t')
from slayer_r9t.protocol import request  # noqa: E402

OUT = Path(__file__).resolve().parent.parent / 'docs/07_fan_rpm_egrisi.json'


def status():
    state = request({'op': 'status'})
    measured = state['fans'].get('measured')
    if not measured:
        raise RuntimeError('Fan ölçümü yok: ' + str(state['fans'].get('error')))
    return state, measured


def ac_online():
    for supply in Path('/sys/class/power_supply').iterdir():
        try:
            if (supply / 'type').read_text().strip() == 'Mains' and (supply / 'online').read_text().strip() == '1':
                return True
        except OSError:
            pass
    return False


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--step-seconds', type=int, default=24)
    args = parser.parse_args()
    signal.signal(signal.SIGTERM, lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))

    state, m = status()
    if not ac_online():
        raise SystemExit('AC bağlı değil.')
    if m['cpu_temp'] >= 75 or m['gpu_temp'] >= 70:
        raise SystemExit(f"Sıcaklık yüksek: CPU {m['cpu_temp']} GPU {m['gpu_temp']}")
    was_paused = state['automation']['paused']
    result = {'generated_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
              'module_version': Path('/sys/module/r9t_fan/version').read_text().strip()
              if Path('/sys/module/r9t_fan/version').exists() else None,
              'precondition': {'cpu_temp': m['cpu_temp'], 'gpu_temp': m['gpu_temp'], 'auto_cpu_pwm': m['cpu_pwm'],
                               'auto_cpu_rpm': m['cpu_rpm'], 'auto_gpu_rpm': m['gpu_rpm']},
              'step_seconds': args.step_seconds, 'steps': [], 'stopped': None}
    low_streak = 0
    try:
        if not was_paused:
            request({'op': 'automation_pause'})
        if state['cooling']['temperature_target']['active']:
            request({'op': 'temperature_target_stop'})
        for duty in range(50, 101, 10):
            request({'op': 'manual', 'cpu': duty, 'gpu': duty})
            start = time.monotonic()
            samples = []
            while time.monotonic() - start < args.step_seconds:
                time.sleep(2)
                _, m = status()
                if m['cpu_temp'] >= 80 or m['gpu_temp'] >= 75:
                    raise RuntimeError(f"Sıcaklık sınırı: CPU {m['cpu_temp']} GPU {m['gpu_temp']}")
                if min(m['cpu_rpm'], m['gpu_rpm']) < 500:
                    low_streak += 1
                    if low_streak >= 2:
                        raise RuntimeError(f'RPM < 500 (duty {duty}): {m}')
                else:
                    low_streak = 0
                # İlk 8 sn fan hızlanması; sabit duruma gelen örnekler kaydedilir.
                if time.monotonic() - start >= 8:
                    samples.append({k: m[k] for k in ('cpu_rpm', 'gpu_rpm', 'cpu_pwm', 'gpu_pwm', 'cpu_temp', 'gpu_temp', 'mode')})
            if len(samples) < 3:
                raise RuntimeError(f'%{duty} için yeterli örnek yok')
            step = {'duty': duty, 'samples': samples,
                    'cpu_rpm_median': statistics.median(s['cpu_rpm'] for s in samples),
                    'gpu_rpm_median': statistics.median(s['gpu_rpm'] for s in samples),
                    'cpu_rpm_min': min(s['cpu_rpm'] for s in samples),
                    'gpu_rpm_min': min(s['gpu_rpm'] for s in samples)}
            result['steps'].append(step)
            print(f"%{duty}: CPU {step['cpu_rpm_median']} rpm, GPU {step['gpu_rpm_median']} rpm, "
                  f"T {samples[-1]['cpu_temp']}/{samples[-1]['gpu_temp']} °C", flush=True)
    except (RuntimeError, KeyboardInterrupt, OSError) as exc:
        result['stopped'] = f'{type(exc).__name__}: {exc}'
        print('Durduruldu:', result['stopped'], flush=True)
    finally:
        try:
            request({'op': 'auto'})
            time.sleep(2)
            _, m = status()
            result['final'] = {'mode': m['mode'], 'mode_byte': m['mode_byte'],
                               'auto_verified': m['mode'] == 0 and not m['mode_byte'] & 0x40}
        except Exception as exc:  # son çare: köprü üzerinden yeniden dene
            result['final'] = {'error': str(exc)}
        if not was_paused:
            request({'op': 'automation_resume'})
        OUT.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
        print('Son durum:', result['final'], '→', OUT)


if __name__ == '__main__':
    main()
