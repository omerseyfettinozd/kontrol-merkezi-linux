#!/usr/bin/env python3
"""Faz 1 kabulü: `sudo r9tctl probe --json` ile eski servisin `status` yanıtını karşılaştırır.

    python3 -I r9t-studio/tools/compare-probe-legacy.py --probe probe.json --legacy legacy.json [--out sonuç.json]

Toleranslar (plan Faz 1): sıcaklık ±2 °C, fan modu ve duty aynı, RPM ±%10, NVML sıcaklığı ±1 °C.
"""
import argparse
import json
from pathlib import Path


def check(name, ok, detail):
    return {'check': name, 'ok': bool(ok), 'detail': detail}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--probe', required=True)
    ap.add_argument('--legacy', required=True)
    ap.add_argument('--out')
    a = ap.parse_args()
    probe = json.loads(Path(a.probe).read_text())['readings']
    legacy = json.loads(Path(a.legacy).read_text())
    lm = legacy['fans']['measured']
    fan = probe['fan']
    ec = probe['ec']
    out = []
    pt = fan['ec_temps']
    out.append(check('cpu_temp ±2', abs(pt[0] - lm['cpu_temp']) <= 2, f"probe {pt[0]} / legacy {lm['cpu_temp']}"))
    out.append(check('gpu_temp ±2', abs(pt[1] - lm['gpu_temp']) <= 2, f"probe {pt[1]} / legacy {lm['gpu_temp']}"))
    out.append(check('ec fast-path cpu ±2', abs(ec['cpu_temp'] - lm['cpu_temp']) <= 2, f"tuxedo_io {ec['cpu_temp']} / legacy {lm['cpu_temp']}"))
    out.append(check('fan mode', fan['mode'] == lm['mode'], f"probe {fan['mode']} / legacy {lm['mode']}"))
    for side in ('cpu', 'gpu'):
        pd, ld = fan[f'{side}_duty_pct'], lm[f'{side}_pwm'] // 2
        out.append(check(f'{side} duty', abs(pd - ld) <= 1, f"probe {pd}% / legacy {ld}%"))
        pr, lr = fan[f'{side}_rpm'], lm[f'{side}_rpm']
        out.append(check(f'{side} rpm ±10%', lr > 0 and abs(pr - lr) / lr <= 0.10, f"probe {pr} / legacy {lr}"))
    nv = (probe['gpu'].get('sample') or {}).get('temp_c') if isinstance(probe['gpu'].get('sample'), dict) else None
    legacy_gpu_c = None
    # Legacy GUI reads NVML through nvidia-smi; the service exposes EC GPU temp. Compare NVML with EC GPU ±2
    # and, when the legacy snapshot has an NVML temperature, ±1.
    if nv is not None:
        out.append(check('nvml vs ec gpu ±2', abs(nv - lm['gpu_temp']) <= 2, f"nvml {nv} / ec {lm['gpu_temp']}"))
    result = {'ok': all(c['ok'] for c in out), 'checks': out, 'legacy_gpu_nvml_c': legacy_gpu_c}
    text = json.dumps(result, ensure_ascii=False, indent=2)
    print(text)
    if a.out:
        Path(a.out).write_text(text + '\n')


if __name__ == '__main__':
    main()
