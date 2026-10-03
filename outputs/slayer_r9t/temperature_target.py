"""Opt-in temperature feedback, using only existing fan/clock interfaces."""
import json
import os
from pathlib import Path
import time
from . import cooling
from .fans import CurveSmoother

JOURNAL = Path('/var/lib/slayer-r9t/temperature-target-recovery.json')


def validate_targets(cpu, gpu):
    if type(cpu) is not int or type(gpu) is not int or not 60 <= cpu <= 85 or not 60 <= gpu <= 80:
        raise ValueError('CPU hedefi 60–85°C, GPU hedefi 60–80°C olmalı.')


def recover_cpu(path=JOURNAL):
    """Recover only fixed cpufreq policy files; never accept arbitrary paths."""
    if not path.exists():
        return
    data = json.loads(path.read_text())
    if not isinstance(data, dict) or not data or len(data) > 256:
        raise RuntimeError('Sıcaklık hedefi geri dönüş kaydı geçersiz.')
    snapshot = []
    for name, value in data.items():
        if not name.startswith('policy') or not name[6:].isdigit() or type(value) is not int or not 1 <= value <= 10000000:
            raise RuntimeError('Sıcaklık hedefi geri dönüş kaydı geçersiz.')
        snapshot.append((cooling.CPU_ROOT/name/'scaling_max_freq', str(value)))
    cooling.restore_cpu(snapshot)
    path.unlink()


