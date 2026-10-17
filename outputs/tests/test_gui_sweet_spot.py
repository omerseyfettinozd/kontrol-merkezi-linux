"""Preset preview and capability gates; no live hardware writes."""
import os
os.environ.setdefault('QT_QPA_PLATFORM','offscreen')
import unittest
from unittest.mock import Mock
from PySide6.QtCore import QEvent
from PySide6.QtWidgets import QApplication,QMainWindow,QTabWidget
from slayer_r9t.gui import ControlCenter
from slayer_r9t.cooling import PRESETS

class CoolingWindow(ControlCenter):
    def __init__(self):
        QMainWindow.__init__(self)
        self.state={'cooling':{'presets':PRESETS},'capabilities':dict.fromkeys(('power','boost','cpu_limit','gpu_limit','cpu_epp','curve'),True)}
        self.tabs=QTabWidget(self);self.setCentralWidget(self.tabs)
        self.action_buttons=[];self.action_busy=False;self.command=Mock()
        self.build_cooling()
        for key,preset in PRESETS.items():self.thermal_combo.addItem(preset['name'],key)
        self.thermal_combo.setCurrentIndex(self.thermal_combo.findData('sweet_spot'))
    def closeEvent(self,event):QMainWindow.closeEvent(self,event)

class SweetSpotGuiTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.app=QApplication.instance() or QApplication([])
    def setUp(self):self.win=CoolingWindow()
    def tearDown(self):self.win.close();self.win.deleteLater();self.app.sendPostedEvents(None,QEvent.Type.DeferredDelete)
    def button(self):return next(button for button,cap in self.win.action_buttons if cap=='thermal_preset')
    def test_preview_includes_measured_scope_and_applied_fan_energy_settings(self):
        text=self.win.thermal_description.text()
        for expected in ('5 kısa','%4,9','Gerçek oyun FPS','4900','2475','Serin fan eğrisi','Dengeli performans'):
            self.assertIn(expected,text)
        self.button().click();self.win.command.assert_called_once_with('thermal_preset','sweet_spot')
    def test_missing_fan_or_energy_support_blocks_selected_preset_but_not_legacy(self):
        for cap in ('curve','cpu_epp'):
            self.win.state['capabilities'][cap]=False;self.win.update_buttons()
            self.assertFalse(self.button().isEnabled())
            self.win.thermal_combo.setCurrentIndex(self.win.thermal_combo.findData('gaming_cool'))
            self.assertTrue(self.button().isEnabled())
            self.win.state['capabilities'][cap]=True
            self.win.thermal_combo.setCurrentIndex(self.win.thermal_combo.findData('sweet_spot'))
    def test_selected_preset_gate_does_not_block_other_thermal_actions(self):
        self.win.state['capabilities']['curve']=False;self.win.update_buttons()
        self.assertFalse(self.button().isEnabled())
        others=[button for button,cap in self.win.action_buttons if cap=='thermal']
        self.assertTrue(others);self.assertTrue(all(button.isEnabled() for button in others))
    def test_busy_state_blocks_profile_action(self):
        self.win.action_busy=True;self.win.update_buttons();self.assertFalse(self.button().isEnabled())
