"""R9T fan controller: fixed PWM targets, curves, RPM feedback and kernel lease."""
from pathlib import Path
import time

ROOT = Path('/sys/module/r9t_fan/parameters')
PRESETS = {
    'balanced': {'name':'Dengeli fan eğrisi','points':[[45,50],[60,60],[75,80],[85,100]]},
    'cool': {'name':'Serin fan eğrisi','points':[[45,60],[60,75],[75,90],[85,100]]},
    'maximum': {'name':'Maksimum soğutma','boost':True},
}


def validate_curve(points):
    if not isinstance(points,list) or not 2<=len(points)<=10:
        raise ValueError('Eğri 2–10 nokta içermeli.')
    previous = (0,0)
    for point in points:
        if not isinstance(point,list) or len(point)!=2:
            raise ValueError('Her nokta sıcaklık ve fan yüzdesi içermeli.')
        temperature, duty = point
        if type(temperature) is not int or type(duty) is not int or not 40<=temperature<=95 or not 50<=duty<=100:
            raise ValueError('Eğride sıcaklık 40–95°C, fan %50–100 olmalı.')
        if temperature<=previous[0] or duty<previous[1]:
            raise ValueError('Sıcaklıklar artmalı, fan yüzdesi azalmamalı.')
        previous = temperature,duty
    if previous[1]!=100 or previous[0]>90:
        raise ValueError('Eğri en geç 90°C’de %100 fanla bitmeli.')
    return [list(p) for p in points]


def curve_target(points, temperature):
    if temperature<=points[0][0]:return points[0][1]
    for (t0,p0),(t1,p1) in zip(points,points[1:]):
        if temperature<=t1:return round(p0+(p1-p0)*(temperature-t0)/(t1-t0))
    return 100


class CurveSmoother:
    """Raise immediately; require cooling before gradually reducing duty."""
    def __init__(self, duty, temperature):
        self.duty=duty
        self.anchor=temperature
        self.cooling_since=None

    def update(self, desired, temperature, now):
        if desired>=self.duty:
            self.duty=desired;self.anchor=temperature;self.cooling_since=None
        elif temperature>self.anchor-2:
            self.cooling_since=None
        else:
            if self.cooling_since is None:self.cooling_since=now
            if now-self.cooling_since>=6:
                self.duty=max(desired,self.duty-2)
                if self.duty==desired:self.anchor=temperature;self.cooling_since=None
        return self.duty


def validate_fan(value):
    if not isinstance(value,dict):raise ValueError('Fan profili bir nesne olmalı.')
    mode=value.get('mode')
    fields={'auto':{'mode'},'boost':{'mode'},'manual':{'mode','cpu','gpu'},'curve':{'mode','points'},'preset':{'mode','preset'}}
    if not isinstance(mode,str) or mode not in fields or set(value)!=fields[mode]:raise ValueError('Fan profili alanları geçersiz.')
    if mode=='manual' and any(type(value[k]) is not int or not 50<=value[k]<=100 for k in ('cpu','gpu')):
        raise ValueError('Fan hedefleri %50–100 olmalı.')
    if mode=='curve':validate_curve(value['points'])
    if mode=='preset' and (not isinstance(value['preset'],str) or value['preset'] not in PRESETS):raise ValueError('Bilinmeyen fan profili.')
    return dict(value)


