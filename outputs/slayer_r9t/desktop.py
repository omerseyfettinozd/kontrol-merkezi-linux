"""KDE/Wayland controls executed in the user's session, without a shell."""
import json
from pathlib import Path
import re
import subprocess

def run(*args):
    result=subprocess.run(args,capture_output=True,text=True,timeout=5)
    if result.returncode:raise RuntimeError(result.stderr.strip() or result.stdout.strip() or 'Masaüstü işlemi başarısız.')
    return result.stdout.strip()

def prop(service,path,interface,name,system=False):
    return json.loads(run('/usr/bin/busctl','--system' if system else '--user','--json=short','get-property',service,path,interface,name))['data']

def screens():return [output for output in json.loads(run('/usr/bin/kscreen-doctor','-j'))['outputs'] if output['connected'] and output['enabled']]

def touchpads():
    ids=prop('org.kde.KWin','/org/kde/KWin/InputDevice','org.kde.KWin.InputDeviceManager','devicesSysNames')
    result=[]
    for identifier in ids:
        if not re.fullmatch(r'event\d+',identifier):continue
        path='/org/kde/KWin/InputDevice/'+identifier
        if prop('org.kde.KWin',path,'org.kde.KWin.InputDevice','touchpad'):
            result.append({'id':identifier,'name':prop('org.kde.KWin',path,'org.kde.KWin.InputDevice','name'),
                'enabled':prop('org.kde.KWin',path,'org.kde.KWin.InputDevice','enabled')})
    return result

def status():
    result={'screens':[],'touchpads':[],'errors':{},'nightlight':None,'wifi':None,'bluetooth':None}
    readers={'screens':screens,'touchpads':touchpads,
        'nightlight':lambda:prop('org.kde.KWin','/org/kde/KWin/NightLight','org.kde.KWin.NightLight','enabled'),
        'wifi':lambda:run('/usr/bin/nmcli','radio','wifi')=='enabled',
        'bluetooth':lambda:prop('org.bluez','/org/bluez/hci0','org.bluez.Adapter1','Powered',True)}
    for key,reader in readers.items():
        try:result[key]=reader()
        except (OSError,ValueError,KeyError,RuntimeError,subprocess.SubprocessError) as exc:result['errors'][key]=str(exc)
    result['super_lock']={'available':False,'reason':'Tam Super tuşu kilidi için bu Wayland oturumunda doğrulanmış giriş yeniden eşleme yolu yok.'}
    result['remapping']={'available':True,'reason':'KDE klavye ayarlarında desteklenen XKB seçenekleri yönetilebilir.'}
    return result

def set_mode(output,mode):
    current=next((s for s in screens() if s['id']==output),None)
    if current is None or mode not in [m['id'] for m in current['modes']]:raise ValueError('Bağlı ekranın mevcut modlarından birini seçin.')
    run('/usr/bin/kscreen-doctor',f'output.{output}.mode.{mode}')
    actual=next((s for s in screens() if s['id']==output),None)
    if not actual or actual['currentModeId']!=mode:raise RuntimeError('Ekran modu geri okunarak doğrulanamadı.')

