"""Exercise acceptance cleanup after failed writes without touching hardware."""
import copy
import os
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch


class LiveAcceptanceRestoreTests(unittest.TestCase):
    def exercise_failure(self, rgb_state, fail_write, cleanup_failure=None, paused=False):
        initial = {'power_profile': 'power-saver', 'boost_enabled': False,
                   'rgb_state': rgb_state, 'capabilities': {'cpu_limit': True},
                   'cooling': {'cpu': {'max_mhz': 2000}},
                   'automation': {'paused': paused}}
        requests = []
        def request(payload):
            requests.append(copy.deepcopy(payload))
            if payload['op'] == 'status':
                if len(requests) > 1 and cleanup_failure == 'status':
                    raise RuntimeError('injected cleanup failure')
                result = copy.deepcopy(initial)
                if len(requests) > 1 and cleanup_failure == 'assert':
                    result['rgb_state']['brightness'] = 99
                return result
            if payload['op'] == 'profile':
                if len(requests) == 2:
                    if fail_write:
                        raise RuntimeError('injected write failure')
                    # Simulate a command which returned an unverified outcome.
                    return {'verified': False, 'state': {}}
                if cleanup_failure == 'profile':
                    raise RuntimeError('injected cleanup failure')
                return {'verified': True}
            return {}
        with patch.dict(os.environ, {'R9T_LIVE_CHECK': '1'}), \
             patch('slayer_r9t.protocol.request', side_effect=request), \
             patch('slayer_r9t.gui.ControlCenter') as window, \
             patch('PySide6.QtWidgets.QApplication') as app, \
             patch.object(Path, 'mkdir'), \
             patch.object(Path, 'write_text'):
            expected = RuntimeError if fail_write or cleanup_failure in ('profile', 'status') else AssertionError
            with self.assertRaises(expected):
                runpy.run_path(str(Path(__file__).parent / 'live_acceptance.py'), run_name='__main__')
            window.assert_not_called()
            app.assert_not_called()
        restored = requests[2]['settings']
        self.assertEqual(requests[2]['op'], 'profile')
        self.assertEqual(restored['power_profile'], initial['power_profile'])
        self.assertEqual(restored['boost_enabled'], initial['boost_enabled'])
        self.assertEqual(restored['cpu_max_mhz'], 2000)
        self.assertEqual(restored['brightness'], rgb_state['brightness'])
        self.assertEqual(requests[-1], {'op': 'automation_pause' if paused else 'automation_resume'})
        return restored

    def test_rgb_map_restored_after_write_or_verification_failure(self):
        rgb = {'rgb': [3, 4, 5], 'brightness': 64,
               'rgb_map': [[i, (i * 2) % 256, (i * 3) % 256] for i in range(126)]}
        for fail_write in (True, False):
            with self.subTest(fail_write=fail_write):
                restored = self.exercise_failure(rgb, fail_write)
                self.assertEqual(restored['rgb_map'], rgb['rgb_map'])
                self.assertNotIn('rgb', restored)

    def test_solid_color_restored_after_write_or_verification_failure(self):
        rgb = {'rgb': [3, 4, 5], 'brightness': 64}
        for fail_write in (True, False):
            with self.subTest(fail_write=fail_write):
                restored = self.exercise_failure(rgb, fail_write)
                self.assertEqual(restored['rgb'], rgb['rgb'])
                self.assertNotIn('rgb_map', restored)

    def test_automation_restored_even_when_hardware_cleanup_fails(self):
        for cleanup_failure in ('profile', 'status', 'assert'):
            for paused in (False, True):
                with self.subTest(cleanup_failure=cleanup_failure, paused=paused):
                    self.exercise_failure({'rgb': [3, 4, 5], 'brightness': 64},
                                          False, cleanup_failure, paused)
