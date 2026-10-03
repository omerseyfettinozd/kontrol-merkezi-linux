import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch, Mock
from slayer_r9t import cooling, hardware


class CoolingTests(unittest.TestCase):
    def policies(self, root):
        for i in range(2):
            policy = root/f'policy{i}'
            policy.mkdir()
            for name, value in {'scaling_driver':'amd-pstate-epp','amd_pstate_max_freq':'5386000',
                                'scaling_min_freq':'421798','scaling_max_freq':'2401000',
                                'cpuinfo_max_freq':'5386000'}.items():
                (policy/name).write_text(value)

    def test_cpu_cap_all_policies_and_restore(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.policies(root)
            with patch.object(cooling,'CPU_ROOT',root):
                snapshot = cooling.cpu_snapshot()
                cooling.set_cpu_max(1800)
                self.assertEqual(cooling.cpu_status()['max_mhz'],1800)
                cooling.restore_cpu(snapshot)
                self.assertEqual(cooling.cpu_status()['max_mhz'],2401)

    def test_invalid_cpu_caps_no_changes(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.policies(root)
            with patch.object(cooling,'CPU_ROOT',root):
                for value in (True,-1,500,7000):
                    with self.assertRaises(ValueError):cooling.set_cpu_max(value)
                self.assertEqual(cooling.cpu_status()['max_mhz'],2401)

    def test_reset_uses_driver_ceiling(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.policies(root)
            with patch.object(cooling,'CPU_ROOT',root):
                cooling.set_cpu_max(0)
                self.assertEqual(cooling.cpu_status()['max_mhz'],5386)

    def test_profile_validates_thermal_fields(self):
        for value in (False,-100,8000,'1800'):
            with self.assertRaises(ValueError):hardware.validate_profile({'cpu_max_mhz':value})
        for preset in cooling.PRESETS.values():hardware.validate_profile(preset['settings'])

    def test_gpu_failure_rolls_cpu_back(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.policies(root)
            gpu = Mock(requested_max=None)
            gpu.set_max.side_effect = RuntimeError('GPU unsupported')
            with patch.object(cooling,'CPU_ROOT',root):
                with self.assertRaisesRegex(RuntimeError,'Önceki ayarlar geri yüklendi'):
                    hardware.apply_profile({'cpu_max_mhz':1800,'gpu_max_mhz':1200},gpu)
                self.assertEqual(cooling.cpu_status()['max_mhz'],2401)

    def test_gpu_command_marker_and_reset(self):
        with tempfile.TemporaryDirectory() as temp:
            gpu = cooling.Gpu.__new__(cooling.Gpu)
            gpu.lib = Mock()
            gpu.lib.nvmlDeviceSetGpuLockedClocks.return_value = 0
            gpu.lib.nvmlDeviceResetGpuLockedClocks.return_value = 0
            gpu.handle = None
            gpu.requested_max = None
            gpu.status = lambda:{'available':True,'max_mhz':3090}
            marker = Path(temp)/'marker'
            with patch.object(cooling,'GPU_MARKER',marker):
                gpu.set_max(1800)
                self.assertEqual(marker.read_text(),'1800')
                self.assertEqual(gpu.requested_max,1800)
                gpu.set_max(0)
                self.assertFalse(marker.exists())
                self.assertIsNone(gpu.requested_max)


if __name__=='__main__':unittest.main()
