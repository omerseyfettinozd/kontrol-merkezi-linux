"""Offscreen static color drafts; every device command is replaced by a mock."""
import copy
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

os.environ.setdefault('QT_QPA_PLATFORM','offscreen')
from PySide6.QtGui import QColor
from PySide6.QtWidgets import QApplication, QDialog, QLabel, QMainWindow, QPushButton, QTabWidget
from slayer_r9t.gui import ControlCenter, LightingEditor, ProfileEditor
from slayer_r9t.lighting import ROWS, COLUMNS, pattern
from slayer_r9t.settings import Settings


class KeyboardWindow(ControlCenter):
    def __init__(self, store):
        QMainWindow.__init__(self)
        self.store = store;self.state = {'capabilities':{}}
        self.action_buttons = [];self.action_busy = False;self.initialized = True
        self.tabs = QTabWidget(self);self.setCentralWidget(self.tabs)
        self.operation_status = QLabel(self);self.color = QColor('#112233')
        self.command = Mock()
        self.build_power();self.build_fans();self.build_cooling()
        self.build_keyboard();self.build_profiles();self.build_automation()

    def closeEvent(self, event):QMainWindow.closeEvent(self,event)


class GuiRgbTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.app = QApplication.instance() or QApplication([])

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.store = Settings(Path(self.temp.name)/'settings.json')
        self.store.profiles['Existing'] = {'cpu_max_mhz':2500}
        self.store.save();self.window = KeyboardWindow(self.store)

    def tearDown(self):
        self.window.close();self.window.deleteLater();self.app.sendPostedEvents();self.temp.cleanup()

    @staticmethod
    def colors():return [[i%256,(i*3)%256,(i*7)%256] for i in range(ROWS*COLUMNS)]

    def test_cell_click_changes_only_chosen_coordinate_and_not_input(self):
        original = self.colors();editor = LightingEditor(original,self.window)
        editor.color = QColor(200,70,30)
        editor.cells[2*COLUMNS+7].click()
        expected = self.colors();expected[2*COLUMNS+7] = [200,70,30]
        self.assertEqual(editor.colors(),expected)
        self.assertEqual(original,self.colors())
        self.assertEqual(editor.cells[2*COLUMNS+7].toolTip(),'Satır 3, sütun 8')
        editor.deleteLater()

    def test_region_paints_inclusive_rectangle_normalizing_reversed_bounds(self):
        editor = LightingEditor(self.colors(),self.window);editor.color = QColor(4,5,6)
        for key,value in {'row_start':3,'row_end':2,'col_start':5,'col_end':3}.items():editor.region[key].setValue(value)
        editor.region_button.click()
        expected = self.colors()
        for row in (1,2):
            for col in (2,3,4):expected[row*COLUMNS+col] = [4,5,6]
        self.assertEqual(editor.colors(),expected);editor.deleteLater()

    def test_region_whole_matrix_and_last_coordinate_bounds(self):
        editor = LightingEditor(self.colors(),self.window);editor.color = QColor(4,5,6)
        editor.paint_region();self.assertEqual(editor.colors(),[[4,5,6] for _ in range(126)])
        editor.color = QColor(1,2,3);editor.cells[-1].click()
        self.assertEqual(editor.colors()[-1],[1,2,3]);self.assertEqual(editor.colors()[-2],[4,5,6])
        editor.deleteLater()

    def test_existing_presets_load_exact_pattern_values(self):
        editor = LightingEditor(self.colors(),self.window);editor.color = QColor(100,150,200)
        for key in ('uniform','rainbow','zones','gradient'):
            editor.presets.setCurrentIndex(editor.presets.findData(key));editor.load_preset()
            expected = [[100,150,200] for _ in range(126)] if key=='uniform' else pattern(key,[100,150,200])
            self.assertEqual(editor.colors(),expected)
        editor.deleteLater()

    def test_clear_reset_and_returned_map_are_independent(self):
        original = self.colors();editor = LightingEditor(original,self.window)
        editor.clear_colors();self.assertEqual(editor.colors(),[[0,0,0] for _ in range(126)])
        editor.reset_colors();self.assertEqual(editor.colors(),original)
        returned = editor.colors();returned[0][0] = 255
        self.assertEqual(editor.colors(),original)
        editor.draft[1][0] = 255;editor.reset_colors();self.assertEqual(editor.colors(),original)
        editor.deleteLater()

    def test_malformed_maps_are_rejected_before_editing(self):
        for colors in ([[]],[[0,0,0]]*125,[[0,0,True]]*126):
            with self.assertRaises(ValueError):LightingEditor(colors,self.window)

    @staticmethod
    def accept_painted(editor):
        editor.color = QColor(25,100,200);editor.cells[12].click()
        return QDialog.DialogCode.Accepted

    def test_accept_only_updates_draft_until_explicit_apply_button(self):
        self.window.brightness.setValue(61)
        with patch.object(LightingEditor,'exec',self.accept_painted):self.window.edit_rgb_map()
        self.window.command.assert_not_called()
        self.assertEqual(self.window.light_pattern.currentData(),'custom')
        settings = self.window.lighting_settings();self.assertEqual(settings['rgb_map'][12],[25,100,200])
        self.assertEqual(settings['brightness'],61)
        self.window.state['capabilities']['rgb_map'] = True;self.window.update_buttons()
        button = next(b for b in self.window.findChildren(QPushButton) if b.text()=='Seçilen renk düzenini uygula')
        self.assertTrue(button.isEnabled());button.click()
        self.assertEqual(self.window.command.call_args.args[0],'profile')
        self.assertEqual(json.loads(self.window.command.call_args.args[1]),settings)

    def test_cancel_preserves_existing_custom_draft_disk_and_no_device_command(self):
        with patch.object(LightingEditor,'exec',self.accept_painted):self.window.edit_rgb_map()
        before = self.window.lighting_settings();disk = self.store.path.read_bytes()
        def cancel(editor):editor.clear_colors();return QDialog.DialogCode.Rejected
        with patch.object(LightingEditor,'exec',cancel):self.window.edit_rgb_map()
        self.assertEqual(self.window.lighting_settings(),before)
        self.assertEqual(self.store.path.read_bytes(),disk);self.window.command.assert_not_called()

    def test_custom_map_is_saved_in_combined_profile_without_pattern_regeneration(self):
        with patch.object(LightingEditor,'exec',self.accept_painted):self.window.edit_rgb_map()
        expected = self.window.lighting_settings()
        with patch('slayer_r9t.gui.QInputDialog.getText',return_value=('Painted',True)):self.window.save_profile()
        saved = Settings(self.store.path).profiles['Painted']
        self.assertEqual(saved['rgb_map'],expected['rgb_map'])
        self.assertEqual(saved['brightness'],expected['brightness'])
        self.assertNotIn('rgb',saved)
        self.assertEqual(self.window.command.call_args.args[0],'configure')

    def test_switching_existing_pattern_does_not_destroy_custom_draft(self):
        with patch.object(LightingEditor,'exec',self.accept_painted):self.window.edit_rgb_map()
        original = self.window.lighting_settings()
        self.window.light_pattern.setCurrentIndex(self.window.light_pattern.findData('zones'))
        self.assertEqual(self.window.lighting_settings()['rgb_map'],pattern('zones',self.window.rgb_values()))
        self.window.light_pattern.setCurrentIndex(self.window.light_pattern.findData('custom'))
        self.assertEqual(self.window.lighting_settings(),original)

    def test_nested_profile_editor_accept_changes_map_preserving_other_fields(self):
        profile = {'cpu_max_mhz':2700,'rgb_map':self.colors(),'brightness':43,'fan':{'mode':'auto'}}
        editor = ProfileEditor('Existing',profile,self.window)
        with patch.object(LightingEditor,'exec',self.accept_painted):editor.edit_lighting()
        expected = copy.deepcopy(profile);expected['rgb_map'][12] = [25,100,200]
        self.assertEqual(editor.profile_settings(),expected)
        self.assertEqual(profile['rgb_map'],self.colors())
        self.window.command.assert_not_called();editor.deleteLater()

    def test_nested_profile_editor_cancel_preserves_map_and_parent_profile(self):
        profile = {'rgb_map':self.colors(),'brightness':43}
        editor = ProfileEditor('Existing',profile,self.window)
        def cancel(dialog):dialog.clear_colors();return QDialog.DialogCode.Rejected
        with patch.object(LightingEditor,'exec',cancel):editor.edit_lighting()
        self.assertEqual(editor.profile_settings(),profile)
        self.window.command.assert_not_called();editor.deleteLater()


if __name__ == '__main__':unittest.main()
