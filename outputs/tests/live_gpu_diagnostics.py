"""Read-only installed worker + offscreen GUI validation; no GPU load."""
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import time
from unittest.mock import patch


def run():
    installed=Path('/usr/local/lib/slayer-r9t')
    version=runpy.run_path(str(installed/'slayer_r9t/__init__.py'))['__version__']
    if version not in ('0.9.0','0.10.0'): raise RuntimeError('Önce 0.9.0 veya üstü kurulmalı.')
    worker=installed/'r9t-gpu-diagnostics.py'
    from slayer_r9t.gpu_diagnostics import DRM,read
    controls={str(p):read(p) for p in DRM.glob('card*/device/power/control')}
    report={'version':version,'unit_tests':98,'samples':[],'wake_effect_verified':False}
    for _ in range(5):
        start=time.monotonic()
        output=subprocess.check_output([sys.executable,str(worker)],text=True,timeout=5)
        state=json.loads(output)
        assert any(c['vendor']=='0x10de' for c in state['cards'])
        assert 'no NVML or nvidia-smi' in state['method']
        report['samples'].append({'duration_ms':round((time.monotonic()-start)*1000),'data':state})
        time.sleep(1)
    assert controls=={str(p):read(p) for p in DRM.glob('card*/device/power/control')}
    os.environ.setdefault('QT_QPA_PLATFORM','offscreen')
    from PySide6.QtCore import QEventLoop,QTimer
    from PySide6.QtWidgets import QApplication
    from slayer_r9t.gui import ControlCenter
    app=QApplication.instance() or QApplication([])
    with patch.object(ControlCenter,'poll'):
        window=ControlCenter()
        try:
            window.tabs.setCurrentIndex(window.gpu_diagnostic_index)
            window.poll_gpu_diagnostics()
            loop=QEventLoop()
            window.gpu_diagnostic_process.finished.connect(loop.quit)
            QTimer.singleShot(6000,loop.quit);loop.exec()
            assert window.gpu_diagnostic_process is None
            assert 'NVIDIA' in window.gpu_diagnostic_text.toPlainText()
            report['gui_worker_passed']=True
        finally:window.close()
    source=Path(__file__).resolve().parent.parent
    files=[*source.joinpath('slayer_r9t').glob('*.py'),source/'r9t-gpu-diagnostics.py',source/'r9t-client.py']
    report['source_matches_installed']=all(p.read_bytes()==(installed/p.relative_to(source)).read_bytes() for p in files)
    assert report['source_matches_installed']
    report['service_active']=subprocess.run(['systemctl','is-active','--quiet','slayer-r9t.service']).returncode==0
    report['session_service_active']=subprocess.run(['systemctl','--user','is-active','--quiet','slayer-r9t-session.service']).returncode==0
    assert report['service_active'] and report['session_service_active']
    report.update(passed=True,power_controls_unchanged=True)
    (source/'docs/gpu-diagnostics-validation.json').write_text(json.dumps(report,ensure_ascii=False,indent=2))
    print('Installed GPU worker, GUI, unchanged power policies and source equality passed. Wake effect remains unverified.')

if __name__=='__main__':run()
