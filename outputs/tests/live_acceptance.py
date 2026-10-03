"""Explicit opt-in hardware acceptance check; restores the initial settings."""
import json
import os
from pathlib import Path
import time

if os.environ.get('R9T_LIVE_CHECK') != '1':
    raise SystemExit('Gerçek donanım kontrolü için R9T_LIVE_CHECK=1 gerekli.')

from PySide6.QtCore import QTimer
from PySide6.QtWidgets import QApplication
from slayer_r9t.gui import ControlCenter
from slayer_r9t.protocol import request

ARTIFACTS = Path(__file__).resolve().parents[2]/'work'
ARTIFACTS.mkdir(exist_ok=True)

before = request({'op': 'status'})
rgb = before['rgb_state']
restore = {'power_profile': before['power_profile'], 'boost_enabled': before['boost_enabled'],
           'rgb': rgb['rgb'], 'brightness': rgb['brightness']}
if before['capabilities'].get('cpu_limit'):restore['cpu_max_mhz']=before['cooling']['cpu']['max_mhz']
report = {'before': restore, 'checks': []}
try:
    applied = request({'op': 'profile', 'settings': {'power_profile': 'balanced',
        'boost_enabled': True, 'rgb': [23,255,255], 'brightness': 51}})
    actual = applied['state']
    assert applied['verified'] is True
    assert actual['power_profile'] == 'balanced'
    assert actual['boost_enabled'] is True
    assert actual['rgb_state'] == {'rgb': [23,255,255], 'brightness': 52}
    report['checks'].append('Combined profile: power, all CPU boost policies, RGB quantization verified')
    for payload in ({'op':'manual','cpu':0,'gpu':0},{'op':'curve','points':[[45,30],[85,100]]}):
        try:
            request(payload)
            raise AssertionError('Invalid fan operation accepted')
        except RuntimeError:
            pass
    report['checks'].append('Unsafe fan targets rejected by root service')
    app = QApplication([])
    window = ControlCenter()
    window.show()
    outcomes = []
    def trigger():
        assert window.initialized
        assert window.tabs.count() == 13
        window.power_combo.setCurrentIndex(window.power_combo.findData('power-saver'))
        window.command('power', 'power-saver')
    def inspect():
        try:
            assert request({'op':'status'})['power_profile'] == 'power-saver'
            assert 'doğrulandı' in window.operation_status.text()
            assert 'Güç tasarrufu' in window.power_state.text()
            assert window.action_busy is False
            window.grab().save(str(ARTIFACTS/'professional-preview.png'))
            outcomes.append(True)
        finally:
            window.close()
            app.quit()
    QTimer.singleShot(2000, trigger)
    QTimer.singleShot(8500, inspect)
    app.exec()
    assert outcomes == [True]
    report['checks'].append('Eleven-tab GUI: asynchronous action, actual state, persistent feedback passed')
    events = request({'op': 'events'})['events']
    assert any(event['operation'] == 'profile' and event['ok'] for event in events)
    assert any(event['operation'] == 'manual' and not event['ok'] for event in events)
    report['checks'].append('Successful and rejected actions recorded in diagnostics')
finally:
    request({'op':'profile', 'settings': restore})
    after = request({'op':'status'})
    assert after['power_profile'] == restore['power_profile']
    assert after['boost_enabled'] == restore['boost_enabled']
    assert after['rgb_state'] == {'rgb':restore['rgb'], 'brightness':restore['brightness']}
    report['restored'] = True
    request({'op':'automation_pause' if before.get('automation',{}).get('paused') else 'automation_resume'})
    (ARTIFACTS/'live-acceptance.json').write_text(json.dumps(report, ensure_ascii=False, indent=2))
print(json.dumps(report, ensure_ascii=False, indent=2))
