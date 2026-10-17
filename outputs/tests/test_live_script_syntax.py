"""Compile opt-in acceptance scripts without importing or running hardware checks."""
from pathlib import Path
import unittest
from unittest.mock import patch


class LiveScriptSyntaxTests(unittest.TestCase):
    def test_all_live_scripts_compile(self):
        scripts = sorted(Path(__file__).resolve().parent.glob('live_*.py'))
        self.assertTrue(scripts, 'No live acceptance scripts found')
        for script in scripts:
            with self.subTest(script=script.name):
                compile(script.read_bytes(), str(script), 'exec')

    def test_live_gui_contract_matches_current_window(self):
        from PySide6.QtCore import QTimer
        from PySide6.QtWidgets import QApplication
        from slayer_r9t.gui import ControlCenter
        from acceptance_gui_contract import verify_gui_tabs
        app = QApplication.instance() or QApplication([])
        with patch.object(ControlCenter, 'poll'):
            window = ControlCenter()
            try:
                verify_gui_tabs(window)
            finally:
                for timer in window.findChildren(QTimer):
                    timer.stop()
                window.close()
