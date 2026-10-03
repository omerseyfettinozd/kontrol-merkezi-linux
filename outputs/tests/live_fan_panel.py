"""Opt-in fan button acceptance and offscreen panel capture."""
import os
from pathlib import Path
from PySide6.QtCore import QTimer
from PySide6.QtWidgets import QApplication
from slayer_r9t.gui import ControlCenter
from slayer_r9t.protocol import request

if os.environ.get('R9T_LIVE_CHECK')!='1':raise SystemExit('R9T_LIVE_CHECK=1 gerekli.')
app=QApplication([])
window=ControlCenter()
window.resize(1080,920)
window.tabs.setCurrentIndex(2)
window.show()
errors=[]
def activate():
    try:
        assert window.initialized
        window.fan_cpu.setValue(80);window.fan_gpu.setValue(70)
        button=next(button for button,capability in window.action_buttons if capability=='manual_fan')
        assert button.isEnabled()
        button.click()
    except Exception as exc:errors.append(repr(exc));app.quit()
def inspect():
    try:
        fans=request({'op':'status'})['fans']
        assert fans['verified'] and fans['targets']==[80,70],fans
        assert 'doğrulandı' in window.fan_validation.text(),window.fan_validation.text()
        assert 'doğrulandı' in window.operation_status.text(),window.operation_status.text()
        assert 'CPU %80 / GPU %70' in window.fan_targets.text()
        assert not window.action_busy
        capture=Path(__file__).resolve().parents[2]/'work/fan-panel-preview.png'
        window.grab().save(str(capture))
    except Exception as exc:errors.append(repr(exc))
    finally:window.close();app.quit()
QTimer.singleShot(2500,activate)
QTimer.singleShot(17000,inspect)
try:app.exec()
finally:request({'op':'auto'})
assert not errors,errors
print('Fan panel button, target/RPM feedback and capture passed; automatic mode restored.')
