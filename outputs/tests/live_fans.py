"""Opt-in real fan targets/RPM acceptance. Finishes in EC automatic mode."""
import os,time,json
from pathlib import Path
if os.environ.get('R9T_LIVE_CHECK')!='1':raise SystemExit('R9T_LIVE_CHECK=1 gerekli.')
from slayer_r9t.protocol import request
before=request({'op':'status'})
checks=[]
try:
    for operation in [{'op':'manual','cpu':80,'gpu':70},{'op':'manual','cpu':60,'gpu':90},
                      {'op':'fan_preset','preset':'cool'},{'op':'fan_boost'}]:
        request(operation)
        for _ in range(6):
            time.sleep(2)
            state=request({'op':'status'})
            fans=state['fans']
            print(fans['mode'],fans['verified'],fans['targets'],fans['measured'],flush=True)
        assert fans['verified'] is True,fans
        assert fans['measured']['cpu_rpm']>500 and fans['measured']['gpu_rpm']>500
        assert state['power_profile']==before['power_profile'] and state['boost_enabled']==before['boost_enabled']
        assert state['rgb_state']==before['rgb_state']
        checks.append({'operation':operation,'fan_state':fans})
finally:
    result=request({'op':'auto'})
    assert result['verified'] is True
Path(__file__).resolve().parents[1].joinpath('docs/fan-validation.json').write_text(json.dumps({'checks':checks,'ended_in_auto':True,'stress_test':False},ensure_ascii=False,indent=2))
print('Fan acceptance passed; EC automatic mode restored.')