def dispatch(req):
    op=req.get('op')
    fields={'status':set(),'mode':{'output','mode'},'brightness':{'output','value'},'nightlight':{'enabled'},
        'touchpad':{'id','enabled'},'wifi':{'enabled'},'bluetooth':{'enabled'},'airplane':{'enabled'},
        'keyboard_settings':set(),'color_settings':set(),'icc':{'output','path'},'remap':{'preset'}}
    if op not in fields or set(req)!={'op'}|fields[op]:raise ValueError('Masaüstü istek alanları geçersiz.')
    if op=='status':return status()
    if 'enabled' in req and type(req['enabled']) is not bool:raise ValueError('Boolean gerekli.')
    if op in ('mode','brightness'):
        if type(req['output']) is not int:raise ValueError('Ekran kimliği geçersiz.')
        current=next((s for s in screens() if s['id']==req['output']),None)
        if current is None:raise ValueError('Ekran bağlı değil.')
        if op=='mode':
            if not isinstance(req['mode'],str):raise ValueError('Mod kimliği geçersiz.')
            set_mode(req['output'],req['mode'])
        else:
            if type(req['value']) is not int or not 10<=req['value']<=100:raise ValueError('Ekran parlaklığı %10–100 olmalı.')
            run('/usr/bin/kscreen-doctor',f"output.{req['output']}.brightness.{req['value']}")
            actual=next(s for s in screens() if s['id']==req['output'])
            if abs(actual.get('brightness',-1)*100-req['value'])>2:raise RuntimeError('Parlaklık geri okuması doğrulanamadı.')
    elif op=='nightlight':
        run('/usr/bin/kwriteconfig6','--file','kwinrc','--group','NightColor','--key','Active','--type','bool','--notify','true' if req['enabled'] else 'false')
        import time
        for _ in range(25):
            if prop('org.kde.KWin','/org/kde/KWin/NightLight','org.kde.KWin.NightLight','enabled')==req['enabled']:break
            time.sleep(.2)
        else:raise RuntimeError('Gece rengi etkinliği doğrulanamadı.')
    elif op=='touchpad':
        if req['id'] not in [p['id'] for p in touchpads()]:raise ValueError('Touchpad kimliği geçersiz.')
        path='/org/kde/KWin/InputDevice/'+req['id']
        run('/usr/bin/busctl','--user','set-property','org.kde.KWin',path,'org.kde.KWin.InputDevice','enabled','b',str(req['enabled']).lower())
        if prop('org.kde.KWin',path,'org.kde.KWin.InputDevice','enabled')!=req['enabled']:raise RuntimeError('Touchpad doğrulanamadı.')
    elif op=='icc':
        if type(req['output']) is not int or req['output'] not in [s['id'] for s in screens()]:raise ValueError('Bağlı ekran seçin.')
        if not isinstance(req['path'],str):raise ValueError('ICC dosyası gerekli.')
        path=Path(req['path']).resolve()
        if path.suffix.lower() not in ('.icc','.icm') or not path.is_file() or not 128<=path.stat().st_size<=16777216:raise ValueError('Geçerli ICC/ICM dosyası seçin (128 bayt–16 MiB).')
        with path.open('rb') as profile:header=profile.read(128)
        if header[36:40]!=b'acsp':raise ValueError('ICC dosya imzası geçersiz.')
        run('/usr/bin/kscreen-doctor',f"output.{req['output']}.iccprofile.{path}",f"output.{req['output']}.colorProfileSource.ICC")
        actual=next(s for s in screens() if s['id']==req['output'])
        if actual.get('iccProfilePath')!=str(path):raise RuntimeError('ICC dosya yolu geri okunamadı.')
    elif op=='remap':
        options={'default':None,'caps_escape':'caps:escape','caps_ctrl':'caps:ctrl_modifier','swap_ctrl_caps':'ctrl:swapcaps'}
        if req['preset'] not in options:raise ValueError('Bilinmeyen tuş düzeni.')
        original=run('/usr/bin/kreadconfig6','--file','kxkbrc','--group','Layout','--key','Options')
        selected=[o for o in original.split(',') if o and not o.startswith('caps:') and o!='ctrl:swapcaps']
        if options[req['preset']]:selected.append(options[req['preset']])
        value=','.join(selected)
        run('/usr/bin/kwriteconfig6','--file','kxkbrc','--group','Layout','--key','Options',value)
        run('/usr/bin/kwriteconfig6','--file','kxkbrc','--group','Layout','--key','ResetOldOptions','true')
        run('/usr/bin/qdbus6','org.kde.KWin','/KWin','org.kde.KWin.reconfigure')
        if run('/usr/bin/kreadconfig6','--file','kxkbrc','--group','Layout','--key','Options')!=value:raise RuntimeError('Klavye seçimi kaydedilemedi.')
        return {'verified':False,'outcome':'accepted','message':'KDE tuş düzeni kaydedildi ve yeniden yüklendi; gerçek tuş davranışı ayrıca kontrol edilmeli.','state':status()}
    elif op in ('wifi','bluetooth'):
        if op=='wifi':run('/usr/bin/nmcli','radio','wifi','on' if req['enabled'] else 'off')
        else:run('/usr/bin/busctl','--system','set-property','org.bluez','/org/bluez/hci0','org.bluez.Adapter1','Powered','b',str(req['enabled']).lower())
        if status()[op]!=req['enabled']:raise RuntimeError('Radyo ayarı doğrulanamadı.')
    elif op=='airplane':
        previous=status();changed=[]
        try:
            for radio in ('wifi','bluetooth'):
                if previous[radio] is None:continue
                dispatch({'op':radio,'enabled':not req['enabled']});changed.append(radio)
            if not changed:raise RuntimeError('Yönetilebilir radyo yok.')
        except Exception:
            for radio in reversed(changed):dispatch({'op':radio,'enabled':previous[radio]})
            raise
    else:
        subprocess.Popen(['/usr/bin/systemsettings','kcm_keyboard' if op=='keyboard_settings' else 'kcm_nightcolor'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
        return {'verified':False,'message':'KDE ayarları açıldı; değişiklikler KDE üzerinden uygulanır.','outcome':'accepted'}
    return {'verified':True,'message':'Masaüstü ayarı sistemden doğrulandı.','outcome':'verified','state':status()}
