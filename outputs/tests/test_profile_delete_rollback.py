"""Preserve saved automation and unsaved rule drafts after a failed deletion."""
import copy
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from PySide6.QtWidgets import QApplication
from slayer_r9t.settings import Settings
from test_gui_profiles import DraftWindow


class ProfileDeleteRollbackTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def test_failed_save_preserves_profile_rules_and_distinct_unsaved_draft(self):
        with tempfile.TemporaryDirectory() as directory:
            store = Settings(Path(directory) / 'settings.json')
            store.profiles.update(Quiet={'cpu_max_mhz': 2400}, Game={'gpu_max_mhz': 1700})
            store.data['automation'].update(
                startup='Quiet', ac='Quiet', battery='Quiet',
                apps=[{'executable': '/opt/saved', 'profile': 'Quiet'},
                      {'executable': '/opt/game', 'profile': 'Game'}])
            store.save()
            window = DraftWindow(store)
            try:
                window.profile_combo.setCurrentText('Quiet')
                window.app_rules.append({'executable': '/opt/unsaved', 'profile': 'Quiet'})
                window.refresh_rules()
                saved = copy.deepcopy(store.data)
                draft = copy.deepcopy(window.app_rules)
                list_text = [window.app_list.item(i).text() for i in range(window.app_list.count())]
                disk = store.path.read_bytes()
                with patch('slayer_r9t.settings.atomic_json', side_effect=OSError('disk full')):
                    window.delete_profile()
                self.assertEqual(store.data, saved)
                self.assertEqual(window.app_rules, draft)
                self.assertEqual(store.path.read_bytes(), disk)
                self.assertEqual([window.app_list.item(i).text()
                                  for i in range(window.app_list.count())], list_text)
                self.assertEqual(window.profile_combo.currentText(), 'Quiet')
                self.assertIn('disk full', window.operation_status.text())
                window.command.assert_not_called()
            finally:
                window.close()
                window.deleteLater()
                self.app.sendPostedEvents()
