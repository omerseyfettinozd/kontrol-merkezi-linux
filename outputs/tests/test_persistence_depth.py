"""Corrupt deeply nested JSON must not prevent settings/policy initialization."""
import copy
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from slayer_r9t.settings import Settings,parse_document_json,MAX_JSON_DEPTH
from slayer_r9t.automation import Automation


class PersistenceDepthTests(unittest.TestCase):
    def setUp(self):
        temporary=tempfile.TemporaryDirectory();self.addCleanup(temporary.cleanup)
        self.root=Path(temporary.name)
        self.nested='['*60000+'0'+']'*60000

    def test_settings_recovers_and_archives_deep_document_without_decoding(self):
        path=self.root/'settings.json';path.write_text(self.nested)
        with patch('slayer_r9t.settings.json.loads',side_effect=AssertionError('decoder must not run')):
            settings=Settings(path)
        self.assertEqual(settings.profiles,{})
        self.assertIn('64',settings.warning)
        archived=list(self.root.glob('settings.json.invalid-*'))
        self.assertEqual(len(archived),1)
        self.assertEqual(archived[0].read_text(),self.nested)

    def test_policy_records_error_and_keeps_default_document(self):
        path=self.root/'policy.json';path.write_text(self.nested)
        policy=Automation(SimpleNamespace(),1234,path)
        self.assertEqual(policy.document['profiles'],{})
        self.assertIn('64',policy.error)
        self.assertEqual(path.read_text(),self.nested)

    def test_import_rejects_deep_document_without_changing_saved_settings(self):
        settings=Settings(self.root/'settings.json')
        settings.profiles['Quiet']={'cpu_max_mhz':2400};settings.save()
        before=copy.deepcopy(settings.data);disk=settings.path.read_bytes()
        incoming=self.root/'incoming.json';incoming.write_text(self.nested)
        with self.assertRaisesRegex(ValueError,'64'):settings.import_file(incoming)
        self.assertEqual(settings.data,before)
        self.assertEqual(settings.path.read_bytes(),disk)

    def test_brackets_and_escaped_quotes_in_strings_are_not_depth(self):
        value={'label':'['*100+'\\"'+']'*100,'values':[1]}
        self.assertEqual(parse_document_json(json.dumps(value)),value)
        within='['*MAX_JSON_DEPTH+'0'+']'*MAX_JSON_DEPTH
        self.assertIsInstance(parse_document_json(within),list)
        with self.assertRaisesRegex(ValueError,'64'):
            parse_document_json('['*(MAX_JSON_DEPTH+1)+'0'+']'*(MAX_JSON_DEPTH+1))

    def test_size_and_decoder_recursion_failure_are_controlled_errors(self):
        with self.assertRaisesRegex(ValueError,'büyük'):parse_document_json(' '*131073)
        with patch('slayer_r9t.settings.json.loads',side_effect=RecursionError('stack')):
            with self.assertRaisesRegex(ValueError,'derin'):parse_document_json('{}')
