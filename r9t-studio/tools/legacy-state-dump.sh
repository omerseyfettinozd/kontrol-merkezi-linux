#!/usr/bin/env bash
# Faz 0.3: eski servisin root durum dizinini SALT OKUNUR döker.
# Kullanım (depo kökünden ya da herhangi bir yerden):
#   sudo "r9t-studio/tools/legacy-state-dump.sh"
# Çıktı: r9t-studio/docs/legacy-state-dump.json (sahibi sudo'yu çağıran kullanıcı).
# Hiçbir dosyayı değiştirmez, servise komut göndermez, donanıma erişmez.
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    echo "Bu betik root ister: sudo $0" >&2
    exit 1
fi
if [[ -z "${SUDO_UID:-}" || "${SUDO_UID}" == 0 ]]; then
    echo "sudo ile normal kullanıcıdan çalıştırın (SUDO_UID gerekli)." >&2
    exit 1
fi

here="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
out="$here/../docs/legacy-state-dump.json"

/usr/bin/python3 -I - "$out" <<'PY'
import collections, json, os, stat, sys, time
from pathlib import Path

STATE = Path('/var/lib/slayer-r9t')
out = Path(sys.argv[1])
result = {'generated_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
          'state_dir': str(STATE), 'files': [], 'errors': {}}

def read_json(path):
    try:
        if path.stat().st_size > 300000:
            return {'error': 'dosya çok büyük, okunmadı'}
        return json.loads(path.read_text())
    except (OSError, ValueError) as exc:
        return {'error': f'{type(exc).__name__}: {exc}'}

try:
    info = STATE.stat()
    result['state_dir_mode'] = oct(stat.S_IMODE(info.st_mode))
    for entry in sorted(STATE.iterdir()):
        st = entry.lstat()
        result['files'].append({'name': entry.name, 'size': st.st_size,
                                'mode': oct(stat.S_IMODE(st.st_mode)),
                                'type': 'symlink' if entry.is_symlink() else 'dir' if entry.is_dir() else 'file',
                                'mtime_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(st.st_mtime))})
except OSError as exc:
    result['errors']['state_dir'] = str(exc)

policy = STATE/'policy.json'
if policy.is_file() and not policy.is_symlink():
    result['policy'] = read_json(policy)
result['automation_paused_file'] = (STATE/'policy.paused').exists()

recovery = STATE/'temperature-target-recovery.json'
result['temperature_target_recovery'] = read_json(recovery) if recovery.is_file() else None

marker = STATE/'gpu-clock-owned'
try:
    result['gpu_clock_owned'] = marker.read_text().strip()[:32] if marker.is_file() else None
except OSError as exc:
    result['errors']['gpu_clock_owned'] = str(exc)

# Denetim günlüğü: tam içerik yerine özet + son olaylar (uygulama yolu içerebilir).
events = []
for name in ('actions.jsonl.2', 'actions.jsonl.1', 'actions.jsonl'):
    path = STATE/name
    if not path.is_file() or path.is_symlink():
        continue
    try:
        for line in path.read_text(errors='replace').splitlines():
            try:
                value = json.loads(line)
            except ValueError:
                continue
            if isinstance(value, dict):
                events.append(value)
    except OSError as exc:
        result['errors'][name] = str(exc)
by_op = collections.Counter(e.get('operation') for e in events)
failed = collections.Counter(e.get('operation') for e in events if e.get('ok') is False)
durations = collections.defaultdict(list)
for e in events:
    if isinstance(e.get('duration_ms'), int):
        durations[e.get('operation')].append(e['duration_ms'])
result['actions'] = {
    'count': len(events),
    'first_utc': events[0].get('time') if events else None,
    'last_utc': events[-1].get('time') if events else None,
    'by_operation': dict(by_op.most_common()),
    'failed_by_operation': dict(failed.most_common()),
    'duration_ms_median': {op: sorted(v)[len(v)//2] for op, v in durations.items()},
    'duration_ms_max': {op: max(v) for op, v in durations.items()},
    'last_events': [{k: (str(e.get(k))[:200] if k == 'message' else e.get(k))
                     for k in ('time', 'operation', 'ok', 'duration_ms', 'message')} for e in events[-30:]],
}

out.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
os.chown(out, int(os.environ['SUDO_UID']), int(os.environ.get('SUDO_GID', os.environ['SUDO_UID'])))
os.chmod(out, 0o644)
print(f'Yazıldı: {out} ({len(events)} olay, {len(result["files"])} dosya)')
PY
