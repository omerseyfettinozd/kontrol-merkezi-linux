import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch,Mock
import subprocess
from slayer_r9t import dynamic_boost as boost
from slayer_r9t.power_monitor import PackagePower

class PackagePowerTests(unittest.TestCase):
    def setUp(self):
        temp=tempfile.TemporaryDirectory(); self.addCleanup(temp.cleanup)
        self.root=Path(temp.name);self.zone=self.root/'intel-rapl:0';self.zone.mkdir()
        (self.zone/'name').write_text('package-0');(self.zone/'energy_uj').write_text('10000000')
        (self.zone/'max_energy_range_uj').write_text('100000000')
        self.monitor=PackagePower(self.root)
    def energy(self,value): (self.zone/'energy_uj').write_text(str(value))
    def test_first_sample_missing_then_average(self):
        self.monitor.tick(now=0);self.assertIsNone(self.monitor.status(now=0)['watts'])
        self.energy(30000000);self.monitor.tick(now=2)
        self.assertEqual(self.monitor.status(now=2)['watts'],10);self.assertEqual(self.monitor.status(now=2)['interval_seconds'],2)
    def test_core_subzone_not_double_counted(self):
        child=self.root/'intel-rapl:0:0';child.mkdir()
        (child/'name').write_text('core');(child/'energy_uj').write_text('999999999')
        self.monitor.tick(now=0);self.energy(20000000);self.monitor.tick(now=1)
        self.assertEqual(self.monitor.status(now=1)['watts'],10);self.assertEqual(len(self.monitor.zones),1)
    def test_symlink_alias_not_double_counted(self):
        (self.root/'alias').symlink_to(self.zone)
        self.monitor.tick(now=0);self.energy(20000000);self.monitor.tick(now=1)
        self.assertEqual(self.monitor.status(now=1)['watts'],10)
    def test_counter_wrap(self):
        self.energy(99000000);self.monitor.tick(now=0);self.energy(1000000);self.monitor.tick(now=1)
        self.assertEqual(self.monitor.status(now=1)['watts'],2)
    def test_stale_and_suspend_are_gaps(self):
        self.monitor.tick(now=0);self.energy(20000000);self.monitor.tick(now=1)
        self.assertIsNone(self.monitor.status(now=7)['watts'])
        self.energy(40000000);self.monitor.tick(now=20);self.assertIsNone(self.monitor.status(now=20)['watts'])
        self.energy(45000000);self.monitor.tick(now=21);self.assertEqual(self.monitor.status(now=21)['watts'],5)
    def test_counter_reset_or_excess_is_missing(self):
        (self.zone/'max_energy_range_uj').write_text('65000000000')
        self.monitor.tick(now=0);self.energy(0);self.monitor.tick(now=1)
        self.assertIsNone(self.monitor.status(now=1)['watts']);self.assertIn('geçersiz',self.monitor.error)
    def test_permission_failure_never_falls_back_to_igpu_ppt(self):
        original=Path.read_text
        def read(path,*args,**kwargs):
            if path.name=='energy_uj':raise PermissionError('denied')
            return original(path,*args,**kwargs)
        with patch.object(Path,'read_text',read):self.monitor.tick(now=0)
        self.assertIsNone(self.monitor.status(now=0)['watts']);self.assertIn('denied',self.monitor.error)
    def test_missing_package_is_explicit(self):
        (self.zone/'name').write_text('core');self.monitor.tick(now=0)
        self.assertIsNone(self.monitor.status(now=0)['watts']);self.assertIn('bulunamadı',self.monitor.error)
    def test_multi_package_requires_all_samples(self):
        second=self.root/'intel-rapl:1';second.mkdir()
        for key,val in [('name','package-1'),('energy_uj','5000000'),('max_energy_range_uj','100000000')]: (second/key).write_text(val)
        self.monitor.tick(now=0);self.energy(20000000);(second/'energy_uj').write_text('8000000');self.monitor.tick(now=1)
        self.assertEqual(self.monitor.status(now=1)['watts'],13)
        (second/'energy_uj').unlink();self.energy(25000000);self.monitor.tick(now=2)
        self.assertIsNone(self.monitor.status(now=2)['watts'])

