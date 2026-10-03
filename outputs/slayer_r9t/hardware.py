"""Verified hardware operations for the GameGaraj Slayer R9T."""
import fcntl
import logging
import os
from pathlib import Path
import socket
import subprocess
import time
from . import driver_io as io
from . import cooling
from .fans import Fans
from . import features
from .lighting import validate_map


class Platform:
    def __init__(self):
        self.fd = None
        self.ec_error = ''
        self.last_connect = 0
        self.match = self.identity()['vendor'] == 'GAME GARAJ' and self.identity()['product'] == 'SLAYER R9T'
        self.fans = Fans()
        self.gpu = cooling.Gpu()
        from .power_monitor import PackagePower
        self.package_power = PackagePower()
        self.idle_rgb=None
        self.connect_ec()
        from .temperature_target import TemperatureTarget
        self.temperature_target = TemperatureTarget(self.fans, self.gpu)

    @staticmethod
    def identity():
        result = {}
        for name, field in [('sys_vendor','vendor'),('product_name','product'),('bios_version','bios')]:
            try: result[field] = Path('/sys/class/dmi/id', name).read_text().strip()
            except OSError: result[field] = 'Bilinmiyor'
        return result

    def connect_ec(self):
        self.last_connect = time.monotonic()
        if not self.match: return
        try:
            self.fd = io.open_device()
            self.auto()
            self.ec_error = ''
        except (OSError, RuntimeError) as exc:
            if self.fd is not None: os.close(self.fd)
            self.fd = None
            self.ec_error = str(exc)
            logging.warning('EC bağlantısı hazır değil: %s', exc)

    def auto(self):
        if self.fans.status()['available']:
            return self.fans.auto()
        if self.fd is None: raise RuntimeError('EC arayüzüne bağlantı yok.')
        current = io.read_int(self.fd,0x14)
        if current&0x40:io.write_int(self.fd,0x12,current&~0x40)
        fcntl.ioctl(self.fd, (io.WRITE_UW << 8) | 0x14)
        return {'mode': 'auto', 'verified': False,
                'message': 'Otomatik fan komutu gönderildi; sıcaklıklar ve EC ölçümleri izleniyor.'}

    def monitor(self):
        self.package_power.tick()
        self.fans.monitor()
        self.temperature_target.tick()
        if self.fd is None and time.monotonic()-self.last_connect >= 30: self.connect_ec()

    def close(self):
        try: self.temperature_target.stop(emergency=True)
        except (OSError, RuntimeError, ValueError): logging.exception("Sıcaklık hedefi geri dönüşü başarısız")
        self.lighting_idle(False)
        if self.fans.mode!='auto':
            try: self.fans.auto()
            except (OSError,RuntimeError): logging.exception('Fan otomatiğine dönüş başarısız')
        try: self.gpu.close()
        except (OSError,RuntimeError): logging.exception('GPU otomatik frekansına dönüş başarısız')
        if self.fd is not None:
            try: self.auto()
            finally: os.close(self.fd); self.fd = None

    def status(self):
        temperatures = None
        raw = None
        if self.fd is not None:
            try:
                temperatures = [io.read_int(self.fd, 0x12), io.read_int(self.fd, 0x13)]
                if any(not 1 <= t <= 115 for t in temperatures): raise RuntimeError('EC sensör değeri geçersiz.')
                raw = [io.read_int(self.fd, 0x10), io.read_int(self.fd, 0x11)]
                self.ec_error = ''
            except (OSError, RuntimeError) as exc:
                self.ec_error = str(exc); temperatures = None
        fan_state = self.fans.status()
        cpu_limits = cooling.cpu_status()
        cpu_epp = cooling.epp_status()
        gpu_limits = self.gpu.status()
        power = get_power_profile()
        boost = get_boost()
        rgb = get_rgb()
        color_ok = rgb is not None and rgb_range_supported()
        capabilities = {'power': self.match and power is not None,
                        'boost': self.match and boost is not None,
                        'rgb': self.match and color_ok,
                        'manual_fan': self.match and fan_state['available'], 'curve': self.match and fan_state['available'],
                        'fan_boost': self.match and fan_state['available'],
                        'auto_fan': self.match and (self.fd is not None or fan_state['available']),
                        'cpu_limit': self.match and cpu_limits is not None,
                        'cpu_epp':self.match and cpu_epp['available'],
                        'gpu_limit': self.match and gpu_limits.get('available',False) and 'max_mhz' in gpu_limits,
                        'undervolt': False}
        capabilities['temperature_target'] = self.match and fan_state['available'] and cpu_limits is not None and capabilities['gpu_limit']
        extra=features.status()
        capabilities['camera']=features.camera_status()['available']
        capabilities['rgb_map']=self.match and perkey_available() and color_ok
        capabilities.update(fn_lock=extra['fn_lock']['available'],charge_limit=extra['charge_limit']['available'])
        return {'device': self.identity(), 'capabilities': capabilities,'features':extra,
                'reasons': {'manual_fan': '' if fan_state['available'] else 'R9T fan sürücüsü yüklü değil.',
                            'curve': '' if fan_state['available'] else 'R9T fan sürücüsü gerekli.',
                            'rgb': '' if color_ok else 'RGB sürücüsü veya 0–255 renk aralığı hazır değil.'},
                'ec': {'connected': self.fd is not None, 'error': self.ec_error,
                       'temperatures': temperatures, 'fan_raw': raw, 'last_command': fan_state['mode'] if fan_state['available'] else ('auto' if self.fd is not None else None),
                       'mode_verified': fan_state['verified'] if fan_state['available'] else False},
                'fans': fan_state, 'power_profile': power, 'boost_enabled': boost, 'rgb_state': rgb,
                'cooling': {'cpu_power':self.package_power.status(),'temperature_target':self.temperature_target.status(),'epp':cpu_epp,'cpu': cpu_limits, 'gpu': gpu_limits, 'presets': cooling.PRESETS,
                    'undervolt_reason': 'Bu cihazda doğrulanmış CPU/GPU voltaj yazma ve geri okuma arayüzü yok. Frekans sınırları kullanılabilir.'}}

    def dispatch(self, req):
        if not self.match: raise RuntimeError('Bu servis yalnızca GameGaraj Slayer R9T için yetkilidir.')
        op = req['op']
        if op=='temperature_target':
            return dict(self.temperature_target.start(req['cpu'], req['gpu']), state=self.status())
        if op=='temperature_target_stop':
            return dict(self.temperature_target.stop(), state=self.status())
        if op in ('auto','manual','curve','fan_boost','fan_preset','power','boost','thermal_preset','profile'):
            if self.temperature_target.saved is not None: self.temperature_target.stop()
        if op=='lighting_idle':return self.lighting_idle(req['idle'])
        if op=='camera':return dict(features.camera(req['enabled']),state=self.status())
        if op=='fn_lock':return dict(features.fn_lock(req['enabled']),state=self.status())
        if op=='charge_limit':return dict(features.charge_limit(req['end'],req['start']),state=self.status())
        if op == 'auto': return self.auto()
        if op in ('manual','curve','fan_boost','fan_preset'):
            if not self.fans.status()['available']: raise RuntimeError('R9T fan sürücüsü hazır değil; işlem yapılmadı.')
            return dict(self.fans.apply(op,req),state=self.status())
        if op == 'power':
            set_power_profile(req.get('profile'))
            return {'message': 'Güç profili sistemden doğrulandı.', 'verified': True, 'state': self.status()}
        if op == 'boost':
            set_boost(req.get('enabled'))
            return {'message': 'CPU boost ayarı sistemden doğrulandı.', 'verified': True, 'state': self.status()}
        if op == 'rgb':
            set_rgb(req.get('rgb'), req.get('brightness'))
            return {'message': 'Klavye ayarları sürücüden doğrulandı.', 'verified': True, 'state': self.status()}
        if op == 'thermal_preset':
            preset = req.get('preset')
            if not isinstance(preset,str) or preset not in cooling.PRESETS: raise ValueError('Bilinmeyen soğutma profili.')
            req = {'settings': cooling.PRESETS[preset]['settings']}
            op = 'profile'
        if op == 'profile':
            settings = req.get('settings')
            apply_profile(settings,self.gpu,self.fans)
            return {'message': 'Profil uygulandı; geri okunabilen ayarlar doğrulandı.'+(' Fan hedefi kabul edildi; RPM doğrulaması sürüyor.' if 'fan' in settings else '')+(' GPU sınır komutu sürücü tarafından kabul edildi; sınırın bağımsız geri okuması yok.' if 'gpu_max_mhz' in settings else ''),
                    'verified': 'gpu_max_mhz' not in settings and 'fan' not in settings, 'state': self.status()}
        raise ValueError('Bilinmeyen donanım işlemi.')

    @staticmethod
    def write_lighting(state,brightness):
        if 'rgb_map' in state:set_rgb_map(state['rgb_map'],brightness)
        else:set_rgb(state['rgb'],brightness)

    def lighting_idle(self,idle):
        if type(idle) is not bool:raise ValueError('Işık durumu boolean olmalı.')
        if idle and self.idle_rgb is None:
            original=get_rgb()
            if original and original['brightness'] is not None and original['brightness']>0:
                self.idle_rgb=original
                self.write_lighting(original,0)
        elif not idle and self.idle_rgb:
            original=self.idle_rgb;current=get_rgb()
            if current and current['brightness']==0 and current.get('rgb_map',current['rgb'])==original.get('rgb_map',original['rgb']):self.write_lighting(original,original['brightness'])
            self.idle_rgb=None
        return {'verified':True,'message':'Klavye boşta kalma aydınlatması güncellendi.'}


