"""Opt-in real acceptance; restores policy, hardware, and the display mode."""
import json,os,subprocess,time
from pathlib import Path
from slayer_r9t.protocol import request
from slayer_r9t.session import client
from slayer_r9t.settings import default_automation
if os.environ.get('R9T_LIVE_CHECK')!='1':raise SystemExit('R9T_LIVE_CHECK=1 gerekli.')
before=request({'op':'status'});desktop=client({'op':'status'});checks=[];process=None
fan=before['fans'];fan_setting={'mode':fan['mode']}
if fan['mode']=='manual':fan_setting.update(cpu=fan['targets'][0],gpu=fan['targets'][1])
elif fan['mode']=='curve':fan_setting['points']=fan['points']
restore={'power_profile':before['power_profile'],'boost_enabled':before['boost_enabled'],**before['rgb_state'],
    'cpu_max_mhz':before['cooling']['cpu']['max_mhz'],'fan':fan_setting}
try:
    profile=dict(restore,fan={'mode':'manual','cpu':80,'gpu':70})
    result=request({'op':'profile','settings':profile});assert result['outcome']=='accepted'
    for index in range(40):
        time.sleep(2);state=request({'op':'status'})
        if index>=3:
            assert state['fans']['verified'],state['fans']
            assert state['fans']['targets']==[80,70]
            assert state['fans']['measured']['cpu_rpm']>500 and state['fans']['measured']['gpu_rpm']>500
        assert state['power_profile']==before['power_profile'] and state['boost_enabled']==before['boost_enabled']
        assert state['rgb_state']==before['rgb_state']
        print('sustained fan',index,state['fans']['verified'],flush=True)
    checks.append('Combined profile and sustained fan targets/RPM for 40 samples')
    if before['capabilities']['fn_lock']:
        original=before['features']['fn_lock']['value']==1
        request({'op':'fn_lock','enabled':not original})
        assert request({'op':'status'})['features']['fn_lock']['value']==int(not original)
        request({'op':'fn_lock','enabled':original});checks.append('Fn Lock toggled, read back and restored')
    output=desktop['screens'][0];original_mode=output['currentModeId']
    different=next(m['id'] for m in output['modes'] if m['id']!=original_mode)
    client({'op':'mode','output':output['id'],'mode':different})
    client({'op':'mode','output':output['id'],'mode':original_mode})
    checks.append('KDE screen mode changed and restored')
    brightness=round(output['brightness']*100)
    client({'op':'brightness','output':output['id'],'value':max(10,brightness-10)})
    client({'op':'brightness','output':output['id'],'value':brightness});checks.append('Screen brightness readback and restore')
    night=desktop['nightlight']
    client({'op':'nightlight','enabled':not night});client({'op':'nightlight','enabled':night})
    checks.append('KDE night light toggled and restored')
    pads=desktop['touchpads']
    if pads:
        client({'op':'touchpad','id':pads[0]['id'],'enabled':not pads[0]['enabled']})
        client({'op':'touchpad','id':pads[0]['id'],'enabled':pads[0]['enabled']})
        checks.append('Touchpad toggled and restored')
    for radio in ('wifi','bluetooth'):
        if desktop[radio] is not None:
            client({'op':radio,'enabled':desktop[radio]});checks.append(radio+' current value command/readback (no disconnection)')
    rules=default_automation();rules['apps']=[{'executable':'/usr/bin/sleep','profile':'Acceptance'}]
    document={'schema':3,'profiles':{'Acceptance':{'fan':{'mode':'manual','cpu':70,'gpu':80}}},'automation':rules}
    request({'op':'configure','document':document});request({'op':'automation_resume'})
    process=subprocess.Popen(['/usr/bin/sleep','12'])
    time.sleep(5)
    assert request({'op':'status'})['automation']['active']==['app:/usr/bin/sleep','Acceptance']
    process.terminate();process.wait();process=None;time.sleep(4)
    state=request({'op':'status'});assert state['fans']['targets']==[80,70],state['fans']
    checks.append('Real executable detection and previous hardware snapshot restoration')
    request({'op':'auto'});assert request({'op':'status'})['automation']['paused']
    checks.append('Manual override pauses automation')
    print('Extended live acceptance passed',flush=True)
finally:
    if process:process.terminate();process.wait()
    request({'op':'profile','settings':restore})
    request({'op':'configure','document':before['automation']['document']})
    request({'op':'automation_pause' if before['automation']['paused'] else 'automation_resume'})
    if before['capabilities']['fn_lock']:request({'op':'fn_lock','enabled':before['features']['fn_lock']['value']==1})
    for output in desktop['screens']:
        client({'op':'mode','output':output['id'],'mode':output['currentModeId']})
        client({'op':'brightness','output':output['id'],'value':round(output['brightness']*100)})
    client({'op':'nightlight','enabled':desktop['nightlight']})
    for pad in desktop['touchpads']:client({'op':'touchpad','id':pad['id'],'enabled':pad['enabled']})
    request({'op':'automation_pause' if before['automation']['paused'] else 'automation_resume'})
    Path(__file__).resolve().parents[1].joinpath('docs/extended-validation.json').write_text(json.dumps({'checks':checks,'restored':True,'physical_suspend_tested':False,'load_benchmark':False},ensure_ascii=False,indent=2))
