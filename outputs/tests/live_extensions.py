"""Explicit live EPP, stable curve and GUI history checks; restore initial state."""
import json,time,os
from pathlib import Path
from slayer_r9t.protocol import request
from slayer_r9t import telemetry

def fan_snapshot(state):
    fan=state['fans'];mode=fan['mode'];value={'mode':mode}
    if mode=='manual':value.update(cpu=fan['targets'][0],gpu=fan['targets'][1])
    if mode=='curve':value['points']=fan['points']
    return value

def run():
    before=request({'op':'status'})
    original={'fan':fan_snapshot(before),'cpu_epp':before['cooling']['epp']['value']}
    report={'version':before['version'],'epp':[],'fan_samples':[]}
    try:
        assert before['version'] in ('0.7.0','0.8.0','0.9.0','0.10.0', '0.11.0')
        for value in before['cooling']['epp']['choices']:
            result=request({'op':'profile','settings':{'cpu_epp':value}})
            assert result['verified']
            time.sleep(1)
            state=request({'op':'status'})
            assert state['cooling']['epp']['value']==value
            assert state['power_profile']==before['power_profile']
            assert state['boost_enabled']==before['boost_enabled']
            assert state['cooling']['cpu']['max_mhz']==before['cooling']['cpu']['max_mhz']
            report['epp'].append({'value':value,'policy_count':state['cooling']['epp']['policy_count'],'verified':True})
        request({'op':'profile','settings':{'cpu_epp':original['cpu_epp']}})
        try:request({'op':'profile','settings':{'cpu_epp':'performance','gpu_max_mhz':500}})
        except RuntimeError as exc:
            assert 'Önceki ayarlar geri yüklendi' in str(exc)
        else:raise AssertionError('Invalid GPU bound accepted')
        assert request({'op':'status'})['cooling']['epp']['value']==original['cpu_epp']
        report['epp_transaction_rollback']=True
        request({'op':'curve','points':[[45,50],[65,70],[85,100]]})
        for _ in range(12):
            time.sleep(3)
            fan=request({'op':'status'})['fans']
            assert fan['mode']=='curve'
            assert not fan['error'] or (fan['measured'] or {}).get('thermal')
            assert fan['measured']['cpu_rpm']>500 and fan['measured']['gpu_rpm']>500
            report['fan_samples'].append(fan)
        assert any(s['verified'] for s in report['fan_samples'])
        report['sensors']=telemetry.storage_memory_sensors()
        assert len({s['id'].split(':temp')[0] for s in report['sensors'] if s['driver']=='nvme'})==2
        assert len([s for s in report['sensors'] if s['driver']=='spd5118'])==2
    finally:
        request({'op':'profile','settings':original})
        if not before['automation']['paused']:request({'op':'automation_resume'})
    after=request({'op':'status'})
    assert after['cooling']['epp']['value']==original['cpu_epp']
    assert fan_snapshot(after)==original['fan']
    assert after['rgb_state']==before['rgb_state']
    assert after['features']['camera']['value']==before['features']['camera']['value']
    report['restored']=True
    report['hardware_passed']=True
    Path(__file__).resolve().parents[1].joinpath('docs/extensions-validation.json').write_text(json.dumps(report,indent=2))
    from PySide6.QtCore import QTimer
    from PySide6.QtWidgets import QApplication
    from slayer_r9t.gui import ControlCenter
    os.environ.setdefault('QT_QPA_PLATFORM','offscreen')
    app=QApplication([]);window=ControlCenter();window.show()
    errors=[]
    def finish():
        try:
            assert window.tabs.count()==13
            assert window.epp_combo.count()==4
            assert len(window.history.rows)>=4
            assert len(window.sensor_cards)==4
            docs=Path(__file__).resolve().parents[1]/'docs'
            count=window.history.export(docs/'history-live.csv',300)
            assert count>=4
            window.tabs.setCurrentIndex(11);app.processEvents();window.grab().save(str(docs/'history-live.png'))
            window.tabs.setCurrentIndex(12);app.processEvents();window.grab().save(str(docs/'sensors-live.png'))
            report['gui']={'tabs':13,'history_rows':count,'sensor_cards':4,'epp_choices':4}
        except Exception as exc:errors.append(exc)
        finally:window.close();app.quit()
    QTimer.singleShot(15000,finish);app.exec()
    if errors:raise errors[0]
    report['passed']=True
    Path(__file__).resolve().parents[1].joinpath('docs/extensions-validation.json').write_text(json.dumps(report,indent=2))
    print(json.dumps({k:v for k,v in report.items() if k not in ('fan_samples','sensors')},indent=2))
if __name__=='__main__':run()
