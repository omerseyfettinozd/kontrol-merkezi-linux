"""Frequency-based cooling, without voltage or undocumented SMU writes."""
import ctypes
import os
from pathlib import Path

GPU_MARKER = Path('/var/lib/slayer-r9t/gpu-clock-owned')
CPU_ROOT = Path('/sys/devices/system/cpu/cpufreq')
EPP_CHOICES=('performance','balance_performance','balance_power','power')

def epp_status():
    try:
        policies=sorted(CPU_ROOT.glob('policy*'))
        if not policies or any((p/'scaling_driver').read_text().strip()!='amd-pstate-epp' for p in policies):raise ValueError('AMD P-State EPP arayüzü yok.')
        supported=set(EPP_CHOICES)
        for p in policies:supported.intersection_update((p/'energy_performance_available_preferences').read_text().split())
        values={(p/'energy_performance_preference').read_text().strip() for p in policies}
        return {'available':bool(supported),'choices':[v for v in EPP_CHOICES if v in supported],
                'value':next(iter(values)) if len(values)==1 else None,'policy_count':len(policies),
                'reason':'' if supported else 'Ortak enerji tercihi bulunamadı.'}
    except (OSError,ValueError) as exc:return {'available':False,'choices':[],'value':None,'reason':str(exc)}

def epp_snapshot():
    return [(p/'energy_performance_preference',(p/'energy_performance_preference').read_text()) for p in CPU_ROOT.glob('policy*')]

def restore_epp(snapshot):
    errors=[]
    for path,value in snapshot:
        try:
            path.write_text(value)
            if path.read_text().strip()!=value.strip():raise RuntimeError('CPU enerji tercihi geri yüklenemedi.')
        except (OSError,RuntimeError) as exc:errors.append(str(exc))
    if errors:raise RuntimeError('; '.join(errors))

def set_epp(value):
    state=epp_status()
    if not state['available']:raise RuntimeError(state['reason'])
    if not isinstance(value,str) or value not in state['choices']:raise ValueError('Desteklenmeyen CPU enerji tercihi.')
    snapshot=epp_snapshot()
    try:
        for path,_ in snapshot:path.write_text(value)
        if epp_status()['value']!=value:raise RuntimeError('CPU enerji tercihi tüm politikalarda doğrulanamadı.')
    except Exception as exc:
        try:restore_epp(snapshot)
        except Exception as rollback:raise RuntimeError(str(exc)+' Geri yükleme eksik: '+str(rollback)) from exc
        raise


def cpu_status():
    try:
        policies = sorted(CPU_ROOT.glob('policy*'))
        if not policies or any((p/'scaling_driver').read_text().strip()!='amd-pstate-epp' for p in policies):
            return None
        maximum = min(int((p/'amd_pstate_max_freq').read_text()) for p in policies)//1000
        minimum = max(int((p/'scaling_min_freq').read_text()) for p in policies)//1000+1
        values = {int((p/'scaling_max_freq').read_text()) for p in policies}
        return {'max_mhz': next(iter(values))//1000 if len(values)==1 else None,
                'allowed_min_mhz': max(1000, minimum), 'allowed_max_mhz': maximum}
    except (OSError, ValueError):
        return None


def cpu_snapshot():
    return [(p/'scaling_max_freq',(p/'scaling_max_freq').read_text()) for p in CPU_ROOT.glob('policy*')]


def restore_cpu(snapshot):
    errors = []
    for path, value in snapshot:
        try:
            path.write_text(value)
            if path.read_text().strip()!=value.strip():
                raise RuntimeError('CPU frekans sınırı geri yüklenemedi.')
        except (OSError, RuntimeError) as exc:
            errors.append(str(exc))
    if errors:
        raise RuntimeError('; '.join(errors))


def set_cpu_max(mhz):
    state = cpu_status()
    if not state:
        raise RuntimeError('AMD P-State frekans arayüzü hazır değil.')
    if type(mhz) is not int or (mhz!=0 and not state['allowed_min_mhz']<=mhz<=state['allowed_max_mhz']):
        raise ValueError('CPU sınırı desteklenen aralıkta bir MHz değeri veya 0 (otomatik) olmalı.')
    snapshot = cpu_snapshot()
    try:
        for path, _ in snapshot:
            desired = int((path.parent/'cpuinfo_max_freq').read_text()) if mhz==0 else mhz*1000
            path.write_text(str(desired))
            # The driver clamps the request to the non-boost ceiling if boost is off.
            expected = min(desired,int((path.parent/'cpuinfo_max_freq').read_text()))
            if int(path.read_text())!=expected:
                raise RuntimeError('CPU frekans sınırı sistemden doğrulanamadı.')
    except Exception as exc:
        try:
            restore_cpu(snapshot)
        except Exception as rollback:
            raise RuntimeError(str(exc)+' Geri yükleme eksik: '+str(rollback)) from exc
        raise