def rgb_range_supported():
    entries = leds()
    if not entries: return False
    for entry in entries:
        try:
            limit = entry/'multi_max_intensity'
            if limit.exists() and min(map(int,limit.read_text().split())) < 255: return False
        except (OSError, ValueError): return False
    return True


def validate_profile(settings):
    if not isinstance(settings, dict) or not settings or set(settings)-{'power_profile','boost_enabled','rgb','brightness','cpu_max_mhz','gpu_max_mhz','fan','rgb_map','cpu_epp'}:
        raise ValueError('Profil alanları geçersiz.')
    if 'power_profile' in settings and settings['power_profile'] not in ('power-saver','balanced','performance'):
        raise ValueError('Profil güç ayarı geçersiz.')
    if 'boost_enabled' in settings and type(settings['boost_enabled']) is not bool:
        raise ValueError('Profil boost ayarı geçersiz.')
    for field, upper in [('cpu_max_mhz',6000),('gpu_max_mhz',4000)]:
        if field in settings and (type(settings[field]) is not int or not 0<=settings[field]<=upper):
            raise ValueError('Frekans sınırı geçersiz.')
    if 'rgb' in settings and 'rgb_map' in settings:raise ValueError('Tek renk ve renk haritası aynı profilde olamaz.')
    if ('rgb' in settings or 'rgb_map' in settings) != ('brightness' in settings):raise ValueError('Renk ve parlaklık birlikte kaydedilmeli.')
    if 'brightness' in settings and (type(settings['brightness']) is not int or not 0<=settings['brightness']<=100):raise ValueError('Profil parlaklığı geçersiz.')
    if 'rgb_map' in settings:validate_map(settings['rgb_map'])
    if 'rgb' in settings:
        colors = settings['rgb']; brightness = settings['brightness']
        if not isinstance(colors,list) or len(colors)!=3 or any(type(c) is not int or not 0<=c<=255 for c in colors):raise ValueError('Profil rengi geçersiz.')
        if type(brightness) is not int or not 0<=brightness<=100:raise ValueError('Profil parlaklığı geçersiz.')
    if 'cpu_epp' in settings and (not isinstance(settings['cpu_epp'],str) or settings['cpu_epp'] not in cooling.EPP_CHOICES):raise ValueError('Geçersiz CPU enerji tercihi.')
    if 'fan' in settings:
        from .fans import validate_fan
        validate_fan(settings['fan'])
    return dict(settings)


