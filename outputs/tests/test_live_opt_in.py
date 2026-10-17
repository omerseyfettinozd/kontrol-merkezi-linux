"""Verify hardware checks refuse to start unless explicitly opted in."""
import os
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch


SCRIPTS = ('live_camera.py', 'live_lighting.py', 'live_extensions.py')


class LiveOptInTests(unittest.TestCase):
    def test_denied_checks_never_access_hardware_or_gui(self):
        for name in SCRIPTS:
            for value in (None, '0', 'true'):
                with self.subTest(script=name, value=value):
                    environment = {} if value is None else {'R9T_LIVE_CHECK': value}
                    with patch.dict(os.environ, environment, clear=True), \
                         patch('slayer_r9t.protocol.request') as request, \
                         patch('slayer_r9t.gui.ControlCenter') as window, \
                         patch('PySide6.QtWidgets.QApplication') as app, \
                         patch.object(Path, 'read_text') as read, \
                         patch.object(Path, 'write_text') as write, \
                         patch.object(Path, 'iterdir') as listing, \
                         patch.object(Path, 'glob') as glob:
                        with self.assertRaisesRegex(SystemExit, 'R9T_LIVE_CHECK=1'):
                            runpy.run_path(str(Path(__file__).parent / name), run_name='__main__')
                        for operation in (request, window, app, read, write, listing, glob):
                            operation.assert_not_called()

    def test_opted_in_checks_reach_initial_status_request(self):
        for name in SCRIPTS:
            with self.subTest(script=name), \
                 patch.dict(os.environ, {'R9T_LIVE_CHECK': '1'}), \
                 patch('slayer_r9t.protocol.request', side_effect=RuntimeError('mock stop')) as request:
                with self.assertRaisesRegex(RuntimeError, 'mock stop'):
                    runpy.run_path(str(Path(__file__).parent / name), run_name='__main__')
                request.assert_called_once_with({'op': 'status'})
