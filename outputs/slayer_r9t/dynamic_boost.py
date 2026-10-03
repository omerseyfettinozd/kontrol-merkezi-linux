"""Read-only firmware support and daemon health, distinct from behavior proof."""
from datetime import datetime, timezone
from pathlib import Path
import re
import subprocess
from .gpu_diagnostics import NVIDIA,read

CPU_ROOT=Path('/sys/devices/system/cpu/cpufreq')
PROPERTIES=('LoadState','ActiveState','SubState','Result','ExecMainStatus','NRestarts','MainPID','UnitFileState')


def daemon_status():
    result={'healthy':None,'properties':{},'error':''}
    try:
        reply=subprocess.run(['/usr/bin/systemctl','show','nvidia-powerd.service','--no-pager',
                              '--property='+','.join(PROPERTIES)],capture_output=True,text=True,timeout=2)
        if reply.returncode:
            raise RuntimeError((reply.stderr.strip() or 'Servis bilgisi okunamadı.')[:300])
        fields={k:v for k,sep,v in (line.partition('=') for line in reply.stdout.splitlines()) if sep and k in PROPERTIES}
        if set(fields)!=set(PROPERTIES):
            raise ValueError('Eksik nvidia-powerd servis yanıtı.')
        for field in ('ExecMainStatus','NRestarts','MainPID'):
            if not fields[field].isdigit():raise ValueError('Geçersiz servis sayacı.')
            fields[field]=int(fields[field])
        result['properties']=fields
        result['healthy']=(fields['LoadState']=='loaded' and fields['ActiveState']=='active' and
                           fields['SubState']=='running' and fields['Result']=='success' and
                           fields['ExecMainStatus']==0 and fields['MainPID']>0)
    except (OSError,ValueError,RuntimeError,subprocess.SubprocessError) as exc:
        result['error']=str(exc)[:400]
    return result


def snapshot():
    devices=[]
    for gpu in sorted(NVIDIA.glob('*')):
        if not re.fullmatch(r'[0-9a-fA-F]{4}:[0-9a-fA-F]{2}:[0-9a-fA-F]{2}\.[0-7]',gpu.name):continue
        text=read(gpu/'power')
        value=None
        for line in (text or '').splitlines():
            key,sep,v=line.partition(':')
            if sep and key.strip()=='Notebook Dynamic Boost':value=v.strip()
        supported={'Supported':True,'Not Supported':False,'Unsupported':False}.get(value)
        devices.append({'pci':gpu.name,'supported':supported,'raw_support':value,
                        'reason':'' if supported is not None else 'Firmware destek alanı okunamadı veya tanınmıyor.'})
    policies=[]
    for entry in sorted(CPU_ROOT.glob('policy*')):
        driver=read(entry/'scaling_driver')
        value=read(entry/'scaling_max_freq')
        policies.append({'name':entry.name,'driver':driver,'max_mhz':int(value)/1000 if value and value.isdigit() else None})
    return {'time':datetime.now(timezone.utc).isoformat(timespec='seconds'),'devices':devices,
            'daemon':daemon_status(),'cpu_policies':policies,
            'behavior_verified':False,'behavior_reason':'Firmware desteği ve çalışan servis, yük altında güç aktarımının kanıtı değildir.'}


if __name__=='__main__':
    import json
    print(json.dumps(snapshot(),ensure_ascii=False))