def apply_profile(settings,gpu=None,fans=None):
    settings = validate_profile(settings)
    if 'gpu_max_mhz' in settings and gpu is None: raise RuntimeError('GPU denetleyicisi hazır değil.')
    if 'fan' in settings and (fans is None or not fans.status()['available']):raise RuntimeError('Fan köprüsü hazır değil.')
    if 'rgb_map' in settings and not perkey_available():raise RuntimeError('126 kanallı R9T klavye arayüzü hazır değil.')
    if 'cpu_epp' in settings and settings['cpu_epp'] not in cooling.epp_status()['choices']:raise RuntimeError('CPU enerji tercihi arayüzü hazır değil.')
    original_epp=cooling.epp_snapshot() if 'cpu_epp' in settings else []
    original_fan=fans.snapshot() if 'fan' in settings else None
    original_cpu = cooling.cpu_snapshot() if 'cpu_max_mhz' in settings or 'boost_enabled' in settings else []
    original_gpu = gpu.requested_max if gpu else None
    original_power = get_power_profile() if 'power_profile' in settings else None
    policies = list(Path('/sys/devices/system/cpu/cpufreq').glob('policy*/boost')) if 'boost_enabled' in settings else []
    original_boost = [(file,file.read_text()) for file in policies]
    original_rgb = [(entry,(entry/'multi_intensity').read_text(),(entry/'brightness').read_text()) for entry in (leds() if 'rgb' in settings or 'rgb_map' in settings else [])]
    try:
        if 'power_profile' in settings: set_power_profile(settings['power_profile'])
        if 'boost_enabled' in settings: set_boost(settings['boost_enabled'])
        if 'cpu_max_mhz' in settings: cooling.set_cpu_max(settings['cpu_max_mhz'])
        if 'cpu_epp' in settings:cooling.set_epp(settings['cpu_epp'])
        if 'rgb' in settings: set_rgb(settings['rgb'],settings['brightness'])
        if 'rgb_map' in settings:set_rgb_map(settings['rgb_map'],settings['brightness'])
        if 'gpu_max_mhz' in settings: gpu.set_max(settings['gpu_max_mhz'])
        if 'fan' in settings:fans.apply_profile(settings['fan'])
    except Exception as exc:
        failures = []
        if original_power:
            try: set_power_profile(original_power)
            except Exception as error: failures.append(str(error))
        for file,value in original_boost:
            try:
                file.write_text(value)
                if file.read_text().strip()!=value.strip(): raise OSError('Boost geri yüklemesi doğrulanamadı.')
            except OSError as error: failures.append(str(error))
        if original_cpu:
            try: cooling.restore_cpu(original_cpu)
            except Exception as error: failures.append(str(error))
        if original_epp:
            try:cooling.restore_epp(original_epp)
            except Exception as error:failures.append(str(error))
        for entry,intensity,brightness in original_rgb:
            try:
                (entry/'multi_intensity').write_text(intensity); (entry/'brightness').write_text(brightness)
            except OSError as error: failures.append(str(error))
        for entry,intensity,brightness in original_rgb:
            try:
                if (entry/'multi_intensity').read_text().strip()!=intensity.strip() or (entry/'brightness').read_text().strip()!=brightness.strip():
                    raise OSError('RGB geri yüklemesi doğrulanamadı.')
            except OSError as error: failures.append(str(error))
        if gpu and gpu.requested_max!=original_gpu:
            try: gpu.set_max(original_gpu or 0)
            except Exception as error: failures.append(str(error))
        if original_fan:
            try:
                fans.apply_profile(original_fan)
                if original_fan['mode']!='auto':
                    deadline=time.monotonic()+10
                    while time.monotonic()<deadline:
                        fans.monitor()
                        if fans.verified:break
                        time.sleep(.5)
                    if not fans.verified:raise RuntimeError('Önceki fan hedefi doğrulanamadı.')
            except Exception as error:
                failures.append(str(error))
                try:fans.auto()
                except Exception as fallback:failures.append(str(fallback))
        detail = ' Önceki ayarlar geri yüklendi.' if not failures else ' Geri yükleme eksik: '+'; '.join(failures)
        raise RuntimeError(str(exc)+detail) from exc