class BoostStatusTests(unittest.TestCase):
    def response(self,**changes):
        fields={'LoadState':'loaded','ActiveState':'active','SubState':'running','Result':'success',
                'ExecMainStatus':'0','NRestarts':'0','MainPID':'123','UnitFileState':'enabled'}
        fields.update(changes)
        return Mock(returncode=0,stderr='',stdout='\n'.join(k+'='+v for k,v in fields.items()))
    def test_healthy_daemon_does_not_verify_behavior(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);gpu=root/'0000:01:00.0';gpu.mkdir();(gpu/'power').write_text('Notebook Dynamic Boost: Supported\n')
            with patch.object(boost,'NVIDIA',root),patch.object(boost,'CPU_ROOT',root/'cpu'),patch.object(boost.subprocess,'run',return_value=self.response()) as run:
                state=boost.snapshot()
            self.assertTrue(state['devices'][0]['supported']);self.assertTrue(state['daemon']['healthy'])
            self.assertFalse(state['behavior_verified'])
            self.assertEqual(run.call_args.args[0][1:3],['show','nvidia-powerd.service'])
            self.assertNotIn('shell',run.call_args.kwargs)
    def test_failed_missing_and_exited_daemon_are_not_healthy(self):
        for change in ({'ActiveState':'failed','Result':'exit-code','ExecMainStatus':'1'},
                       {'LoadState':'not-found','MainPID':'0'}, {'SubState':'exited'}):
            with patch.object(boost.subprocess,'run',return_value=self.response(**change)):
                self.assertFalse(boost.daemon_status()['healthy'])
    def test_timeout_and_incomplete_reply_are_unknown(self):
        with patch.object(boost.subprocess,'run',side_effect=subprocess.TimeoutExpired('systemctl',2)):
            self.assertIsNone(boost.daemon_status()['healthy'])
        with patch.object(boost.subprocess,'run',return_value=Mock(returncode=0,stderr='',stdout='ActiveState=active')):
            self.assertIsNone(boost.daemon_status()['healthy'])
    def test_support_missing_false_and_unrecognized(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);gpu=root/'0000:01:00.0';gpu.mkdir()
            for text,value in [('Notebook Dynamic Boost: Not Supported',False),('Notebook Dynamic Boost: banana',None),('',None)]:
                (gpu/'power').write_text(text)
                with patch.object(boost,'NVIDIA',root),patch.object(boost.subprocess,'run',return_value=self.response()):
                    self.assertIs(boost.snapshot()['devices'][0]['supported'],value)

class BoostGuiTests(unittest.TestCase):
    def test_stale_cpu_sample_and_partial_gpu_values_are_missing(self):
        os.environ.setdefault('QT_QPA_PLATFORM','offscreen')
        from PySide6.QtWidgets import QApplication
        from slayer_r9t.gui import ControlCenter
        from slayer_r9t.settings import Settings
        app=QApplication.instance() or QApplication([])
        with tempfile.TemporaryDirectory() as temp,patch.object(ControlCenter,'poll'),patch('slayer_r9t.gui.Settings',return_value=Settings(Path(temp)/'settings.json')):
            window=ControlCenter()
            try:
                window.state={'cooling':{'cpu_power':{'watts':10,'sample_monotonic':10}}};window.state_received=16
                self.assertIsNone(window.cpu_power_sample(16))
                window.state_received=12;self.assertEqual(window.cpu_power_sample(12),10)
                process=Mock();process.readAllStandardOutput.return_value=b'60, 50, N/A\n';window.gpu_process=process
                window.gpu_done(process,0);self.assertEqual(window.gpu_values,[60,50,None])
                process=Mock();process.readAllStandardOutput.return_value=b'NaN, 101, inf\n';window.gpu_process=process
                window.gpu_done(process,0);self.assertEqual(window.gpu_values,[None,None,None])
                window.render_dynamic_boost_measurements()
                self.assertNotIn('tüm sistem tüketimi değildir)',window.dynamic_measurements.text())
            finally:window.close()