class Fans:
    def __init__(self):
        self.mode = 'auto'
        self.targets = None
        self.points = None
        self.started = 0
        self.last_refresh = 0
        self.verified = False
        self.error = ''
        self.samples = 0
        self.failure_since = None
        self.last_state = None
        self.smoothers=[]

    def snapshot(self):
        if self.mode=='manual':return {'mode':'manual','cpu':self.targets[0],'gpu':self.targets[1]}
        if self.mode=='curve':return {'mode':'curve','points':[list(p) for p in self.points]}
        return {'mode':self.mode}

    def apply_profile(self,value):
        value=validate_fan(value)
        mode=value['mode']
        return self.apply({'auto':'auto','boost':'fan_boost','manual':'manual','curve':'curve','preset':'fan_preset'}[mode],value)

    def read(self):
        values = {key:int(value) for key,value in (entry.split('=') for entry in (ROOT/'status').read_text().split())}
        required = {'mode','cpu_target','gpu_target','cpu_rpm','gpu_rpm','cpu_temp','gpu_temp','mode_byte','cpu_pwm','gpu_pwm','lease_ms','error','thermal'}
        if set(values)!=required:raise RuntimeError('Fan sürücüsü yanıtı geçersiz.')
        self.last_state = values
        return values

    def send(self, command):
        (ROOT/'control').write_text(command)
        self.last_refresh = time.monotonic()

    def auto(self):
        self.send('auto')
        self.mode = 'auto';self.targets=None;self.points=None;self.samples=0
        self.smoothers=[]
        state=self.read()
        if state['mode']!=0 or state['mode_byte']&0x40:
            raise RuntimeError('Fan otomatik modu geri okunarak doğrulanamadı.')
        self.verified=True;self.error=''
        return {'verified':True,'message':'Fanlar EC otomatiğine döndü; mod biti doğrulandı.'}

    def apply(self, op, req):
        if op=='auto':return self.auto()
        if op=='fan_preset':
            name=req.get('preset')
            if not isinstance(name,str) or name not in PRESETS:raise ValueError('Bilinmeyen fan profili.')
            preset=PRESETS[name]
            op='fan_boost' if preset.get('boost') else 'curve'
            req={'points':preset.get('points')}
        points=None
        if op=='manual':
            targets=[req.get('cpu'),req.get('gpu')]
            if any(type(v) is not int or not 50<=v<=100 for v in targets):
                raise ValueError('CPU/GPU fanı %50–100 arasında olmalı.')
            mode='manual'
        elif op=='curve':
            points=validate_curve(req.get('points'))
            measured=self.read()
            targets=[curve_target(points,measured['cpu_temp']),curve_target(points,measured['gpu_temp'])]
            mode='curve'
        elif op=='fan_boost':targets=[100,100];mode='boost'
        else:raise ValueError('Bilinmeyen fan işlemi.')
        self.send('boost' if mode=='boost' else f'manual {targets[0]} {targets[1]}')
        self.mode=mode;self.targets=targets;self.points=points
        self.smoothers=[CurveSmoother(duty,measured[key]) for duty,key in zip(targets,('cpu_temp','gpu_temp'))] if mode=='curve' else []
        self.started=time.monotonic();self.samples=0;self.failure_since=None;self.verified=False;self.error=''
        return {'verified':False,'message':'Fan hedefi gönderildi. Mod, PWM ve gerçek RPM üzerinden doğrulama sürüyor.'}

    def update_manual_targets(self, targets):
        # Preserve the feedback failure timer across temperature adjustments.
        if self.mode != 'manual' or any(type(v) is not int or not 50<=v<=100 for v in targets) or len(targets)!=2:
            raise RuntimeError('Etkin manuel fan kontrolü gerekli.')
        self.send(f'manual {targets[0]} {targets[1]}')
        self.targets=list(targets);self.verified=False;self.samples=0

    def monitor(self):
        if self.mode=='auto' or time.monotonic()-self.last_refresh<2:return
        try:
            measured=self.read()
            if measured['mode']==0 or not measured['mode_byte']&0x40:
                raise RuntimeError('Fan sürücüsü otomatiğe döndü; zaman aşımı, uyku veya firmware mod değişimi olabilir.')
            expected=list(self.targets)
            if self.mode=='curve':
                now=time.monotonic()
                self.targets=[smoother.update(curve_target(self.points,measured[key]),measured[key],now) for smoother,key in zip(self.smoothers,('cpu_temp','gpu_temp'))]
                if measured['thermal']:
                    self.targets=[100,100]
                    self.smoothers=[CurveSmoother(100,measured[key]) for key in ('cpu_temp','gpu_temp')]
            self.send('boost' if self.mode=='boost' else f'manual {self.targets[0]} {self.targets[1]}')
            if measured['thermal']:
                self.error='Sıcaklık koruması etkin: iki fan %100.';self.verified=False
                return
            matches=all(abs(measured[field]-target*2)<=6 for field,target in zip(('cpu_pwm','gpu_pwm'),expected))
            moving=measured['cpu_rpm']>500 and measured['gpu_rpm']>500
            self.samples=self.samples+1 if matches and moving else 0
            self.verified=self.samples>=2 and time.monotonic()-self.started>=6
            if self.targets!=expected:self.verified=False;self.samples=0
            if matches and moving:self.failure_since=None
            elif self.failure_since is None:self.failure_since=time.monotonic()
            if self.failure_since is not None and time.monotonic()-self.failure_since>=15:
                raise RuntimeError('Fan hedefleri veya RPM doğrulanamadı; EC otomatiğine dönüldü.')
            self.error=''
        except (OSError,ValueError,RuntimeError) as exc:
            error=str(exc)
            try:self.auto()
            except Exception as fallback:error+=' Otomatiğe dönüş hatası: '+str(fallback)
            self.mode='auto';self.verified=False;self.error=error

    def status(self):
        available=(ROOT/'status').is_file() and (ROOT/'control').exists()
        try:measured=self.read() if available else None
        except (OSError,ValueError,RuntimeError) as exc:measured=None;self.error=str(exc)
        return {'available':available and measured is not None,'mode':self.mode,
                'targets':self.targets,'points':self.points,'verified':self.verified,
                'error':self.error,'measured':measured,'presets':PRESETS,
                'curve_smoothing':{'cooling_hysteresis_c':2,'cooling_hold_seconds':6,'down_step_percent':2,'refresh_seconds':2}}
