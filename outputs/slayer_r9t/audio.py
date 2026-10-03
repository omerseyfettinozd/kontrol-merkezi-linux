"""Audio/microphone control via WirePlumber `wpctl` (user session only).

Every write is followed by a read-back through wpctl; a mismatch raises
AudioReadbackError. Muting a microphone is a software mute in PipeWire, not a
hardware disconnect.
"""
import re
import shutil
import subprocess
from dataclasses import dataclass, field

TARGETS = {'sink': '@DEFAULT_AUDIO_SINK@', 'source': '@DEFAULT_AUDIO_SOURCE@'}
MAX_PERCENT = 150
TOLERANCE = 1  # percentage points
TIMEOUT = 5


class AudioError(Exception):
    pass


class AudioUnavailable(AudioError):
    pass


class AudioValidationError(AudioError, ValueError):
    pass


class AudioReadbackError(AudioError):
    pass


@dataclass
class AudioDevice:
    id: int
    name: str
    default: bool = False
    volume: float = None
    muted: bool = False


@dataclass
class AudioState:
    sinks: list = field(default_factory=list)
    sources: list = field(default_factory=list)

    def default(self, kind):
        for d in (self.sinks if kind == 'sink' else self.sources):
            if d.default:
                return d
        return None


_DEV = re.compile(r'^[\s│├└─]*(\*)?\s*(\d+)\.\s+(.*?)\s*(?:\[vol:\s*([\d.]+)(\s+MUTED)?\])?\s*$')


def parse_status(text):
    """Parse `wpctl status` output into AudioState (Sinks/Sources sections)."""
    state, section, top = AudioState(), None, None
    for raw in text.splitlines():
        line = raw.replace('\r', '')
        t = re.match(r'^([A-Za-z]+)\s*$', line)
        if t:
            top, section = t.group(1), None
            continue
        if top != 'Audio':
            continue
        m = re.match(r'^[\s│├└─]*(Sinks|Sources|Devices|Filters|Streams|Sink endpoints|Source endpoints):', line)
        if m:
            section = m.group(1)
            continue
        if section not in ('Sinks', 'Sources'):
            continue
        d = _DEV.match(line)
        if not d:
            continue
        dev = AudioDevice(int(d.group(2)), d.group(3).strip(), bool(d.group(1)),
                          float(d.group(4)) if d.group(4) else None, bool(d.group(5)))
        (state.sinks if section == 'Sinks' else state.sources).append(dev)
    return state


def parse_volume(text):
    """Parse `wpctl get-volume` -> (percent:int, muted:bool)."""
    m = re.search(r'Volume:\s*([\d.]+)(\s+\[MUTED\])?', text)
    if not m:
        raise AudioError('wpctl get-volume çıktısı anlaşılamadı')
    return round(float(m.group(1)) * 100), bool(m.group(2))


def validate_percent(value):
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise AudioValidationError('Ses düzeyi sayı olmalı')
    if value != value or not 0 <= value <= MAX_PERCENT:
        raise AudioValidationError(f'Ses düzeyi 0–{MAX_PERCENT} arasında olmalı')
    return float(value)


def _target(kind):
    if kind not in TARGETS:
        raise AudioValidationError("Hedef 'sink' veya 'source' olmalı")
    return TARGETS[kind]


def _run(args):
    try:
        p = subprocess.run(['wpctl', *args], capture_output=True, text=True, timeout=TIMEOUT)
    except FileNotFoundError:
        raise AudioUnavailable('wpctl bulunamadı')
    except subprocess.TimeoutExpired:
        raise AudioUnavailable('wpctl zaman aşımına uğradı')
    return p.returncode, p.stdout, p.stderr


class AudioBackend:
    """`runner(args:list)->(returncode, stdout, stderr)` is injectable for tests."""

    def __init__(self, runner=None):
        self._runner = runner or _run

    def _wpctl(self, *args):
        rc, out, err = self._runner(list(args))
        if rc != 0:
            raise AudioError(f'wpctl {args[0]} başarısız: {(err or out).strip()[:200]}')
        return out

    def availability(self):
        """Return (ok, reason)."""
        if self._runner is _run and not shutil.which('wpctl'):
            return False, 'wpctl bulunamadı; ses kontrolleri kapalı'
        try:
            self._wpctl('status')
        except AudioUnavailable as e:
            return False, f'{e}; ses kontrolleri kapalı'
        except AudioError:
            return False, 'WirePlumber/PipeWire çalışmıyor; ses kontrolleri kapalı'
        return True, ''

    def status(self):
        return parse_status(self._wpctl('status'))

    def get(self, kind):
        return parse_volume(self._wpctl('get-volume', _target(kind)))

    def set_volume(self, kind, percent):
        pct = validate_percent(percent)
        target = _target(kind)
        self._wpctl('set-volume', '-l', '1.5', target, f'{pct / 100:.2f}')
        got, _ = self.get(kind)
        if abs(got - pct) > TOLERANCE:
            raise AudioReadbackError(f'Ses düzeyi doğrulanamadı: istenen %{pct:.0f}, okunan %{got}')
        return got

    def set_mute(self, kind, muted):
        if not isinstance(muted, bool):
            raise AudioValidationError('Sessize alma değeri bool olmalı')
        self._wpctl('set-mute', _target(kind), '1' if muted else '0')
        _, got = self.get(kind)
        if got != muted:
            raise AudioReadbackError('Sessize alma durumu doğrulanamadı')
        return got

    def set_default(self, kind, device_id):
        _target(kind)
        if isinstance(device_id, bool) or not isinstance(device_id, int) or device_id <= 0:
            raise AudioValidationError('Geçersiz aygıt kimliği')
        state = self.status()
        if device_id not in [d.id for d in (state.sinks if kind == 'sink' else state.sources)]:
            raise AudioValidationError('Aygıt listede yok')
        self._wpctl('set-default', str(device_id))
        cur = self.status().default(kind)
        if cur is None or cur.id != device_id:
            raise AudioReadbackError('Varsayılan aygıt doğrulanamadı')
        return cur
