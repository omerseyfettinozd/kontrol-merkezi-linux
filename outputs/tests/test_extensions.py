import csv
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch,Mock
from slayer_r9t import cooling,hardware,telemetry
from slayer_r9t.fans import CurveSmoother,Fans
from slayer_r9t.history import History

class CurveStabilityTests(unittest.TestCase):
    def test_heating_is_immediate(self):
        smoother=CurveSmoother(60,60)
        self.assertEqual(smoother.update(100,90,1),100)

    def test_small_temperature_jitter_holds_duty(self):
        smoother=CurveSmoother(80,75)
        for stamp in range(20):self.assertEqual(smoother.update(78,74,stamp),80)

    def test_cooling_hold_and_gradual_ramp(self):
        smoother=CurveSmoother(80,75)
        self.assertEqual(smoother.update(60,65,0),80)
        self.assertEqual(smoother.update(60,65,5),80)
        for stamp,duty in [(6,78),(8,76),(10,74),(12,72)]:self.assertEqual(smoother.update(60,65,stamp),duty)
        self.assertEqual(smoother.update(90,80,14),90)

    def test_interrupted_cooling_restarts_hold(self):
        smoother=CurveSmoother(80,75)
        smoother.update(60,65,0);smoother.update(79,74,4)
        self.assertEqual(smoother.update(60,65,6),80)
        self.assertEqual(smoother.update(60,65,12),78)

    def test_thermal_override_has_no_ramp(self):
        fan=Fans();fan.mode='curve';fan.targets=[60,60];fan.points=[[45,50],[85,100]]
        fan.smoothers=[CurveSmoother(60,55),CurveSmoother(60,55)]
        fan.read=Mock(return_value={'mode':1,'mode_byte':192,'cpu_temp':96,'gpu_temp':55,'thermal':1})
        fan.send=Mock()
        with patch('slayer_r9t.fans.time.monotonic',return_value=10):fan.monitor()
        fan.send.assert_called_once_with('manual 100 100')
        self.assertEqual(fan.targets,[100,100])

class HistoryTests(unittest.TestCase):
    def test_missing_and_nonfinite_are_gaps(self):
        history=History();history.add({'cpu_temp':65,'gpu_temp':float('nan'),'gpu_power':float('inf'),'cpu_usage':True},now=0)
        row=history.rows[0]
        self.assertEqual(row['cpu_temp'],65)
        for key in ('gpu_temp','gpu_power','cpu_usage','cpu_rpm'):self.assertIsNone(row[key])

    def test_real_time_window_and_pruning(self):
        history=History()
        for stamp in (0,100,1800,1900):history.add({'cpu_temp':stamp},now=stamp)
        self.assertEqual([r['monotonic'] for r in history.window(300,now=1900)],[1800,1900])
        self.assertEqual([r['monotonic'] for r in history.rows],[100,1800,1900])

    def test_export_only_selected_window(self):
        history=History();history.add({'cpu_temp':60},now=0,utc='old');history.add({'cpu_temp':70},now=500,utc='new')
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp)/'history.csv';self.assertEqual(history.export(path,300,now=500),1)
            with path.open() as stream:rows=list(csv.DictReader(stream))
            self.assertEqual(rows[0]['utc'],'new');self.assertEqual(rows[0]['gpu_temp'],'')

class EppTests(unittest.TestCase):
    def make_policies(self,root):
        for i in range(2):
            p=root/f'policy{i}';p.mkdir()
            for key,value in {'scaling_driver':'amd-pstate-epp','energy_performance_available_preferences':'performance balance_performance balance_power power custom','energy_performance_preference':'power'}.items():(p/key).write_text(value)

    def test_all_policies_and_restore(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);self.make_policies(root)
            with patch.object(cooling,'CPU_ROOT',root):
                snapshot=cooling.epp_snapshot();cooling.set_epp('balance_power')
                self.assertEqual(cooling.epp_status()['value'],'balance_power')
                cooling.restore_epp(snapshot);self.assertEqual(cooling.epp_status()['value'],'power')

    def test_unsupported_choice_never_writes(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);self.make_policies(root)
            with patch.object(cooling,'CPU_ROOT',root):
                for value in (True,'custom','default','/tmp/x',None):
                    with self.assertRaises(ValueError):cooling.set_epp(value)
                self.assertEqual(cooling.epp_status()['value'],'power')

    def test_partial_write_failure_restores_every_policy(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);self.make_policies(root);original=Path.write_text
            def write(path,value,*args,**kwargs):
                if path.parent.name=='policy1' and value=='performance':raise OSError('injected failure')
                return original(path,value,*args,**kwargs)
            with patch.object(cooling,'CPU_ROOT',root),patch.object(Path,'write_text',write):
                with self.assertRaises(OSError):cooling.set_epp('performance')
                self.assertEqual(cooling.epp_status()['value'],'power')

    def test_gpu_failure_restores_epp(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);self.make_policies(root);gpu=Mock(requested_max=None);gpu.set_max.side_effect=RuntimeError('unsupported')
            with patch.object(cooling,'CPU_ROOT',root):
                with self.assertRaisesRegex(RuntimeError,'Önceki ayarlar geri yüklendi'):hardware.apply_profile({'cpu_epp':'performance','gpu_max_mhz':1200},gpu)
                self.assertEqual(cooling.epp_status()['value'],'power')

    def test_unavailable_preflight_preserves_power(self):
        with patch.object(cooling,'epp_status',return_value={'choices':[]}),patch.object(hardware,'set_power_profile') as write:
            with self.assertRaises(RuntimeError):hardware.apply_profile({'power_profile':'balanced','cpu_epp':'power'})
            write.assert_not_called()

class SensorTests(unittest.TestCase):
    def test_only_supported_sensors_and_labels(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            for i,driver in enumerate(('nvme','spd5118','other')):
                p=root/f'hwmon{i}';p.mkdir();(p/'name').write_text(driver);(p/'temp1_input').write_text('42500')
                (p/'temp1_label').write_text('Composite')
            with patch.object(telemetry,'HWMON',root):
                sensors=telemetry.storage_memory_sensors()
                self.assertEqual(len(sensors),2)
                self.assertTrue(all(v['celsius']==42.5 for v in sensors))
                self.assertTrue(sensors[0]['name'].endswith('Composite'))

class GuiHistoryTests(unittest.TestCase):
    def test_rpc_responses_do_not_reset_history_or_sensor_cards(self):
        import os
        os.environ.setdefault('QT_QPA_PLATFORM','offscreen')
        from PySide6.QtWidgets import QApplication
        from slayer_r9t.gui import ControlCenter
        from slayer_r9t.settings import Settings
        app=QApplication.instance() or QApplication([])
        with tempfile.TemporaryDirectory() as temp,patch.object(ControlCenter,'poll'),patch('slayer_r9t.gui.Settings',return_value=Settings(Path(temp)/'settings.json')):
            window=ControlCenter()
            try:
                history=window.history;history.add({'cpu_temp':65});cards=window.sensor_cards
                cards['key']=object();window.gpu_received=123
                for code in (0,1):
                    process=Mock();process.readAllStandardOutput.return_value=b'{}';process.readAllStandardError.return_value=b'injected failure'
                    window.process=process
                    with patch.object(window,'update_state'):window.complete(process,'status',False,code)
                    self.assertIs(window.history,history);self.assertEqual(len(history.rows),1)
                    self.assertIs(window.sensor_cards,cards);self.assertIn('key',cards)
                    self.assertEqual(window.gpu_received,123)
            finally:window.close()
