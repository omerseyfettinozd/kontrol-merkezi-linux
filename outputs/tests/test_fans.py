import unittest
from unittest.mock import Mock,patch
from slayer_r9t.fans import Fans,validate_curve,curve_target
from slayer_r9t.hardware import Platform


class FanTests(unittest.TestCase):
    def test_handover_uses_bridge_even_when_telemetry_is_unavailable(self):
        device=Platform.__new__(Platform)
        device.fans=Mock()
        device.fans.has_control.return_value=True
        device.fans.auto.side_effect=OSError('EC unavailable')
        device.fd=123
        with patch('slayer_r9t.hardware.io.read_int') as raw_read, patch('slayer_r9t.hardware.fcntl.ioctl') as raw_write:
            with self.assertRaises(OSError):device.auto()
        raw_read.assert_not_called();raw_write.assert_not_called()
        device.fans.auto.assert_called_once()

    def test_auto_already_active_does_not_write_ec(self):
        fans=Fans();fans.mode='manual';fans.targets=[70,80];fans.failure_since=1
        fans.send=Mock();fans.read=Mock(return_value={'mode':0,'mode_byte':160,'cpu_rpm':0,'gpu_rpm':0})
        self.assertTrue(fans.auto()['verified'])
        fans.send.assert_not_called()
        self.assertEqual(fans.snapshot(),{'mode':'auto'})
        self.assertIsNone(fans.targets);self.assertIsNone(fans.failure_since)

    def test_manual_to_auto_writes_once_and_checks_fresh_read(self):
        fans=Fans();fans.send=Mock();fans.read=Mock(side_effect=[{'mode':1,'mode_byte':192},{'mode':0,'mode_byte':128}])
        self.assertTrue(fans.auto()['verified'])
        fans.send.assert_called_once_with('auto')
        self.assertEqual(fans.read.call_count,2)

    def test_failed_initial_read_still_attempts_handover(self):
        fans=Fans();fans.send=Mock();fans.read=Mock(side_effect=[OSError('read failed'),{'mode':0,'mode_byte':128}])
        self.assertTrue(fans.auto()['verified'])
        fans.send.assert_called_once_with('auto')

    def test_failed_auto_write_does_not_resume_manual_lease(self):
        fans=Fans();fans.mode='manual';fans.targets=[70,80];fans.verified=True
        fans.read=Mock(return_value={'mode':1,'mode_byte':192})
        fans.send=Mock(side_effect=OSError('write failed'))
        with self.assertRaises(OSError):fans.auto()
        self.assertFalse(fans.verified);self.assertEqual(fans.mode,'auto')
        self.assertIsNone(fans.targets)
        fans.monitor()
        fans.send.assert_called_once_with('auto')

    def test_auto_does_not_claim_success_with_manual_driver_or_bit(self):
        for measured in ({'mode':1,'mode_byte':128},{'mode':0,'mode_byte':192}):
            fans=Fans();fans.send=Mock();fans.read=Mock(return_value=measured)
            with self.assertRaises(RuntimeError):fans.auto()
            self.assertFalse(fans.verified)

    def test_auto_status_follows_current_measurement_without_writes(self):
        fans=Fans();fans.send=Mock()
        with patch('slayer_r9t.fans.ROOT') as root:
            (root/'status').is_file.return_value=True
            (root/'control').exists.return_value=True
            fans.read=Mock(side_effect=[{'mode':0,'mode_byte':128},
                                       {'mode':1,'mode_byte':192},OSError('unavailable'),
                                       {'mode':0,'mode_byte':128}])
            self.assertTrue(fans.status()['verified'])
            mismatch=fans.status()
            self.assertFalse(mismatch['verified']);self.assertTrue(mismatch['verification_error'])
            unavailable=fans.status()
            self.assertFalse(unavailable['verified']);self.assertEqual(unavailable['error'],'unavailable')
            recovered=fans.status()
            self.assertTrue(recovered['verified']);self.assertEqual(recovered['error'],'')
            self.assertEqual(fans.read.call_count,4)
        fans.send.assert_not_called()

    def test_curve_validation(self):
        for points in ([],[[45,40],[85,100]],[[60,70],[50,100]],[[45,80],[85,70]],[[45,50],[95,100]],[[True,50],[85,100]]):
            with self.assertRaises(ValueError):validate_curve(points)
        self.assertEqual(validate_curve([[45,50],[85,100]]),[[45,50],[85,100]])

    def test_curve_interpolation(self):
        points=[[45,50],[65,70],[85,100]]
        self.assertEqual(curve_target(points,35),50)
        self.assertEqual(curve_target(points,55),60)
        self.assertEqual(curve_target(points,95),100)

    def test_invalid_manual_no_write(self):
        fans=Fans();fans.send=Mock()
        for target in (0,49,101,True):
            with self.assertRaises(ValueError):fans.apply('manual',{'cpu':target,'gpu':70})
        fans.send.assert_not_called()

    def test_verified_requires_pwm_and_rpm(self):
        fans=Fans();fans.send=Mock();fans.read=Mock(return_value={'mode':1,'mode_byte':192,'cpu_pwm':140,'gpu_pwm':160,'cpu_rpm':4200,'gpu_rpm':4700,'thermal':0})
        fans.mode='manual';fans.targets=[70,80];fans.started=0
        with patch('slayer_r9t.fans.time.monotonic',return_value=10):
            fans.monitor();self.assertFalse(fans.verified)
            fans.monitor();self.assertTrue(fans.verified)

    def test_firmware_mode_change_cancels(self):
        fans=Fans();fans.mode='manual';fans.targets=[70,70]
        fans.read=Mock(return_value={'mode':0,'mode_byte':128});fans.auto=Mock()
        with patch('slayer_r9t.fans.time.monotonic',return_value=10):fans.monitor()
        self.assertEqual(fans.mode,'auto');self.assertFalse(fans.verified)
        self.assertIn('otomatiğe döndü',fans.error)
        fans.auto.assert_called_once()

    def test_failed_measurement_returns_auto(self):
        fans=Fans();fans.mode='manual';fans.targets=[70,80];fans.started=0;fans.failure_since=1
        fans.send=Mock();fans.auto=Mock()
        fans.read=Mock(return_value={'mode':1,'mode_byte':192,'cpu_pwm':100,'gpu_pwm':100,'cpu_rpm':0,'gpu_rpm':0,'thermal':0})
        with patch('slayer_r9t.fans.time.monotonic',return_value=20):fans.monitor()
        fans.auto.assert_called_once();self.assertEqual(fans.mode,'auto')


if __name__=='__main__':unittest.main()