def get_power_profile():
    try:
        result=subprocess.run(['/usr/bin/powerprofilesctl','get'],capture_output=True,text=True,timeout=2,check=True)
        return result.stdout.strip()
    except (OSError,subprocess.SubprocessError):return None


def set_power_profile(profile):
    if profile not in ('power-saver','balanced','performance'):raise ValueError('Güç profili geçersiz.')
    result=subprocess.run(['/usr/bin/powerprofilesctl','set',profile],capture_output=True,text=True,timeout=5)
    if result.returncode:raise RuntimeError(result.stderr.strip() or 'Güç profili değiştirilemedi.')
    if get_power_profile()!=profile:raise RuntimeError('İstenen güç profili sistemden doğrulanamadı.')


def get_boost():
    try:
        values={(p/'boost').read_text().strip() for p in Path('/sys/devices/system/cpu/cpufreq').glob('policy*')}
        return True if values=={'1'} else False if values=={'0'} else None
    except OSError:return None


def get_rgb():
    entries=leds()
    if not entries:return None
    colors=set(); brightness=set(); color_map=[]
    try:
        for entry in entries:
            channels=(entry/'multi_index').read_text().split()
            values=list(map(int,(entry/'multi_intensity').read_text().split()))
            color=dict(zip(channels,values));colors.add(tuple(color[c] for c in ('red','green','blue')));color_map.append([color[c] for c in ('red','green','blue')])
            brightness.add(round(int((entry/'brightness').read_text())*100/int((entry/'max_brightness').read_text())))
        return dict(rgb=list(next(iter(colors))) if len(colors)==1 else None,
                    brightness=next(iter(brightness)) if len(brightness)==1 else None,
                    **({'rgb_map':color_map} if len(colors)>1 and perkey_available() else {}))
    except (OSError,ValueError,KeyError,ZeroDivisionError):return None


