#!/usr/bin/env python3
"""Faz 0.4 ölçümü: spike'ı dört modda çalıştırır, RSS / CPU / NVIDIA etkisini ve FPS'i kaydeder.

Kullanım: python3 -I measure.py [--seconds 30] [--warmup 8] [--out sonuc.json]
Pencere ekranda görünür olmalıdır (gizli pencerede WebKit rAF'yi yavaşlatır).
"""
import argparse
import json
import os
import re
import signal
import subprocess
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
BINARY = HERE / 'src-tauri/target/release/r9t-tauri-canvas-spike'
CLK = os.sysconf('SC_CLK_TCK')
PAGE = os.sysconf('SC_PAGE_SIZE')


def descendants(root):
    children = {}
    for stat in Path('/proc').glob('[0-9]*/stat'):
        try:
            text = stat.read_text()
        except OSError:
            continue
        pid = int(stat.parent.name)
        ppid = int(text.rsplit(')', 1)[1].split()[1])
        children.setdefault(ppid, []).append(pid)
    found, stack = [], [root]
    while stack:
        pid = stack.pop()
        found.append(pid)
        stack.extend(children.get(pid, []))
    return found


def proc_sample(pids):
    cpu, rss, names = 0, 0, {}
    for pid in pids:
        try:
            fields = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
            cpu += int(fields[11]) + int(fields[12])
            rss += int(Path(f'/proc/{pid}/statm').read_text().split()[1]) * PAGE
            names[pid] = Path(f'/proc/{pid}/comm').read_text().strip()
        except OSError:
            pass
    return cpu / CLK, rss, names


def gpu_processes():
    """nvidia-smi -q -d PIDS çıktısından {pid: (tür, MiB)}."""
    out = subprocess.run(['nvidia-smi', '-q', '-d', 'PIDS'], capture_output=True, text=True, timeout=10).stdout
    result, pid, kind = {}, None, None
    for line in out.splitlines():
        line = line.strip()
        if line.startswith('Process ID'):
            pid = int(line.split(':')[1])
        elif line.startswith('Type') and pid is not None:
            kind = line.split(':')[1].strip()
        elif line.startswith('Used GPU Memory') and pid is not None:
            m = re.search(r'(\d+)\s*MiB', line)
            result[pid] = (kind, int(m.group(1)) if m else None)
            pid = None
    return result


def gpu_util():
    out = subprocess.run(['nvidia-smi', '--query-gpu=utilization.gpu,power.draw', '--format=csv,noheader,nounits'],
                         capture_output=True, text=True, timeout=10).stdout.strip()
    util, power = (x.strip() for x in out.split(','))
    return int(util), float(power)


def baseline(seconds):
    samples = []
    for _ in range(seconds):
        samples.append(gpu_util())
        time.sleep(1)
    return {'gpu_util_mean': round(sum(s[0] for s in samples) / len(samples), 1),
            'gpu_power_mean_w': round(sum(s[1] for s in samples) / len(samples), 1)}


CONFIGS = {
    'varsayılan': {},
    'nv_explicit_sync_kapalı': {'__NV_DISABLE_EXPLICIT_SYNC': '1'},
    'dmabuf_kapalı': {'WEBKIT_DISABLE_DMABUF_RENDERER': '1'},
}


def run(mode, animate, extra_env, seconds, warmup):
    env = dict(os.environ, SPIKE_ANIMATE='1' if animate else '0')
    for key in ('WEBKIT_DISABLE_DMABUF_RENDERER', '__NV_DISABLE_EXPLICIT_SYNC'):
        env.pop(key, None)
    env.update(extra_env)
    proc = subprocess.Popen([str(BINARY)], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    lines = []
    reader = threading.Thread(target=lambda: lines.extend(iter(proc.stdout.readline, '')), daemon=True)
    reader.start()
    try:
        time.sleep(warmup)
        if proc.poll() is not None:
            reader.join(timeout=2)
            return {'mode': mode, 'env': extra_env, 'crashed': True, 'returncode': proc.returncode,
                    'output': [l.strip() for l in lines if 'appindicator' not in l][:20]}
        pids = descendants(proc.pid)
        cpu0, _, _ = proc_sample(pids)
        t0 = time.monotonic()
        util, rss_peak = [], 0
        while time.monotonic() - t0 < seconds and proc.poll() is None:
            util.append(gpu_util())
            pids = descendants(proc.pid)
            rss_peak = max(rss_peak, proc_sample(pids)[1])
            time.sleep(1)
        cpu1, rss, names = proc_sample(pids)
        elapsed = time.monotonic() - t0
        gproc = gpu_processes()
        ours = {names.get(p, str(p)): {'type': gproc[p][0], 'mib': gproc[p][1]} for p in pids if p in gproc}
    finally:
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
    reports = [l.strip() for l in lines if l.startswith('SPIKE t=')]
    fps = [float(re.search(r'fps=([\d.]+)', l).group(1)) for l in reports]
    p99 = [float(re.search(r'p99_ms=([\d.]+)', l).group(1)) for l in reports]
    return {'mode': mode, 'animate': animate, 'env': extra_env, 'crashed': proc.returncode not in (None, 0, -15),
            'processes': sorted(set(names.values())),
            'rss_mib': round(rss / 2**20, 1), 'rss_peak_mib': round(rss_peak / 2**20, 1),
            'cpu_percent_of_one_core': round(100 * (cpu1 - cpu0) / elapsed, 2),
            'gpu_util_mean': round(sum(u[0] for u in util) / len(util), 1),
            'gpu_power_mean_w': round(sum(u[1] for u in util) / len(util), 1),
            'nvidia_processes': ours,
            'fps_reports': fps, 'p99_ms_reports': p99,
            'other_output': [l.strip() for l in lines if not l.startswith('SPIKE')][:20]}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--seconds', type=int, default=30)
    parser.add_argument('--warmup', type=int, default=8)
    parser.add_argument('--out', default=str(HERE / 'measure-result.json'))
    args = parser.parse_args()
    if not BINARY.exists():
        raise SystemExit(f'Önce derleyin: {BINARY}')
    result = {'generated_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
              'desktop': os.environ.get('XDG_CURRENT_DESKTOP'), 'baseline_without_app': baseline(10), 'runs': []}
    result['power_profile'] = subprocess.run(['powerprofilesctl', 'get'], capture_output=True, text=True).stdout.strip()
    for name, extra in CONFIGS.items():
        for animate in (False, True):
            mode = f"{name} / {'animasyon' if animate else 'boşta'}"
            print('Ölçülüyor:', mode, flush=True)
            entry = run(mode, animate, extra, args.seconds, args.warmup)
            result['runs'].append(entry)
            time.sleep(3)
            if entry.get('crashed') and not animate:
                break
    Path(args.out).write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    print('Yazıldı:', args.out)


if __name__ == '__main__':
    main()