class Gpu:
    def __init__(self):
        self.lib = None
        self.handle = ctypes.c_void_p()
        self.requested_max = None
        self.error = ''
        try:
            lib = ctypes.CDLL('libnvidia-ml.so.1')
            lib.nvmlErrorString.restype = ctypes.c_char_p
            self.lib = lib
            self.check(lib.nvmlInit_v2())
            self.check(lib.nvmlDeviceGetHandleByIndex_v2(0,ctypes.byref(self.handle)))
            name = ctypes.create_string_buffer(128)
            self.check(lib.nvmlDeviceGetName(self.handle,name,128))
            if name.value.decode()!='NVIDIA GeForce RTX 5070 Laptop GPU':
                raise RuntimeError('Beklenen RTX 5070 Laptop GPU bulunamadı.')
            if GPU_MARKER.exists():
                self.check(lib.nvmlDeviceResetGpuLockedClocks(self.handle))
                GPU_MARKER.unlink()
        except (OSError, AttributeError, RuntimeError) as exc:
            self.error = str(exc)
            if self.lib:
                self.lib.nvmlShutdown()
            self.lib = None

    def check(self, code):
        if code:
            raise RuntimeError('NVIDIA: '+self.lib.nvmlErrorString(code).decode())

    def read(self, function, *args):
        value = ctypes.c_uint()
        self.check(getattr(self.lib,function)(self.handle,*args,ctypes.byref(value)))
        return value.value

    def status(self):
        result = {'available': self.lib is not None, 'requested_max_mhz': self.requested_max,
                  'limit_readback_available': False, 'error': self.error}
        if not self.lib:
            return result
        try:
            result['max_mhz'] = self.read('nvmlDeviceGetMaxClockInfo',ctypes.c_uint(0))
            result['current_mhz'] = self.read('nvmlDeviceGetClockInfo',ctypes.c_uint(0))
            result['enforced_power_w'] = self.read('nvmlDeviceGetEnforcedPowerLimit')/1000
            # The current enforced limit is not proof that power-limit writes are supported.
            value = ctypes.c_uint()
            result['power_limit_supported'] = self.lib.nvmlDeviceGetPowerManagementLimit(self.handle,ctypes.byref(value))==0
        except (AttributeError, RuntimeError) as exc:
            result['error'] = str(exc)
        return result

    def set_max(self, mhz):
        state = self.status()
        if not state['available'] or 'max_mhz' not in state:
            raise RuntimeError('NVIDIA frekans arayüzü hazır değil.')
        if type(mhz) is not int or (mhz!=0 and not 600<=mhz<=state['max_mhz']):
            raise ValueError('GPU sınırı 600 MHz ile donanım üst sınırı arasında veya 0 (otomatik) olmalı.')
        previous = self.requested_max
        if mhz and GPU_MARKER.parent.is_dir():
            temporary = GPU_MARKER.with_suffix('.tmp')
            temporary.write_text(str(mhz))
            os.chmod(temporary,0o600)
            os.replace(temporary,GPU_MARKER)
        try:
            self._set_max(mhz)
        except Exception:
            if previous is not None: GPU_MARKER.write_text(str(previous))
            else: GPU_MARKER.unlink(missing_ok=True)
            raise
        if mhz==0: GPU_MARKER.unlink(missing_ok=True)

    def _set_max(self,mhz):
        if mhz==0:
            self.check(self.lib.nvmlDeviceResetGpuLockedClocks(self.handle))
            self.requested_max = None
        else:
            self.check(self.lib.nvmlDeviceSetGpuLockedClocks(self.handle,ctypes.c_uint(0),ctypes.c_uint(mhz)))
            self.requested_max = mhz

    def close(self):
        if self.lib:
            try:
                if self.requested_max is not None:
                    self.set_max(0)
            finally:
                self.lib.nvmlShutdown()
                self.lib = None


PRESETS = {
    'quiet': {'name': 'Sessiz / Ofis', 'description': 'Daha düşük frekanslar; ofis ve hafif işler için.',
              'settings': {'power_profile':'power-saver','boost_enabled':False,'cpu_max_mhz':1800,'gpu_max_mhz':1200}},
    'balanced_cool': {'name': 'Serin Dengeli', 'description': 'CPU boost kapalı; GPU üst frekansı sınırlı.',
              'settings': {'power_profile':'balanced','boost_enabled':False,'cpu_max_mhz':2400,'gpu_max_mhz':1800}},
    'gaming_cool': {'name': 'Serin Oyun', 'description': 'CPU boost açık, üst frekans sınırlı; FPS etkisi oyuna göre değişir.',
              'settings': {'power_profile':'balanced','boost_enabled':True,'cpu_max_mhz':3500,'gpu_max_mhz':2100}},
    'stock': {'name': 'Sınırları kaldır', 'description': 'Frekans sınırları otomatik, CPU boost açık ve güç profili dengeli.',
              'settings': {'power_profile':'balanced','boost_enabled':True,'cpu_max_mhz':0,'gpu_max_mhz':0}},
}
