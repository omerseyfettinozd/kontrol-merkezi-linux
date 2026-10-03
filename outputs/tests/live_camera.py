"""Verify fixed internal FHD/IR camera USB access and active-use rejection."""
import json,os
from pathlib import Path
from slayer_r9t.protocol import request

def run():
    before=request({'op':'status'})['features']['camera']['value']
    report={}
    try:
        request({'op':'camera','enabled':True})
        fd=os.open('/dev/video0',os.O_RDONLY|os.O_NONBLOCK)
        try:
            try:request({'op':'camera','enabled':False})
            except RuntimeError as exc:
                assert 'kullanımda' in str(exc);report['busy_rejected']=True
            else:raise AssertionError('Active camera disabled')
        finally:os.close(fd)
        off=request({'op':'camera','enabled':False})
        assert off['verified'] and not off['state']['features']['camera']['value']
        assert not list(Path('/sys/class/video4linux').glob('video*'))
        on=request({'op':'camera','enabled':True})
        assert on['verified'] and on['state']['features']['camera']['value']
        assert len(list(Path('/sys/class/video4linux').glob('video*')))==4
        report.update(disable_verified=True,enable_verified=True,video_nodes_restored=4)
    finally:request({'op':'camera','enabled':before})
    report['passed']=True
    Path(__file__).resolve().parents[1].joinpath('docs/camera-validation.json').write_text(json.dumps(report,indent=2))
    print(json.dumps(report,indent=2))
if __name__=='__main__':run()
