"""Tests for newly integrated panels in the main GUI window."""
import unittest
from PySide6.QtWidgets import QApplication
from slayer_r9t.gui import ControlCenter
from slayer_r9t.processes_widget import ProcessesPanel
from slayer_r9t.audio_widget import AudioPanel
from slayer_r9t.compare_widget import ComparePanelWidget
from slayer_r9t.sleepdiag_widget import SleepDiagPanel

app = QApplication.instance() or QApplication([])

class GuiNewPanelsTest(unittest.TestCase):
    def setUp(self):
        self.win = ControlCenter()

    def tearDown(self):
        self.win.timer.stop()
        if hasattr(self.win, 'processes_panel'):
            self.win.processes_panel.timer.stop()
        if hasattr(self.win, 'compare_panel'):
            self.win.compare_panel.timer.stop()
        self.win.close()

    def test_new_tabs_exist_and_instantiated(self):
        tab_names = [self.win.tabs.tabText(i) for i in range(self.win.tabs.count())]
        self.assertIn('Kaynaklar', tab_names)
        self.assertIn('Ses', tab_names)
        self.assertIn('Karşılaştırma', tab_names)
        self.assertIn('Uyku ve pil', tab_names)

        self.assertIsInstance(self.win.processes_panel, ProcessesPanel)
        self.assertIsInstance(self.win.audio_panel, AudioPanel)
        self.assertIsInstance(self.win.compare_panel, ComparePanelWidget)
        self.assertIsInstance(self.win.sleepdiag_panel, SleepDiagPanel)
