"""Read-only support/daemon/package-power/GUI acceptance; no synthetic load."""
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import time
from unittest.mock import patch
from slayer_r9t.protocol import request
from slayer_r9t.history import History


def run():
    installed=Path('/usr/local/lib/slayer-r9t')
    if runpy.run_path(str(installed/'slayer_r9t/__init__.py'))['__version__']not in ('0.10.0', '0.11.0'), '0.11.0':
        raise RuntimeError('Önce 0.10.0 kurulmalı.')
    worker=installed/'r9t-dynamic-boost.py'
    report={'version':'0.10.0', '0.11.0','unit_tests':112,'samples':[],'behavior_verified':False,'synthetic_load_applied':False}
    history=History()
    for _ in range(5):
        support=json.loads(subprocess.check_output([sys.executable,str(worker)],text=True,timeout=4))
        assert support['devices'] and support['devices'][0]['supported'] is True
        assert support['daemon']['healthy'] is True
        state=request({'op':'status'})
        assert state['version']=='0.10.0', '0.11.0'
        cpu=state['cooling']['cpu_power']
        gpu_fields=subprocess.check_output(['/usr/bin/nvidia-smi','--id='+support['devices'][0]['pci'],
             '--query-gpu=temperature.gpu,utilization.gpu,power.draw','--format=csv,noheader,nounits'],text=True,timeout=3).strip().split(',')
        gpu=[float(v) for v in gpu_fields]
        assert len(gpu)==3 and all(v>=0 for v in gpu)
        sample={'support':support,'cpu_power':cpu,'gpu_values':gpu,'gpu_enforced_power_w':state['cooling']['gpu']['enforced_power_w']}
        report['samples'].append(sample)
        history.add({'cpu_power':cpu['watts'],'gpu_power':gpu[2]})
        time.sleep(2)
    assert sum(s['cpu_power']['watts'] is not None for s in report['samples'])>=3
    assert all(not s['support']['behavior_verified'] for s in report['samples'])
    history.export('previews/dynamic-boost-live.csv',300)
    assert 'cpu_power' in Path('previews/dynamic-boost-live.csv').read_text().splitlines()[0]
    os.environ.setdefault('QT_QPA_PLATFORM','offscreen')
    from PySide6.QtCore import QEventLoop,QTimer
    from PySide6.QtWidgets import QApplication
    from slayer_r9t.gui import ControlCenter
    app=QApplication.instance() or QApplication([])
    with patch.object(ControlCenter,'poll'):
        window=ControlCenter()
        try:
            window.tabs.setCurrentIndex(window.dynamic_boost_index)
            window.poll_dynamic_boost()
            loop=QEventLoop();window.dynamic_boost_process.finished.connect(loop.quit)
            QTimer.singleShot(5000,loop.quit);loop.exec()
            assert window.dynamic_boost_process is None
            assert 'Destekleniyor' in window.dynamic_support.text() and 'Çalışıyor' in window.dynamic_daemon.text()
            state=request({'op':'status'});window.state=state;window.state_received=time.monotonic()
            window.poll_gpu()
            loop=QEventLoop();window.gpu_process.finished.connect(loop.quit)
            QTimer.singleShot(3000,loop.quit);loop.exec()
            assert window.gpu_values[2] is not None
            window.history=history;window.render_dynamic_boost_measurements()
            assert f"{state['cooling']['gpu']['enforced_power_w']:.1f} W" in window.dynamic_measurements.text()
            window.resize(1280,1100);window.show();app.processEvents()
            window.grab().save('previews/dynamic-boost-live.png')
            report['gui_worker_passed']=True
        finally:window.close()
    source=Path(__file__).resolve().parent.parent
    files=[*source.joinpath('slayer_r9t').glob('*.py'),source/'r9t-dynamic-boost.py',source/'r9t-gpu-diagnostics.py',source/'r9t-client.py']
    report['source_matches_installed']=all(p.read_bytes()==(installed/p.relative_to(source)).read_bytes() for p in files)
    assert report['source_matches_installed']
    report['service_active']=subprocess.run(['systemctl','is-active','--quiet','slayer-r9t.service']).returncode==0
    report['session_service_active']=subprocess.run(['systemctl','--user','is-active','--quiet','slayer-r9t-session.service']).returncode==0
    assert report['service_active'] and report['session_service_active']
    report.update(passed=True,csv_cpu_power_included=True)
    (source/'docs/dynamic-boost-validation.json').write_text(json.dumps(report,ensure_ascii=False,indent=2))
    print('Dynamic Boost support, daemon, package watts, GPU watts, GUI, CSV and installation passed. Power-transfer behavior remains unverified.')

if __name__=='__main__':run()
