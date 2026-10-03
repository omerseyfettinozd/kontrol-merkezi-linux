"""Explicit bounded target control check; no synthetic thermal load."""
import json
import os
import time
from pathlib import Path
from slayer_r9t.protocol import request
from slayer_r9t.cooling import cpu_snapshot


def run():
    if os.environ.get('R9T_LIVE_CHECK') != '1':
        raise RuntimeError('Canlı ayar testi için R9T_LIVE_CHECK=1 gerekli.')
    before=request({'op':'status'})
    if before['version'] not in ('0.8.0','0.9.0','0.10.0'): raise RuntimeError('Önce 0.8.0 veya üstü kurulmalı.')
    if before['cooling']['temperature_target']['active']: raise RuntimeError('Mevcut hedef önce elle durdurulmalı.')
    original={p.parent.name:v.strip() for p,v in cpu_snapshot()}
    report={'version':before['version'],'samples':[],'gpu_independent_readback':False}
    started=False
    try:
        started=True
        result=request({'op':'temperature_target','cpu':85,'gpu':80})
        assert result['state']['cooling']['temperature_target']['active']
        for _ in range(8):
            time.sleep(2)
            state=request({'op':'status'})
            target=state['cooling']['temperature_target']
            assert target['active'] and not target['error'],target
            fan=state['fans']
            assert fan['mode']=='manual' and fan['measured']['cpu_rpm']>500 and fan['measured']['gpu_rpm']>500
            report['samples'].append({'target':target,'fan':fan})
    finally:
        if started: request({'op':'temperature_target_stop'})
    after=request({'op':'status'})
    assert not after['cooling']['temperature_target']['active']
    assert original=={p.parent.name:v.strip() for p,v in cpu_snapshot()}
    assert before['cooling']['gpu']['requested_max_mhz']==after['cooling']['gpu']['requested_max_mhz']
    assert before['fans']['mode']==after['fans']['mode']
    assert before['fans']['targets']==after['fans']['targets'] or before['fans']['mode']=='curve'
    report.update(passed=True,restored=True)
    Path('docs/temperature-target-validation.json').write_text(json.dumps(report,ensure_ascii=False,indent=2))
    print('Temperature target live check passed; initial limits and fan mode restored.')

if __name__=='__main__': run()
