"""Live static keyboard layouts, idle preservation and restoration."""
import json,time
from pathlib import Path
from slayer_r9t.protocol import request
from slayer_r9t.lighting import pattern

def run():
    state=request({'op':'status'});original=dict(state['rgb_state'])
    if original.get('rgb_map'):original.pop('rgb',None)
    assert state['capabilities']['rgb_map']
    report={'version':state['version'],'patterns':[]}
    try:
        for name in ('zones','rainbow','gradient'):
            colors=pattern(name,[255,129,3])
            result=request({'op':'profile','settings':{'rgb_map':colors,'brightness':80}})
            assert result['verified']
            actual=request({'op':'status'})['rgb_state']
            assert actual['rgb_map']==colors and actual['brightness']==80
            request({'op':'lighting_idle','idle':True})
            assert request({'op':'status'})['rgb_state']['brightness']==0
            request({'op':'lighting_idle','idle':False})
            actual=request({'op':'status'})['rgb_state']
            assert actual['rgb_map']==colors and actual['brightness']==80
            report['patterns'].append({'name':name,'channels':len(colors),'readback':True,'idle_restore':True})
    finally:
        request({'op':'lighting_idle','idle':False})
        request({'op':'profile','settings':original})
    report['restored']=request({'op':'status'})['rgb_state']==state['rgb_state']
    assert report['restored']
    report['passed']=True
    Path(__file__).resolve().parents[1].joinpath('docs/lighting-validation.json').write_text(json.dumps(report,indent=2))
    print(json.dumps(report,indent=2))
if __name__=='__main__':run()
