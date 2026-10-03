"""Validated user profiles with atomic writes and legacy fan-profile preservation."""
import json
import copy
import os
from pathlib import Path
import tempfile
import time
import shutil
from .hardware import validate_profile

DEFAULT_PATH = Path.home()/'.config/slayer-r9t-control-center/settings.json'

def default_automation():
    return {'startup':None,'ac':None,'battery':None,'apps':[],'low_hz_on_battery':False,'lighting_timeout':0}

def validate_automation(value,profiles):
    if not isinstance(value,dict) or set(value)!=set(default_automation()):raise ValueError('Otomasyon alanları geçersiz.')
    for key in ('startup','ac','battery'):
        if value[key] is not None and (not isinstance(value[key],str) or value[key] not in profiles):raise ValueError('Otomasyon profili bulunamadı.')
    if type(value['low_hz_on_battery']) is not bool:raise ValueError('Pil Hz seçimi geçersiz.')
    if type(value['lighting_timeout']) is not int or not 0<=value['lighting_timeout']<=3600:raise ValueError('Işık zaman aşımı 0–3600 saniye olmalı.')
    if not isinstance(value['apps'],list) or len(value['apps'])>32:raise ValueError('En fazla 32 uygulama kuralı.')
    seen=set()
    for rule in value['apps']:
        if not isinstance(rule,dict) or set(rule)!={'executable','profile'}:raise ValueError('Uygulama kuralı geçersiz.')
        executable=rule['executable']
        if not isinstance(executable,str) or not executable.startswith('/') or len(executable)>4096 or '\x00' in executable or executable in seen:raise ValueError('Tekil tam uygulama yolu gerekli.')
        if not isinstance(rule['profile'],str) or rule['profile'] not in profiles:raise ValueError('Uygulama profili bulunamadı.')
        seen.add(executable)
    return value


def edit_app_rule(automation, profiles, index, executable, profile):
    """Replace one draft rule without changing its priority or saving it."""
    candidate=copy.deepcopy(validate_automation(automation, profiles))
    if type(index) is not int or not 0<=index<len(candidate['apps']):
        raise ValueError('Seçili uygulama kuralı bulunamadı.')
    candidate['apps'][index]={'executable':executable,'profile':profile}
    return validate_automation(candidate, profiles)


def move_app_rule(automation, profiles, index, destination):
    """Move one draft rule to an explicit first-match priority position."""
    candidate=copy.deepcopy(validate_automation(automation, profiles))
    if any(type(position) is not int or not 0<=position<len(candidate['apps']) for position in (index,destination)):
        raise ValueError('Uygulama kuralı sırası geçersiz.')
    candidate['apps'].insert(destination,candidate['apps'].pop(index))
    return candidate


def upsert_app_rule(automation, profiles, executable, profile):
    """Adding the same executable updates its existing slot, never its priority."""
    candidate=copy.deepcopy(validate_automation(automation, profiles))
    for index,rule in enumerate(candidate['apps']):
        if rule['executable']==executable:
            return edit_app_rule(candidate,profiles,index,executable,profile)
    candidate['apps'].append({'executable':executable,'profile':profile})
    return validate_automation(candidate,profiles)


def validate_document(value):
    if not isinstance(value, dict) or value.get('schema') not in (2,3):
        raise ValueError('Profil dosyası sürümü geçersiz.')
    profiles = value.get('profiles')
    if not isinstance(profiles, dict) or len(profiles) > 100:
        raise ValueError('En fazla 100 profil desteklenir.')
    for name, settings in profiles.items():
        if not isinstance(name, str) or not name.strip() or len(name) > 80:
            raise ValueError('Profil adı 1–80 karakter olmalı.')
        validate_profile(settings)
    automation=value.get('automation',default_automation())
    validate_automation(automation,profiles)
    if len(json.dumps(value,ensure_ascii=False).encode())>65536:raise ValueError('Profil ve kural belgesi en fazla 64 KiB olmalı.')
    return {'schema': 3, 'profiles': profiles, 'legacy_fan_profiles': value.get('legacy_fan_profiles', {}),'automation':automation}


def atomic_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    handle, temporary = tempfile.mkstemp(prefix='.'+path.name, dir=path.parent)
    try:
        with os.fdopen(handle, 'w') as file:
            json.dump(value, file, ensure_ascii=False, indent=2)
            file.write('\n')
            file.flush()
            os.fsync(file.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


class Settings:
    def __init__(self, path=DEFAULT_PATH):
        self.path = Path(path)
        self.warning = ''
        self.data = {'schema': 3, 'profiles': {}, 'legacy_fan_profiles': {},'automation':default_automation()}
        if not self.path.exists():
            return
        try:
            if self.path.stat().st_size > 131072:
                raise ValueError('Ayar dosyası çok büyük.')
            value = json.loads(self.path.read_text())
            if isinstance(value, dict) and 'schema' not in value:
                self.data['legacy_fan_profiles'] = value
                self.warning = 'Eski fan profilleri arşivlendi; yeni profiller güç, frekans, klavye ve fan ayarlarını birlikte kaydedebilir.'
            else:
                self.data = validate_document(value)
                if value.get('schema')==2:
                    backup=self.path.with_name(self.path.name+'.schema2-backup')
                    if not backup.exists():shutil.copyfile(self.path,backup);backup.chmod(0o600)
                    atomic_json(self.path,self.data)
                    self.warning='Profiller şema 3’e taşındı; şema 2 yedeği korundu. Otomasyon kapalı.'
        except (OSError, ValueError, TypeError) as exc:
            self.warning = 'Ayarlar okunamadı: '+str(exc)
            try:
                backup = self.path.with_name(self.path.name+f'.invalid-{time.time_ns()}')
                self.path.rename(backup)
                self.warning += ' Önceki dosya .invalid ekiyle korundu.'
            except OSError:
                pass

    @property
    def profiles(self):
        return self.data['profiles']

    def save(self):
        atomic_json(self.path, validate_document(self.data))

    def import_file(self, path):
        path = Path(path)
        if path.stat().st_size > 131072:
            raise ValueError('Profil dosyası çok büyük.')
        incoming = validate_document(json.loads(path.read_text()))
        merged = dict(self.profiles, **incoming['profiles'])
        candidate = dict(self.data, profiles=merged)
        validate_document(candidate)
        atomic_json(self.path, candidate)
        self.data = candidate

    def export_file(self, path):
        atomic_json(path, self.data)
