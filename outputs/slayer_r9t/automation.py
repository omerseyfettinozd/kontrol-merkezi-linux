"""Serialized root-owned opt-in rules. No executable or shell is launched."""
import copy
import os
from pathlib import Path
import time
from .settings import validate_document,atomic_json,default_automation

POLICY=Path('/var/lib/slayer-r9t/policy.json')

def power_source():
    for supply in Path('/sys/class/power_supply').iterdir():
        try:
            if (supply/'type').read_text().strip()=='Mains':return 'ac' if (supply/'online').read_text().strip()=='1' else 'battery'
        except OSError:pass
    return None

def executables(uid):
    found=set()
    for entry in Path('/proc').iterdir():
        if not entry.name.isdigit():continue
        try:
            if entry.stat().st_uid==uid:found.add(os.readlink(entry/'exe'))
        except OSError:pass
    return found

class Automation:
    def __init__(self,hardware,uid,path=POLICY):
        self.hardware=hardware;self.uid=uid;self.path=Path(path)
        self.document={'schema':3,'profiles':{},'automation':default_automation()}
        self.pause_path=self.path.with_suffix('.paused')
        self.paused=self.pause_path.exists();self.session=False;self.active=None;self.error='';self.reason='';self.saved=None
        self.source=None;self.candidate=None;self.changed=0;self.last_tick=0;self.startup_done=False
        self.applied_settings=None
        try:
            import json
            if self.path.exists():
                if self.path.stat().st_size>131072:raise ValueError('Politika çok büyük.')
                self.document=validate_document(json.loads(self.path.read_text()))
        except (OSError,ValueError,TypeError) as exc:self.error=str(exc)

    def configure(self,document):
        document=validate_document(document)
        if len(__import__('json').dumps(document).encode())>131072:raise ValueError('Politika çok büyük.')
        atomic_json(self.path,document)
        if document['automation']!=self.document['automation']:
            self.startup_done=False
        self.document=copy.deepcopy(document);self.error=''
        return {'verified':True,'message':'Profiller ve otomasyon kuralları servise kaydedildi.'}

    def status(self):
        return {'paused':self.paused,'session_ready':self.session,'active':self.active,'reason':self.reason,'error':self.error,'source':self.source,'document':self.document}

    def pause(self):
        atomic_json(self.pause_path,{'paused':True})
        self.paused=True;self.active=None;self.saved=None;self.reason='Elle seçim; otomasyon duraklatıldı.'

    def resume(self):
        self.pause_path.unlink(missing_ok=True)
        self.paused=False;self.active=None;self.error='';self.startup_done=False

    def snapshot(self):
        state=self.hardware.status();caps=state['capabilities'];value={}
        if caps['power']:value['power_profile']=state['power_profile']
        epp=state.get('cooling',{}).get('epp',{})
        if caps.get('cpu_epp') and epp.get('value') in epp.get('choices',[]):value['cpu_epp']=epp['value']
        if caps['boost']:value['boost_enabled']=state['boost_enabled']
        if caps['cpu_limit']:value['cpu_max_mhz']=state['cooling']['cpu']['max_mhz']
        if caps['gpu_limit']:
            # No limit requested through this controller means reset to automatic.
            requested=state['cooling']['gpu'].get('requested_max_mhz')
            value['gpu_max_mhz']=0 if requested is None else requested
        if caps['rgb']:
            rgb=dict(state['rgb_state'])
            if 'rgb_map' in rgb:rgb.pop('rgb',None)
            value.update(rgb)
        if caps['manual_fan']:value['fan']=self.hardware.fans.snapshot()
        return value

    def decide(self,running):
        rules=self.document['automation']
        for rule in rules['apps']:
            if rule['executable'] in running:return ('app:'+rule['executable'],rule['profile'])
        if self.source and rules[self.source]:return (self.source,rules[self.source])
        if not self.startup_done and rules['startup']:return ('startup',rules['startup'])
        return None

    def tick(self,now=None,source=None,running=None):
        now=time.monotonic() if now is None else now
        if now-self.last_tick<1:return
        self.last_tick=now
        observed=power_source() if source is None else source
        if observed!=self.candidate:self.candidate=observed;self.changed=now
        if now-self.changed>=3:self.source=self.candidate
        if not self.session or self.paused:return
        try:
            if self.active and 'fan' in self.document['profiles'].get(self.active[1],{}) and self.hardware.fans.error:
                measured=self.hardware.fans.status().get('measured') or {}
                if not measured.get('thermal'):raise RuntimeError('Otomatik profil fan doğrulaması: '+self.hardware.fans.error)
            target=self.decide(executables(self.uid) if running is None else running)
            settings=self.document['profiles'][target[1]] if target else None
            if target==self.active and settings==self.applied_settings:return
            if target:
                if target[0].startswith('app:') and not (self.active and self.active[0].startswith('app:')):self.saved=self.snapshot()
                elif not target[0].startswith('app:'):self.saved=None
                result=self.hardware.dispatch({'op':'profile','settings':settings})
                self.applied_settings=copy.deepcopy(settings)
                self.active=target;self.error='';self.reason=f'{target[0]} → {target[1]}: '+result['message']
                if target[0]=='startup':self.startup_done=True
            else:
                if self.active and self.active[0].startswith('app:') and self.saved:
                    self.hardware.dispatch({'op':'profile','settings':self.saved})
                    self.reason='Uygulama kapandı; önceki ayarlar geri yüklendi.'
                self.active=None;self.saved=None;self.applied_settings=None
        except (OSError,RuntimeError,ValueError,KeyError) as exc:
            self.pause();self.error=str(exc);self.reason='Otomasyon hatası; yeniden etkinleştirme gerekli.'
