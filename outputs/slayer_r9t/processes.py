"""Read-only resource-usage sampler for processes.

Reads only the process name (``comm``), CPU ticks and RSS from /proc.
Command lines, environment, paths and user names are never read or stored.
No process control (signals/kill) exists in this module.
"""
import os
import shutil
import subprocess
import time
from pathlib import Path

PROC = Path('/proc')
PCI = Path('/sys/bus/pci/devices')
NVIDIA_VENDOR = '0x10de'


def _clk_tck():
    try:
        return os.sysconf('SC_CLK_TCK')
    except (ValueError, OSError):
        return 100


def _page_size():
    try:
        return os.sysconf('SC_PAGE_SIZE')
    except (ValueError, OSError):
        return 4096


def parse_stat(text):
    """Return (comm, utime+stime ticks) from /proc/PID/stat text, or None."""
    try:
        left = text.index('(')
        right = text.rindex(')')
        comm = text[left + 1:right]
        fields = text[right + 2:].split()
        # fields[0] is state (field 3); utime=14 -> idx 11, stime=15 -> idx 12
        return comm, int(fields[11]) + int(fields[12])
    except (ValueError, IndexError):
        return None


def parse_statm_rss(text, page_size):
    try:
        return int(text.split()[1]) * page_size
    except (ValueError, IndexError):
        return None


class ProcessSampler:
    """Takes snapshots; CPU% is computed between consecutive samples."""

    def __init__(self, proc=None, pci=None, limit=8, clock=time.monotonic,
                 wall=time.time, nvidia_smi=None, run=subprocess.run):
        self.proc = Path(proc) if proc is not None else PROC
        self.pci = Path(pci) if pci is not None else PCI
        self.limit = max(1, int(limit))
        self.clock = clock
        self.wall = wall
        self.nvidia_smi = nvidia_smi
        self.run = run
        self.tck = _clk_tck()
        self.page = _page_size()
        self._prev = {}
        self._prev_time = None

    def _gpu_active(self):
        """True only if an NVIDIA GPU is already runtime-active (never wakes it)."""
        try:
            devices = list(self.pci.iterdir())
        except OSError:
            return None
        found = False
        for dev in devices:
            try:
                if (dev / 'vendor').read_text().strip().lower() != NVIDIA_VENDOR:
                    continue
                cls = (dev / 'class').read_text().strip()
                if not cls.startswith('0x03'):
                    continue
                found = True
                if (dev / 'power/runtime_status').read_text().strip() == 'active':
                    return True
            except OSError:
                continue
        return False if found else None

    def _gpu_processes(self, names):
        state = self._gpu_active()
        if state is None:
            return [], 'NVIDIA GPU bulunamadı'
        if not state:
            return [], 'GPU uykuda; uyandırmamak için sorgulanmadı'
        exe = self.nvidia_smi or shutil.which('nvidia-smi')
        if not exe:
            return [], 'nvidia-smi bulunamadı'
        try:
            res = self.run([exe, '--query-compute-apps=pid,used_memory',
                            '--format=csv,noheader,nounits'],
                           capture_output=True, text=True, timeout=5)
        except (OSError, subprocess.SubprocessError):
            return [], 'nvidia-smi çalıştırılamadı'
        if res.returncode != 0:
            return [], 'nvidia-smi hata verdi'
        rows = []
        for line in res.stdout.splitlines():
            parts = [p.strip() for p in line.split(',')]
            if len(parts) < 2:
                continue
            try:
                pid, mem = int(parts[0]), int(parts[1])
            except ValueError:
                continue
            rows.append({'pid': pid, 'name': names.get(pid) or self._comm(pid) or '?',
                         'vram_mib': mem})
        rows.sort(key=lambda r: r['vram_mib'], reverse=True)
        return rows[:self.limit], 'yalnız compute süreçleri listelenir (grafik süreçleri görünmeyebilir)'

    def _comm(self, pid):
        try:
            return (self.proc / str(pid) / 'comm').read_text().strip()
        except OSError:
            return None

    def sample(self):
        now = self.clock()
        stamp = self.wall()
        entries = {}
        unreadable = 0
        try:
            listing = list(self.proc.iterdir())
        except OSError:
            return {'timestamp': stamp, 'interval_s': None, 'cpu': [], 'memory': [],
                    'gpu': [], 'scanned': 0, 'unreadable': 0, 'gpu_note': '',
                    'error': '/proc okunamadı', 'limits': self._limits(0)}
        for entry in listing:
            if not entry.name.isdigit():
                continue
            pid = int(entry.name)
            try:
                stat = parse_stat((entry / 'stat').read_text())
                rss = parse_statm_rss((entry / 'statm').read_text(), self.page)
            except OSError:
                unreadable += 1  # exited meanwhile or permission denied
                continue
            if stat is None or rss is None:
                unreadable += 1
                continue
            entries[pid] = (stat[0], stat[1], rss)
        interval = None if self._prev_time is None else now - self._prev_time
        rows = []
        for pid, (comm, ticks, rss) in entries.items():
            cpu = None
            prev = self._prev.get(pid)
            if interval and interval > 0 and prev and prev[0] == comm and ticks >= prev[1]:
                cpu = (ticks - prev[1]) / self.tck / interval * 100.0
            rows.append({'pid': pid, 'name': comm, 'cpu_percent': cpu, 'rss_bytes': rss})
        self._prev = {pid: (v[0], v[1]) for pid, v in entries.items()}
        self._prev_time = now
        cpu_rows = sorted((r for r in rows if r['cpu_percent'] is not None),
                          key=lambda r: r['cpu_percent'], reverse=True)[:self.limit]
        mem_rows = sorted(rows, key=lambda r: r['rss_bytes'], reverse=True)[:self.limit]
        names = {r['pid']: r['name'] for r in rows}
        gpu_rows, gpu_note = self._gpu_processes(names)
        return {'timestamp': stamp, 'interval_s': interval,
                'cpu': cpu_rows, 'memory': mem_rows, 'gpu': gpu_rows, 'gpu_note': gpu_note,
                'scanned': len(entries), 'unreadable': unreadable, 'error': None,
                'limits': self._limits(unreadable)}

    @staticmethod
    def _limits(unreadable):
        text = 'Yalnız süreç adı, CPU% ve RSS okunur; komut satırı toplanmaz. Salt okunur.'
        if unreadable:
            text += f' {unreadable} süreç okunamadı (çıkmış ya da yetki yok).'
        return text


def format_bytes(value):
    value = float(value)
    for unit in ('B', 'KiB', 'MiB', 'GiB'):
        if value < 1024 or unit == 'GiB':
            return f'{value:.0f} {unit}' if unit == 'B' else f'{value:.1f} {unit}'
        value /= 1024