def set_boost(enabled):
    if type(enabled) is not bool: raise ValueError('Boost açık/kapalı olmalı.')
    policies=list(Path('/sys/devices/system/cpu/cpufreq').glob('policy*'))
    if not policies: raise RuntimeError('CPU güç arayüzü bulunamadı.')
    previous=[]
    try:
        for policy in policies:
            if (policy/'scaling_driver').read_text().strip()!='amd-pstate-epp':
                raise RuntimeError('Beklenen AMD P-State sürücüsü bulunamadı.')
            file=policy/'boost'
            previous.append((file,file.read_text()))
            file.write_text('1' if enabled else '0')
            if file.read_text().strip()!=str(int(enabled)):raise RuntimeError('CPU boost değeri doğrulanamadı.')
    except Exception:
        for file,value in previous:
            try:file.write_text(value)
            except OSError:logging.exception('Boost geri yükleme hatası')
        raise


def leds():
    result = []
    for entry in Path('/sys/class/leds').glob('rgb:kbd_backlight*'):
        driver = (entry / 'device/driver').resolve().name
        if driver == 'ite_8291' and (entry / 'multi_intensity').is_file(): result.append(entry)
    return sorted(result,key=lambda p:0 if p.name=='rgb:kbd_backlight' else int(p.name.rsplit('_',1)[-1]))


def perkey_available():
    entries=leds()
    names=['rgb:kbd_backlight']+[f'rgb:kbd_backlight_{i}' for i in range(1,126)]
    return len(entries)==126 and [p.name for p in entries]==names and all('048D:600B' in str((p/'device').resolve()).upper() for p in entries)


