import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch
from slayer_r9t import cooling, hardware
from slayer_r9t.fans import Fans
from slayer_r9t.temperature_target import TemperatureTarget, recover_cpu
from slayer_r9t.protocol import validate

class FakeFans:
    def __init__(self):
        self.mode='curve'; self.targets=[70,70]; self.temps=[65,65]
        self.available=True; self.driver_mode=1; self.thermal=0; self.error=''
    def snapshot(self): return {'mode':'curve','points':[[45,50],[85,100]]}
    def status(self):
        return {'available':self.available,'mode':self.mode,'targets':self.targets,'error':self.error,
                'measured':{'cpu_temp':self.temps[0],'gpu_temp':self.temps[1],'mode':self.driver_mode,
                            'mode_byte':192 if self.driver_mode else 0,'thermal':self.thermal}}
    def apply(self,op,req): self.mode='manual'; self.targets=[req['cpu'],req['gpu']]
    def update_manual_targets(self,targets): self.targets=list(targets)
    def apply_profile(self,value): self.mode=value['mode']
    def auto(self): self.mode='auto'

class TargetTests(unittest.TestCase):
    def setUp(self):
        temp=tempfile.TemporaryDirectory(); self.addCleanup(temp.cleanup)
        self.root=Path(temp.name)/'cpu'; self.root.mkdir()
        for i in range(2):
            p=self.root/f'policy{i}'; p.mkdir()
            for key,value in {'scaling_driver':'amd-pstate-epp','amd_pstate_max_freq':'4000000',
                'scaling_min_freq':'400000','scaling_max_freq':'3500000','cpuinfo_max_freq':'4000000'}.items():
                (p/key).write_text(value)
        patcher=patch.object(cooling,'CPU_ROOT',self.root); patcher.start(); self.addCleanup(patcher.stop)
        self.fans=FakeFans(); self.gpu=Mock(requested_max=2000)
        self.gpu.status.return_value={'available':True,'max_mhz':2800}
        def set_gpu(value): self.gpu.requested_max=value or None
        self.gpu.set_max.side_effect=set_gpu
        self.journal=Path(temp.name)/'recovery.json'
        self.target=TemperatureTarget(self.fans,self.gpu,self.journal)
    def heat(self):
        self.target.start(75,72); self.fans.temps=[85,85]; self.target.tick(now=0)
    def test_invalid_target_no_writes(self):
        for cpu,gpu in [(True,72),(59,72),(86,72),(75,81),(75,float('nan'))]:
            with self.assertRaises(ValueError): self.target.start(cpu,gpu)
        self.assertFalse(self.journal.exists()); self.gpu.set_max.assert_not_called()
    def test_missing_sensor_preflight(self):
        self.fans.temps[0]=None
        with self.assertRaises(RuntimeError): self.target.start(75,72)
        self.assertFalse(self.journal.exists()); self.assertEqual(self.fans.mode,'curve')
    def test_heating_cooling_and_original_ceiling(self):
        self.heat(); self.assertEqual(self.fans.targets,[100,100]); self.assertEqual(self.target.limits,[3400,1900])
        self.target.tick(now=1); self.assertEqual(self.target.limits,[3400,1900])
        self.target.tick(now=2); self.assertEqual(self.target.limits,[3300,1800])
        self.fans.temps=[60,60]
        for now in range(4,14,2): self.target.tick(now=now)
        self.assertEqual(self.target.limits,[3300,1800])
        for now in range(14,30,2): self.target.tick(now=now)
        self.assertEqual(self.target.limits,[3500,2000])
        self.target.stop(); self.assertEqual(self.fans.mode,'curve'); self.assertFalse(self.journal.exists())
    def test_sensor_loss_emergency_restores(self):
        self.heat(); self.fans.temps[1]=0; self.target.tick(now=2)
        self.assertFalse(self.target.active); self.assertEqual(self.fans.mode,'auto')
        self.assertEqual(cooling.cpu_status()['max_mhz'],3500); self.assertEqual(self.gpu.requested_max,2000)
    def test_suspend_and_gap_stop(self):
        for mode,delay in [(0,2),(1,20)]:
            self.fans.driver_mode=1; self.target.start(75,72); self.target.tick(now=0)
            self.fans.driver_mode=mode; self.target.tick(now=delay)
            self.assertFalse(self.target.active); self.assertEqual(self.fans.mode,'auto')
    def test_gpu_failure_rolls_back_cpu(self):
        def fail(value):
            if value==1900: raise RuntimeError('injected GPU error')
            self.gpu.requested_max=value
        self.gpu.set_max.side_effect=fail; self.heat()
        self.assertFalse(self.target.active); self.assertIn('injected',self.target.error)
        self.assertEqual(cooling.cpu_status()['max_mhz'],3500); self.assertFalse(self.journal.exists())
    def test_failed_restore_retains_journal_for_retry(self):
        self.target.start(75,72)
        with patch.object(cooling,'restore_cpu',side_effect=OSError('restore failed')):
            with self.assertRaises(RuntimeError): self.target.stop()
        self.assertTrue(self.journal.exists()); self.assertTrue(self.target.status()['restore_pending'])
        self.target.stop(); self.assertFalse(self.journal.exists())
    def test_crash_restores_mixed_limits(self):
        (self.root/'policy1/scaling_max_freq').write_text('3200000'); self.heat()
        recover_cpu(self.journal)
        self.assertEqual({p.parent.name:int(v) for p,v in cooling.cpu_snapshot()},{'policy0':3500000,'policy1':3200000})
    def test_bad_journal_blocks_start(self):
        self.journal.write_text('{"../../tmp/x": 3500000}')
        target=TemperatureTarget(self.fans,self.gpu,self.journal)
        with self.assertRaises(RuntimeError): target.start(75,72)
        self.assertTrue(self.journal.exists()); self.assertEqual(self.fans.mode,'curve')
    def test_external_clock_change_stops(self):
        self.target.start(75,72); (self.root/'policy0/scaling_max_freq').write_text('2800000')
        self.target.tick(now=0); self.assertFalse(self.target.active); self.assertIn('başka',self.target.error)
    def test_thermal_override_both_fans(self):
        self.target.start(75,72); self.fans.thermal=1; self.fans.temps=[96,60]; self.target.tick(now=0)
        self.assertEqual(self.fans.targets,[100,100]); self.assertEqual(self.target.limits,[3400,2000])
    def test_floors(self):
        self.heat()
        for now in range(2,100,2): self.target.tick(now=now)
        self.assertEqual(self.target.limits,[1000,600])
    def test_manual_profile_stops_before_new_settings(self):
        self.heat(); device=hardware.Platform.__new__(hardware.Platform)
        device.match=True; device.temperature_target=self.target; device.fans=self.fans; device.gpu=self.gpu
        device.status=Mock(return_value={})
        device.dispatch({'op':'profile','settings':{'cpu_max_mhz':2400}})
        self.assertFalse(self.target.active); self.assertEqual(cooling.cpu_status()['max_mhz'],2400)
        self.assertEqual(self.gpu.requested_max,2000)
    def test_rpc_rejects_extra_path(self):
        validate({'version':1,'id':'test','op':'temperature_target','cpu':75,'gpu':72})
        with self.assertRaises(ValueError):
            validate({'version':1,'id':'test','op':'temperature_target','cpu':75,'gpu':72,'path':'/tmp/x'})
    def test_fan_failure_deadline_survives_adjustments(self):
        fans=Fans(); fans.mode='manual'; fans.failure_since=123; fans.send=Mock()
        fans.update_manual_targets([90,100]); self.assertEqual(fans.failure_since,123)
