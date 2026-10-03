"""Offscreen form and persistence tests; no hardware, subprocesses or poll timers."""
import copy
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

os.environ.setdefault('QT_QPA_PLATFORM','offscreen')
from PySide6.QtWidgets import QApplication, QDialog, QMainWindow, QTabWidget, QLabel
from slayer_r9t.gui import ControlCenter, ProfileEditor
from slayer_r9t.settings import Settings


class DraftWindow(ControlCenter):
    def __init__(self, store):
        QMainWindow.__init__(self)
        self.store = store
        self.state = {'capabilities':{}}
        self.action_buttons = [];self.action_busy = False
        self.tabs = QTabWidget(self);self.setCentralWidget(self.tabs)
        self.operation_status = QLabel(self)
        self.command = Mock()
        self.build_profiles();self.build_automation()

    def closeEvent(self, event):
        QMainWindow.closeEvent(self,event)


class GuiProfileTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.store = Settings(Path(self.temporary.name)/'settings.json')
        self.store.profiles.update(Quiet={'cpu_max_mhz':2400},Game={'gpu_max_mhz':1700})
        self.store.data['automation']['apps'] = [
            {'executable':'/opt/first','profile':'Quiet'},
            {'executable':'/opt/second','profile':'Game'}]
        self.store.save()
        self.window = DraftWindow(self.store)
        self.window.profile_combo.setCurrentText('Quiet')

    def tearDown(self):
        self.window.close();self.window.deleteLater()
        self.app.sendPostedEvents()
        self.temporary.cleanup()

    @staticmethod
    def map():
        return [[i%256,(i*3)%256,(i*7)%256] for i in range(126)]

    def test_all_valid_profile_fields_roundtrip_without_hardware_capabilities(self):
        fan_profiles = [None,{'mode':'auto'},{'mode':'boost'},{'mode':'manual','cpu':65,'gpu':91},
                        {'mode':'preset','preset':'cool'},
                        {'mode':'curve','points':[[45,50],[85,100]]},
                        {'mode':'curve','points':[[40+i*5,50+i*5] for i in range(9)]+[[85,100]]}]
        for fan in fan_profiles:
            for lighting in ({'rgb':[10,20,30],'brightness':37},{'rgb_map':self.map(),'brightness':41}):
                profile = dict(power_profile='power-saver',cpu_epp='balance_power',boost_enabled=False,
                               cpu_max_mhz=5900,gpu_max_mhz=3900,**lighting)
                if fan:profile['fan'] = fan
                with self.subTest(fan=fan,lighting=list(lighting)):
                    dialog = ProfileEditor('Quiet',profile,self.window)
                    self.assertEqual(dialog.profile_settings(),profile)
                    dialog.deleteLater()

    def test_partial_profile_keeps_absent_fields_absent(self):
        for profile in ({'boost_enabled':False},{'fan':{'mode':'auto'}},{'cpu_epp':'power'}):
            dialog = ProfileEditor('Quiet',profile,self.window)
            self.assertEqual(dialog.profile_settings(),profile)
            dialog.deleteLater()

    def test_change_one_field_preserves_custom_map_and_curve(self):
        profile = {'cpu_max_mhz':3200,'rgb_map':self.map(),'brightness':70,
                   'fan':{'mode':'curve','points':[[45,50],[65,75],[85,100]]}}
        dialog = ProfileEditor('Quiet',profile,self.window)
        dialog.fields['cpu_max_mhz'].setValue(2500)
        self.assertEqual(dialog.profile_settings(),dict(profile,cpu_max_mhz=2500))
        dialog.profile_settings()['rgb_map'][0][0] = 255
        self.assertEqual(dialog.original,profile)
        dialog.deleteLater()

    def test_explicit_include_toggle_removes_only_chosen_field(self):
        dialog = ProfileEditor('Quiet',{'cpu_max_mhz':2500,'gpu_max_mhz':1300},self.window)
        dialog.includes['gpu_max_mhz'].setChecked(False)
        self.assertEqual(dialog.profile_settings(),{'cpu_max_mhz':2500})
        dialog.deleteLater()

    def test_invalid_empty_profile_does_not_accept(self):
        dialog = ProfileEditor('Quiet',{'cpu_max_mhz':2500},self.window)
        dialog.includes['cpu_max_mhz'].setChecked(False)
        dialog.accept()
        self.assertEqual(dialog.result(),QDialog.DialogCode.Rejected)
        self.assertTrue(dialog.error.text())
        dialog.deleteLater()

    def test_edit_persists_same_profile_and_automation_references(self):
        before = copy.deepcopy(self.store.data['automation'])
        def edit(dialog):
            self.assertTrue(dialog.name.isReadOnly())
            self.assertEqual(dialog.fields['cpu_max_mhz'].value(),2400)
            dialog.fields['cpu_max_mhz'].setValue(2000)
            return QDialog.DialogCode.Accepted
        with patch.object(ProfileEditor,'exec',edit):self.window.edit_profile()
        self.assertEqual(Settings(self.store.path).profiles['Quiet'],{'cpu_max_mhz':2000})
        self.assertEqual(self.store.data['automation'],before)
        self.window.command.assert_called_once()
        self.assertEqual(self.window.command.call_args.args[0],'configure')

    def test_copy_creates_new_profile_without_changing_source(self):
        def edit(dialog):
            self.assertFalse(dialog.name.isReadOnly())
            dialog.name.setText('Quiet copy')
            dialog.fields['cpu_max_mhz'].setValue(1700)
            return QDialog.DialogCode.Accepted
        with patch.object(ProfileEditor,'exec',edit):self.window.copy_profile()
        profiles = Settings(self.store.path).profiles
        self.assertEqual(profiles['Quiet'],{'cpu_max_mhz':2400})
        self.assertEqual(profiles['Quiet copy'],{'cpu_max_mhz':1700})
        self.assertEqual(self.window.profile_combo.currentText(),'Quiet copy')
        self.assertEqual(self.window.command.call_args.args[0],'configure')

    def test_cancel_leaves_document_disk_and_commands_unchanged(self):
        before = copy.deepcopy(self.store.data);disk = self.store.path.read_bytes()
        def cancel(dialog):
            dialog.fields['cpu_max_mhz'].setValue(1700)
            return QDialog.DialogCode.Rejected
        with patch.object(ProfileEditor,'exec',cancel):self.window.edit_profile()
        self.assertEqual(self.store.data,before)
        self.assertEqual(self.store.path.read_bytes(),disk)
        self.window.command.assert_not_called()

    def test_duplicate_copy_name_does_not_overwrite(self):
        before = copy.deepcopy(self.store.data)
        def edit(dialog):
            dialog.name.setText('Game')
            return QDialog.DialogCode.Accepted
        with patch.object(ProfileEditor,'exec',edit):self.window.copy_profile()
        self.assertEqual(self.store.data,before)
        self.assertIn('zaten kayıtlı',self.window.operation_status.text())
        self.window.command.assert_not_called()

    def test_save_failure_restores_in_memory_document_and_no_command(self):
        before = copy.deepcopy(self.store.data);disk = self.store.path.read_bytes()
        def edit(dialog):
            dialog.fields['cpu_max_mhz'].setValue(1700)
            return QDialog.DialogCode.Accepted
        with patch.object(ProfileEditor,'exec',edit),patch.object(self.store,'save',side_effect=OSError('disk full')):
            self.window.edit_profile()
        self.assertEqual(self.store.data,before)
        self.assertEqual(self.store.path.read_bytes(),disk)
        self.assertIn('disk full',self.window.operation_status.text())
        self.window.command.assert_not_called()

    def test_rule_move_stays_draft_until_save_and_preserves_selection(self):
        disk = self.store.path.read_bytes()
        self.window.rule_combos['battery'].setCurrentIndex(self.window.rule_combos['battery'].findData('Quiet'))
        self.window.app_list.setCurrentRow(1);self.window.move_selected_app_rule(-1)
        self.assertEqual(self.window.app_rules[0]['executable'],'/opt/second')
        self.assertEqual(self.window.app_list.currentRow(),0)
        self.assertEqual(self.store.path.read_bytes(),disk)
        self.window.command.assert_not_called()
        self.window.save_automation()
        self.assertEqual(Settings(self.store.path).data['automation']['apps'],self.window.app_rules)
        self.assertEqual(json.loads(self.window.command.call_args.args[1])['automation']['apps'],self.window.app_rules)

    def test_rule_edit_keeps_priority_and_draft_until_save(self):
        self.window.app_list.setCurrentRow(0)
        with patch('slayer_r9t.gui.QInputDialog.getText',return_value=('/opt/replaced',True)),patch('slayer_r9t.gui.QInputDialog.getItem',return_value=('Game',True)):
            self.window.edit_selected_app_rule()
        self.assertEqual(self.window.app_rules[0],{'executable':'/opt/replaced','profile':'Game'})
        self.assertEqual(self.window.app_rules[1]['executable'],'/opt/second')
        self.assertEqual(Settings(self.store.path).data['automation']['apps'][0]['executable'],'/opt/first')
        self.window.command.assert_not_called()

    def test_invalid_rule_edit_leaves_draft_and_disk_unchanged(self):
        before = copy.deepcopy(self.window.app_rules)
        self.window.app_list.setCurrentRow(0)
        with patch('slayer_r9t.gui.QInputDialog.getText',return_value=('/opt/second',True)),patch('slayer_r9t.gui.QInputDialog.getItem',return_value=('Game',True)):
            self.window.edit_selected_app_rule()
        self.assertEqual(self.window.app_rules,before)
        self.assertTrue(self.window.operation_status.text())
        self.window.command.assert_not_called()

    def test_rule_replacement_keeps_existing_priority(self):
        self.window.upsert_rule('/opt/first','Game')
        self.assertEqual(self.window.app_rules[0],{'executable':'/opt/first','profile':'Game'})
        self.assertEqual(self.window.app_list.currentRow(),0)
        self.window.command.assert_not_called()


if __name__ == '__main__':
    unittest.main()
