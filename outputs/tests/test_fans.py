import unittest
from unittest.mock import Mock,patch
from slayer_r9t.fans import Fans,validate_curve,curve_target


class FanTests(unittest.TestCase):
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
