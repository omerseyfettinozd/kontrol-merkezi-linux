#!/usr/bin/env python3
"""Faz 2 kabulü (a, c, d): r9td gölge modu ile eski servisi yan yana kaydeder.

Root olarak geçici bir systemd biriminde çalışır (iki sokete de bağlanabilmek için):
    sudo systemd-run --unit=r9t-shadow-compare --property=RuntimeMaxSec=90000 \
        /usr/bin/python3 -I /yol/shadow-compare.py --out /var/lib/r9t/shadow-compare.jsonl
Özet:  python3 -I shadow-compare.py --summary /var/lib/r9t/shadow-compare.jsonl

Toleranslar (plan Faz 1/2): sıcaklık ±2 °C, fan modu ve duty aynı, RPM ±%10, NVML ±1 °C.
"""
import argparse
import json
import socket
import sys
import time

LEGACY = '/run/slayer-r9t/control.sock'
R9TD = '/run/r9td/r9td.sock'


def rpc(path, payload, timeout=10):
    with socket.socket(socket.AF_UNIX) as s:
        s.settimeout(timeout)
        s.connect(path)
        s.sendall((json.dumps(payload) + '\n').encode())
        buf = b''
        while not buf.endswith(b'\n'):
            chunk = s.recv(65536)
            if not chunk:
                break
            buf += chunk
    return json.loads(buf)


def sample():
    t0 = time.time()
    a = time.perf_counter()
    legacy = rpc(LEGACY, {'version': 1, 'id': 'shadow-compare', 'op': 'status'})
    legacy_ms = (time.perf_counter() - a) * 1000
    r9 = rpc(R9TD, {'jsonrpc': '2.0', 'id': 1, 'method': 'status'})['result']
    caps = rpc(R9TD, {'jsonrpc': '2.0', 'id': 2, 'method': 'capabilities'})['result']['metrics']
    lm = legacy['data']['fans']['measured'] or {}
    v = lambda f: f['value']
    return {
        't': round(t0, 1), 'legacy_ms': round(legacy_ms, 1),
        'legacy': {k: lm.get(k) for k in ('mode', 'cpu_temp', 'gpu_temp', 'cpu_pwm', 'gpu_pwm', 'cpu_rpm', 'gpu_rpm', 'lease_ms', 'error')},
        'r9td': {
            'cpu_ec': v(r9['cpu']['ec_temp_c']), 'gpu_ec': v(r9['gpu']['ec_temp_c']), 'gpu_nvml': v(r9['gpu']['temp_c']),
            'mode': v(r9['fans']['mode']), 'cpu_duty': v(r9['fans']['cpu_duty_pct']), 'gpu_duty': v(r9['fans']['gpu_duty_pct']),
            'cpu_rpm': v(r9['fans']['cpu_rpm']), 'gpu_rpm': v(r9['fans']['gpu_rpm']),
            'rpm_age_ms': r9['fans']['cpu_rpm']['age_ms'], 'temp_age_ms': r9['cpu']['ec_temp_c']['age_ms'],
        },
        'metrics': caps,
    }


def within(s):
    L, R = s['legacy'], s['r9td']
    mode = {0: 'auto', 1: 'manual', 2: 'boost'}.get(L['mode'])
    checks = {
        'cpu_temp': R['cpu_ec'] is not None and L['cpu_temp'] is not None and abs(R['cpu_ec'] - L['cpu_temp']) <= 2,
        'gpu_temp': R['gpu_ec'] is not None and L['gpu_temp'] is not None and abs(R['gpu_ec'] - L['gpu_temp']) <= 2,
        'mode': R['mode'] == mode,
        'duty': R['cpu_duty'] is not None and abs(R['cpu_duty'] - L['cpu_pwm'] // 2) <= 1 and abs(R['gpu_duty'] - L['gpu_pwm'] // 2) <= 1,
        # RPM is sampled every 30 s in shadow mode; a 10 % band applies to steady fans.
        'rpm': R['cpu_rpm'] is not None and L['cpu_rpm'] and abs(R['cpu_rpm'] - L['cpu_rpm']) / L['cpu_rpm'] <= 0.10,
        'nvml': R['gpu_nvml'] is None or abs(R['gpu_nvml'] - L['gpu_temp']) <= 2,
    }
    return checks


def summary(path):
    rows = [json.loads(l) for l in open(path) if l.strip() and not l.startswith('{"error"')]
    errors = sum(1 for l in open(path) if l.startswith('{"error"'))
    if not rows:
        print('kayıt yok')
        return
    keys = ['cpu_temp', 'gpu_temp', 'mode', 'duty', 'rpm', 'nvml']
    counts = {k: 0 for k in keys}
    all_ok = 0
    for r in rows:
        c = within(r)
        all_ok += all(c.values())
        for k in keys:
            counts[k] += c[k]
    n = len(rows)
    first, last = rows[0], rows[-1]
    hours = (last['t'] - first['t']) / 3600
    ec_rate = (last['metrics']['ec_ops'] - first['metrics']['ec_ops']) / max(1, last['t'] - first['t'])
    lat = sorted(r['legacy_ms'] for r in rows)
    lease_zero_manual = sum(1 for r in rows if r['legacy']['mode'] and r['legacy']['mode'] != 0 and r['legacy']['lease_ms'] == 0)
    err110 = sum(1 for r in rows if r['legacy']['error'] == -110)
    out = {
        'samples': n, 'hours': round(hours, 2), 'request_errors': errors,
        'all_checks_pct': round(100 * all_ok / n, 2),
        'per_check_pct': {k: round(100 * counts[k] / n, 2) for k in keys},
        'r9td_ec_ops_per_s': round(ec_rate, 2),
        'legacy_status_ms': {'median': lat[n // 2], 'p90': lat[int(n * 0.9)], 'max': lat[-1]},
        'legacy_lease_zero_while_manual': lease_zero_manual, 'legacy_error_minus110': err110,
        'nvml_calls': last['metrics']['nvml_calls'],
    }
    print(json.dumps(out, ensure_ascii=False, indent=2))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--out')
    ap.add_argument('--interval', type=float, default=60)
    ap.add_argument('--summary')
    a = ap.parse_args()
    if a.summary:
        return summary(a.summary)
    while True:
        try:
            line = json.dumps(sample())
        except Exception as exc:  # keep logging; failures are counted in the summary
            line = json.dumps({'error': f'{type(exc).__name__}: {exc}', 't': round(time.time(), 1)})
        with open(a.out, 'a') as f:
            f.write(line + '\n')
        time.sleep(a.interval)


if __name__ == '__main__':
    sys.exit(main())
