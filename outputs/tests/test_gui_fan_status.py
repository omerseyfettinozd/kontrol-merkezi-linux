import unittest
from slayer_r9t.gui import fan_status_text


class FanStatusTests(unittest.TestCase):
    def test_auto_uses_fresh_ec_measurement_even_with_older_service(self):
        state={'available':True,'mode':'auto','verified':True,
               'measured':{'mode':1,'mode_byte':192}}
        self.assertIn('EC kontrolü doğrulanamadı',fan_status_text(state))
        state['verified']=False
        state['measured']={'mode':0,'mode_byte':128,'cpu_rpm':0,'gpu_rpm':0}
        self.assertIn('EC kontrolü doğrulandı',fan_status_text(state))

    def test_missing_telemetry_does_not_claim_auto_verification(self):
        self.assertIn('EC kontrolü doğrulanamadı',fan_status_text({'available':True,'mode':'auto','verified':True}))

    def test_manual_feedback_and_errors_are_preserved(self):
        text=fan_status_text({'available':True,'mode':'manual','verified':True,'error':'test error'})
        self.assertIn('Hedef / RPM doğrulandı',text)
        self.assertIn('test error',text)