class TemperatureTarget:
    def __init__(self, fans, gpu, journal=JOURNAL):
        self.fans, self.gpu, self.journal = fans, gpu, journal
        self.active = False
        self.saved = None
        self.error = ''
        self.last_tick = None
        self.cool_since = [None, None]
        self.targets = None
        self.limits = None
        self.recovery_error = ''
        try:
            recover_cpu(journal)
        except (OSError, ValueError, RuntimeError) as exc:
            self.recovery_error = str(exc)
            self.error = 'Önceki CPU sınırları geri yüklenemedi: ' + str(exc)

    @staticmethod
    def temperatures(state):
        measured = state.get('measured') or {}
        values = [measured.get('cpu_temp'), measured.get('gpu_temp')]
        if not state.get('available') or any(type(v) is not int or not 1 <= v <= 115 for v in values):
            raise RuntimeError('Güncel CPU/GPU sıcaklık ölçümü yok.')
        return values

    def start(self, cpu, gpu):
        validate_targets(cpu, gpu)
        if self.recovery_error:
            raise RuntimeError(self.error)
        if self.saved is not None:
            self.stop()
        state = self.fans.status()
        measured = self.temperatures(state)
        cpu_state, gpu_state = cooling.cpu_status(), self.gpu.status()
        if not cpu_state or not gpu_state.get('available') or 'max_mhz' not in gpu_state:
            raise RuntimeError('Fan, CPU ve GPU frekans arayüzleri birlikte hazır olmalı.')
        snapshot = cooling.cpu_snapshot()
        if not snapshot:
            raise RuntimeError('CPU geri dönüş değerleri okunamadı.')
        cpu_ceiling = min(int(v) for _, v in snapshot)//1000
        gpu_ceiling = min(self.gpu.requested_max or gpu_state['max_mhz'], gpu_state['max_mhz'])
        if cpu_ceiling < cpu_state['allowed_min_mhz'] or gpu_ceiling < 600:
            raise RuntimeError('Mevcut frekans sınırları kontrol aralığı dışında.')
        # Persist before the first write; service restart restores CPU policies.
        document = {p.parent.name: int(v) for p, v in snapshot}
        temporary = self.journal.with_suffix('.tmp')
        with temporary.open('w') as stream:
            os.chmod(temporary, 0o600)
            stream.write(json.dumps(document))
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, self.journal)
        self.saved = (snapshot, self.gpu.requested_max, self.fans.snapshot())
        self.ceilings = [cpu_ceiling, gpu_ceiling]
        self.floors = [cpu_state['allowed_min_mhz'], 600]
        self.limits = list(self.ceilings)
        self.targets = [cpu, gpu]
        self.cool_since = [None, None]
        self.last_tick = None
        self.cpu_expected = sorted((p, v.strip()) for p, v in snapshot)
        self.gpu_expected = self.gpu.requested_max
        duty = [self.fan_target(t, target) for t, target in zip(measured, self.targets)]
        self.smoothers = [CurveSmoother(d, t) for d, t in zip(duty, measured)]
        try:
            self.fans.apply('manual', dict(cpu=duty[0], gpu=duty[1]))
            self.active = True
            self.error = ''
        except Exception:
            self.stop()
            raise
        return {'verified': False, 'message': 'Sıcaklık hedefi etkin; fan ve frekans yönetimi başladı. Hedef sıcaklık garanti edilmez.'}

    @staticmethod
    def fan_target(temperature, target):
        return max(50, min(100, 100 + (temperature-target)*5))

    def stop(self, emergency=False):
        self.active = False
        if self.saved is None:
            return {'verified': True, 'message': 'Sıcaklık hedefi kapalı.'}
        snapshot, gpu, fan = self.saved
        errors = []
        for restore in (lambda: cooling.restore_cpu(snapshot), lambda: self.gpu.set_max(gpu or 0),
                        lambda: self.fans.auto() if emergency else self.fans.apply_profile(fan)):
            try:
                restore()
            except (OSError, RuntimeError, ValueError) as exc:
                errors.append(str(exc))
        if errors:
            try:
                self.fans.auto()
            except (OSError, RuntimeError, ValueError) as exc:
                errors.append(str(exc))
            self.error = 'Geri dönüş eksik: ' + '; '.join(errors)
            raise RuntimeError(self.error)
        self.journal.unlink(missing_ok=True)
        self.saved = None
        return {'verified': False, 'message': 'Sıcaklık hedefi kapatıldı; önceki frekans sınırları geri yüklendi. Fan geri bildirimi izleniyor.'}

    def tick(self, now=None):
        if not self.active:
            return
        now = time.monotonic() if now is None else now
        if self.last_tick is not None and now-self.last_tick < 2:
            return
        try:
            if self.last_tick is not None and now-self.last_tick > 10:
                raise RuntimeError('Kontrol yenilemesi kesildi; uyku veya servis gecikmesi.')
            self.last_tick = now
            state = self.fans.status()
            measured = self.temperatures(state)
            raw = state['measured']
            if state.get('mode') != 'manual' or raw.get('mode') != 1 or not raw.get('mode_byte', 0) & 0x40:
                raise RuntimeError('Fan kontrolü bırakıldı; uyku, zaman aşımı veya firmware değişimi.')
            if state.get('error') and not raw.get('thermal'):
                raise RuntimeError(state['error'])
            if sorted((p, v.strip()) for p, v in cooling.cpu_snapshot()) != self.cpu_expected or self.gpu.requested_max != self.gpu_expected:
                raise RuntimeError('Frekans ayarı başka bir işlem tarafından değiştirildi.')
            duty = [s.update(self.fan_target(t, target), t, now) for s, t, target in zip(self.smoothers, measured, self.targets)]
            if raw.get('thermal'):
                duty = [100, 100]
                self.smoothers = [CurveSmoother(100, t) for t in measured]
            if duty != state.get('targets'):
                self.fans.update_manual_targets(duty)
            for i, (temperature, target) in enumerate(zip(measured, self.targets)):
                desired = self.limits[i]
                if temperature > target:
                    self.cool_since[i] = None
                    if duty[i] == 100:
                        desired = max(self.floors[i], desired-100)
                elif temperature <= target-3:
                    if self.cool_since[i] is None:
                        self.cool_since[i] = now
                    if now-self.cool_since[i] >= 10:
                        desired = min(self.ceilings[i], desired+100)
                else:
                    self.cool_since[i] = None
                if desired != self.limits[i]:
                    if i == 0:
                        cooling.set_cpu_max(desired)
                        self.cpu_expected = sorted((p, v.strip()) for p, v in cooling.cpu_snapshot())
                    else:
                        self.gpu.set_max(desired)
                        self.gpu_expected = self.gpu.requested_max
                    self.limits[i] = desired
        except (OSError, RuntimeError, ValueError) as exc:
            error = str(exc)
            try:
                self.stop(emergency=True)
            except (OSError, RuntimeError, ValueError) as rollback:
                error += ' ' + str(rollback)
            self.error = error

    def status(self):
        return {'active': self.active, 'targets': self.targets, 'limits_mhz': self.limits,
                'error': self.error, 'restore_pending': self.saved is not None and not self.active,
                'temperature_guaranteed': False, 'gpu_limit_readback_available': False}