def set_rgb_map(colors,brightness):
    validate_map(colors)
    if type(brightness) is not int or not 0<=brightness<=100:raise ValueError('Parlaklık %0–100 olmalı.')
    if not perkey_available() or not rgb_range_supported():raise RuntimeError('R9T ITE8291 renk haritası arayüzü hazır değil.')
    entries=leds()
    before=[(p,(p/'multi_intensity').read_text(),(p/'brightness').read_text()) for p in entries]
    try:
        for entry,color in zip(entries,colors):
            channels=(entry/'multi_index').read_text().split()
            values=dict(zip(('red','green','blue'),color))
            intensity=' '.join(str(values[c]) for c in channels)
            level=str(round(int((entry/'max_brightness').read_text())*brightness/100))
            if (entry/'multi_intensity').read_text().strip()!=intensity:(entry/'multi_intensity').write_text(intensity)
            if (entry/'brightness').read_text().strip()!=level:(entry/'brightness').write_text(level)
            notify('WATCHDOG=1')
        for entry,color in zip(entries,colors):
            channels=(entry/'multi_index').read_text().split()
            actual=dict(zip(channels,map(int,(entry/'multi_intensity').read_text().split())))
            if [actual[c] for c in ('red','green','blue')]!=color or int((entry/'brightness').read_text())!=round(int((entry/'max_brightness').read_text())*brightness/100):raise RuntimeError('Renk haritası geri okuması eşleşmedi.')
    except Exception:
        for entry,intensity,level in before:
            try:(entry/'multi_intensity').write_text(intensity);(entry/'brightness').write_text(level)
            except OSError:logging.exception('Renk haritası geri yüklemesi başarısız')
        raise


def set_rgb(rgb, brightness):
    if not isinstance(rgb, list) or len(rgb) != 3 or any(type(v) is not int or not 0 <= v <= 255 for v in rgb):
        raise ValueError('RGB değerleri 0–255 olmalı.')
    if type(brightness) is not int or not 0 <= brightness <= 100:
        raise ValueError('Parlaklık %0–100 olmalı.')
    entries = leds()
    if not entries: raise RuntimeError('ITE 8291 RGB arayüzü bulunamadı.')
    if not rgb_range_supported(): raise RuntimeError('RGB sürücüsünün 0–255 renk aralığı hazır değil.')
    maxima = {int((entry/'max_brightness').read_text()) for entry in entries}
    if len(maxima)!=1 or min(maxima)<=0: raise RuntimeError('Klavye parlaklık aralığı tutarsız.')
    previous = [(entry, (entry/'multi_intensity').read_text(), (entry/'brightness').read_text()) for entry in entries]
    saved = {entry:(intensity,value) for entry,intensity,value in previous}
    try:
        for entry in entries:
            channels = (entry / 'multi_index').read_text().split()
            if set(channels) != {'red', 'green', 'blue'}: raise RuntimeError('RGB kanal düzeni bilinmiyor.')
            colors = dict(zip(['red', 'green', 'blue'], rgb))
            maximum = int((entry / 'max_brightness').read_text())
            intensity=' '.join(str(colors[c]) for c in channels)
            value=str(round(maximum * brightness / 100))
            if saved[entry][0].strip()!=intensity:(entry / 'multi_intensity').write_text(intensity)
            if saved[entry][1].strip()!=value:(entry / 'brightness').write_text(value)
            notify('WATCHDOG=1')
            actual=list(map(int,(entry/'multi_intensity').read_text().split()))
            if actual!=[colors[c] for c in channels]:raise RuntimeError('RGB kanalları istenen değeri korumadı.')
        actual_state=get_rgb()
        if actual_state is None or actual_state['rgb']!=rgb or actual_state['brightness']!=round(round(maximum * brightness / 100)*100/maximum):
            raise RuntimeError('Klavye ayarları sürücüden doğrulanamadı.')
    except Exception:
        for entry, intensity, value in previous:
            try:
                (entry/'multi_intensity').write_text(intensity); (entry/'brightness').write_text(value)
            except OSError: logging.exception('RGB geri yükleme hatası')
        raise


def notify(message):
    address = os.environ.get('NOTIFY_SOCKET')
    if not address: return
    if address.startswith('@'): address = '\0' + address[1:]
    with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as connection:
        connection.connect(address); connection.sendall(message.encode())
