"""Opt-in cooling command acceptance; no stress or undervolt testing."""
import json
import os
from pathlib import Path
import time
if os.environ.get('R9T_LIVE_CHECK')!='1':
    raise SystemExit('R9T_LIVE_CHECK=1 gerekli.')
from slayer_r9t.protocol import request
before = request({'op':'status'})
restore = {'power_profile':before['power_profile'],'boost_enabled':before['boost_enabled'],
           'cpu_max_mhz':before['cooling']['cpu']['max_mhz'],
           'gpu_max_mhz':before['cooling']['gpu']['requested_max_mhz'] or 0}
checks = []
try:
    for preset in ('quiet','balanced_cool','gaming_cool','stock'):
        result = request({'op':'thermal_preset','preset':preset})
        expected = before['cooling']['presets'][preset]['settings']
        state = result['state']
        assert state['power_profile']==expected['power_profile']
        assert state['boost_enabled']==expected['boost_enabled']
        if expected['cpu_max_mhz']:
            assert state['cooling']['cpu']['max_mhz']==expected['cpu_max_mhz']
        assert state['cooling']['gpu']['requested_max_mhz']==(expected['gpu_max_mhz'] or None)
        assert state['rgb_state']==before['rgb_state']
        # Allow idle/resume clock transition to settle; not a load-temperature benchmark.
        time.sleep(1)
        measured = request({'op':'status'})
        checks.append({'preset':preset,'cpu_max_mhz':measured['cooling']['cpu']['max_mhz'],
                       'gpu_requested_mhz':measured['cooling']['gpu']['requested_max_mhz'],
                       'gpu_measured_mhz':measured['cooling']['gpu'].get('current_mhz'),
                       'gpu_limit_readback':False})
finally:
    request({'op':'profile','settings':restore})
    after = request({'op':'status'})
    assert after['power_profile']==restore['power_profile']
    assert after['boost_enabled']==restore['boost_enabled']
    assert after['cooling']['cpu']['max_mhz']==restore['cpu_max_mhz']
    assert after['cooling']['gpu']['requested_max_mhz']==before['cooling']['gpu']['requested_max_mhz']
    assert after['rgb_state']==before['rgb_state']
artifacts = Path(__file__).resolve().parents[1]/'docs/cooling-validation.json'
artifacts.write_text(json.dumps({'checks':checks,'restored':True,'thermal_benchmark':False},indent=2))
print(artifacts.read_text())
