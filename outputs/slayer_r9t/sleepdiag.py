"""Read-only sleep / battery-drain diagnostics (before/after snapshots).

Never suspends the system and never writes to sysfs. Only a snapshot file in
the user's state directory is written (atomically).
"""
import json
import os
import tempfile
import time
from pathlib import Path

SYSFS_POWER_SUPPLY = Path('/sys/class/power_supply')
SYSFS_POWER = Path('/sys/power')
DEFAULT_PATH = Path.home() / '.local/state/slayer-r9t-control-center/sleepdiag.json'
WARNING = ('Bu araç uykuyu başlatmaz ve fiziksel uyku testi yapılmış sayılmaz. '
           'Şarj/priz durumu değişirse sonuç geçersizdir.')


def _read(path):
    try:
        return Path(path).read_text().strip()
    except (OSError, UnicodeDecodeError):
        return None


def _num(path):
    value = _read(path)
    try:
        return int(value) if value is not None else None
    except ValueError:
        return None


def read_battery(root=SYSFS_POWER_SUPPLY):
    """Return first battery's energy (µWh), power (µW), status, and AC state."""
    root = Path(root)
    result = {'energy_uwh': None, 'power_uw': None, 'status': None, 'capacity': None,
              'ac_online': None, 'battery': None}
    try:
        entries = sorted(root.iterdir())
    except OSError:
        return result
    for entry in entries:
        kind = _read(entry / 'type')
        if kind == 'Mains' or kind == 'USB':
            online = _num(entry / 'online')
            if online is not None:
                result['ac_online'] = bool(online) or bool(result['ac_online'])
        elif kind == 'Battery' and result['battery'] is None:
            result['battery'] = entry.name
            result['status'] = _read(entry / 'status')
            result['capacity'] = _num(entry / 'capacity')
            volt = _num(entry / 'voltage_now')
            energy = _num(entry / 'energy_now')
            if energy is None:
                charge = _num(entry / 'charge_now')
                if charge is not None and volt:
                    energy = charge * volt // 1_000_000
            power = _num(entry / 'power_now')
            if power is None:
                cur = _num(entry / 'current_now')
                if cur is not None and volt:
                    power = abs(cur) * volt // 1_000_000
            result['energy_uwh'], result['power_uw'] = energy, power
    return result


def read_suspend_stats(root=SYSFS_POWER):
    root = Path(root)
    stats = root / 'suspend_stats'
    out = {'success': _num(stats / 'success'), 'fail': _num(stats / 'fail'),
           'last_failed_dev': _read(stats / 'last_failed_dev') or None,
           'last_failed_errno': _read(stats / 'last_failed_errno') or None,
           'mem_sleep': _read(root / 'mem_sleep')}
    return out


def take_snapshot(ps_root=SYSFS_POWER_SUPPLY, power_root=SYSFS_POWER, now=None):
    snap = {'time': time.time() if now is None else now}
    snap.update(read_battery(ps_root))
    snap['stats'] = read_suspend_stats(power_root)
    return snap


def save_snapshot(snap, path=DEFAULT_PATH):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=path.parent, prefix='.sleepdiag.')
    try:
        with os.fdopen(fd, 'w') as handle:
            json.dump(snap, handle)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(tmp, path)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


def load_snapshot(path=DEFAULT_PATH):
    try:
        data = json.loads(Path(path).read_text())
    except (OSError, ValueError):
        return None
    return data if isinstance(data, dict) and isinstance(data.get('time'), (int, float)) else None


def compare(before, after):
    """Compare snapshots. Missing values stay None; `warnings` explain invalidity."""
    res = {'duration_s': None, 'energy_delta_uwh': None, 'loss_uwh_per_h': None,
           'suspend_delta': None, 'fail_delta': None, 'valid': True, 'warnings': [WARNING]}
    dur = after['time'] - before['time']
    if dur <= 0:
        res['valid'] = False
        res['warnings'].append('Saat geriye gitti veya süre sıfır; sonuç geçersiz.')
    else:
        res['duration_s'] = dur
    if before.get('ac_online') or after.get('ac_online'):
        res['valid'] = False
        res['warnings'].append('Priz bağlıydı veya değişti; sonuç geçersiz.')
    for snap in (before, after):
        if snap.get('status') in ('Charging', 'Full'):
            res['valid'] = False
            res['warnings'].append('Pil şarj oluyordu; sonuç geçersiz.')
            break
    if before.get('ac_online') is None or after.get('ac_online') is None:
        res['warnings'].append('Priz durumu okunamadı; geçerlilik doğrulanamadı.')
    e0, e1 = before.get('energy_uwh'), after.get('energy_uwh')
    if e0 is not None and e1 is not None:
        res['energy_delta_uwh'] = e1 - e0
        if res['duration_s']:
            res['loss_uwh_per_h'] = (e0 - e1) / (res['duration_s'] / 3600)
    else:
        res['warnings'].append('Pil enerjisi okunamadı.')
    s0, s1 = before.get('stats') or {}, after.get('stats') or {}
    if s0.get('success') is not None and s1.get('success') is not None:
        res['suspend_delta'] = s1['success'] - s0['success']
        if res['suspend_delta'] == 0:
            res['warnings'].append('Uyku sayacı artmadı; sistem uyumamış olabilir.')
    if s0.get('fail') is not None and s1.get('fail') is not None:
        res['fail_delta'] = s1['fail'] - s0['fail']
    return res
