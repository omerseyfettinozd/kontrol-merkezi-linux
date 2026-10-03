"""Fixed-path feature adapters. Missing or unverified firmware writes stay closed."""
from pathlib import Path
from . import telemetry

FN_LOCK=Path('/sys/devices/platform/tuxedo_keyboard/fn_lock')
BATTERY=Path('/sys/class/power_supply/BAT0')
CAMERA=Path('/sys/bus/usb/devices/5-1.4')

def camera_status():
    try:
        match=(CAMERA/'idVendor').read_text().strip()=='2b7e' and (CAMERA/'idProduct').read_text().strip()=='c906' and (CAMERA/'product').read_text().strip()=='FHD WebCam'
        enabled=read_int(CAMERA/'authorized')
        return {'available':match and enabled in (0,1),'value':bool(enabled) if enabled in (0,1) else None,
                'reason':'FHD ve IR kamera için Linux USB aygıt erişimi; BIOS kamera kilidi değildir.' if match else 'Beklenen dahili FHD/IR kamera kimliği eşleşmedi.'}
    except OSError:return {'available':False,'value':None,'reason':'Dahili kamera USB arayüzü bulunamadı.'}

def camera(enabled):
    import time
    if type(enabled) is not bool:raise ValueError('Kamera açık/kapalı olmalı.')
    if not camera_status()['available']:raise RuntimeError(camera_status()['reason'])
    before=read_int(CAMERA/'authorized')
    if before==int(enabled):return {'verified':True,'message':'Kamera zaten seçilen durumda.'}
    if not enabled:
        nodes={str(p.resolve()) for p in Path('/sys/class/video4linux').glob('video*') if str(CAMERA.resolve())+'/' in str((p/'device').resolve())}
        devices={'/dev/'+Path(p).name for p in nodes}
        for process in Path('/proc').glob('[0-9]*'):
            for fd in (process/'fd').glob('*'):
                try:
                    if str(fd.resolve()) in devices:raise RuntimeError('Kamera kullanımda; kamera kullanan uygulamayı kapatın.')
                except OSError:pass
    try:
        (CAMERA/'authorized').write_text('1' if enabled else '0')
        deadline=time.monotonic()+5
        while time.monotonic()<deadline:
            found=any(str(CAMERA.resolve())+'/' in str((p/'device').resolve()) for p in Path('/sys/class/video4linux').glob('video*'))
            if read_int(CAMERA/'authorized')==int(enabled) and found==enabled:
                return {'verified':True,'message':'FHD/IR kamera '+('etkinleştirildi; video aygıtları geri geldi.' if enabled else 'kapatıldı; video aygıtları kaldırıldı.')}
            time.sleep(.1)
        raise RuntimeError('Kamera aygıt durumu doğrulanamadı.')
    except Exception:
        (CAMERA/'authorized').write_text(str(before))
        raise

def read_int(path):
    try:return int(path.read_text())
    except (OSError,ValueError):return None

def status():
    end=BATTERY/'charge_control_end_threshold';start=BATTERY/'charge_control_start_threshold'
    return {'battery':dict(telemetry.battery() or {},charge_end=read_int(end),charge_start=read_int(start)),
        'fn_lock':{'available':read_int(FN_LOCK) in (0,1),'value':read_int(FN_LOCK),'reason':'TUXEDO Fn Lock sysfs'},
        'charge_limit':{'available':end.exists(),'start_available':start.exists(),'minimum':50,'maximum':100,
            'reason':'' if end.exists() else 'BAT0 şarj eşiği arayüzü yok; OEM şarj eşiği 0x07b9=0; araştırılan platform alanı 0x07c3=254 ve ROMID[0]=255. Model firmware kanıtı bulunmadan ürün kimliği değiştirilerek şarj kontrolü açılmıyor.'},
        'oem_mode':{'available':False,'reason':'Office/Gaming/Turbo EC komutlarının bu BIOS üzerindeki anlamı ve geri okuması doğrulanmadı.'},
        'cpu_watts':{'available':False,'reason':'RyzenAdj bu Dragon Range cihazında güç tablosunu açamıyor; doğrulanmış SPL/SPPT/FPPT arayüzü yok.'},
        'gpu_watts':{'available':False,'ctgp_offset_raw':read_int(Path('/sys/devices/platform/tuxedo_nvidia_power_ctrl/ctgp_offset')),
            'reason':'NVML güç sınırı yazmayı desteklemiyor. ctgp_offset 0→5→10 denendi ve geri yüklendi; 24 ölçümde NVIDIA güç sınırı 105 W kaldı. EC geri okuması var, gerçek güç etkisi doğrulanmadı.'},
        'mux':{'available':False,'reason':'Windows paketinde GPU geçiş komutları var; R9T için doğrulanmış Linux MUX yazma arayüzü yok.'},
        'camera':camera_status(),
        'usb_charge':{'available':False,'reason':'Uyku/kapalı durumda USB şarjının R9T firmware arayüzü doğrulanmadı.'},
        'rgb_effects':{'available':False,'reason':'126 kanallı statik renk düzenleri kullanılabilir. Donanım animasyonlarının bağımsız geri okuma ve bu klavyedeki davranışı henüz doğrulanmadı.'}}

def fn_lock(enabled):
    if type(enabled) is not bool:raise ValueError('Fn Lock boolean olmalı.')
    if not status()['fn_lock']['available']:raise RuntimeError('Fn Lock arayüzü hazır değil.')
    FN_LOCK.write_text('1' if enabled else '0')
    if read_int(FN_LOCK)!=int(enabled):raise RuntimeError('Fn Lock geri okuması eşleşmedi.')
    return {'verified':True,'message':'Fn Lock sistemden doğrulandı.'}

def charge_limit(end,start=None):
    if type(end) is not int or not 50<=end<=100:raise ValueError('Şarj sınırı %50–100 olmalı.')
    if start is not None and (type(start) is not int or not 0<=start<end):raise ValueError('Şarja başlama eşiği üst sınırdan küçük olmalı.')
    feature=status()['charge_limit']
    if not feature['available'] or (start is not None and not feature['start_available']):raise RuntimeError(feature['reason'] or 'Başlama eşiği desteklenmiyor.')
    paths=[(BATTERY/'charge_control_end_threshold',end)]
    if start is not None:paths.append((BATTERY/'charge_control_start_threshold',start))
    before=[(p,p.read_text()) for p,_ in paths]
    try:
        for path,value in paths:path.write_text(str(value))
        if any(read_int(path)!=value for path,value in paths):raise RuntimeError('Şarj eşiği doğrulanamadı.')
    except Exception:
        for path,value in reversed(before):path.write_text(value)
        raise
    return {'verified':True,'message':'Şarj eşikleri geri okunarak doğrulandı.'}
