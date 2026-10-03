"""Draft rule edits preserve explicit first-match priority and atomic validation."""
import copy
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

from slayer_r9t.automation import Automation
from slayer_r9t.settings import (default_automation, edit_app_rule, move_app_rule,
                                upsert_app_rule, Settings)


class AppRuleEditingTests(unittest.TestCase):
    def setUp(self):
        self.profiles={'A':{'gpu_max_mhz':1200},'B':{'gpu_max_mhz':1800},
                       'POWER':{'power_profile':'balanced'}}
        self.rules=default_automation()
        self.rules.update(startup='POWER',ac='POWER',battery='POWER',
                          low_hz_on_battery=True,lighting_timeout=60,
                          apps=[{'executable':'/usr/bin/first','profile':'A'},
                                {'executable':'/usr/bin/second','profile':'B'}])

    def test_edit_preserves_other_rules_and_automation_fields_without_aliases(self):
        original=copy.deepcopy(self.rules)
        result=edit_app_rule(self.rules,self.profiles,0,'/usr/bin/new','B')
        self.assertEqual(result['apps'],[{'executable':'/usr/bin/new','profile':'B'},original['apps'][1]])
        for key in set(self.rules)-{'apps'}:
            self.assertEqual(result[key],original[key])
        result['apps'][1]['profile']='A'
        self.assertEqual(self.rules,original)

    def test_reorder_moves_only_selected_rule_and_keeps_metadata(self):
        original=copy.deepcopy(self.rules)
        moved=move_app_rule(self.rules,self.profiles,1,0)
        self.assertEqual(moved['apps'],list(reversed(original['apps'])))
        self.assertEqual(move_app_rule(moved,self.profiles,0,1),original)
        self.assertEqual(move_app_rule(self.rules,self.profiles,0,0),original)
        self.assertEqual(self.rules,original)

    def test_move_uses_final_list_position_and_keeps_other_relative_order(self):
        self.rules['apps'].append({'executable':'/usr/bin/third','profile':'A'})
        original=copy.deepcopy(self.rules['apps'])
        moved=move_app_rule(self.rules,self.profiles,0,2)
        self.assertEqual(moved['apps'],[original[1],original[2],original[0]])
        self.assertEqual(move_app_rule(moved,self.profiles,2,0)['apps'],original)

    def test_add_duplicate_updates_existing_position_instead_of_reprioritizing(self):
        result=upsert_app_rule(self.rules,self.profiles,'/usr/bin/first','B')
        self.assertEqual(result['apps'],[{'executable':'/usr/bin/first','profile':'B'},self.rules['apps'][1]])
        added=upsert_app_rule(result,self.profiles,'/usr/bin/third','A')
        self.assertEqual(added['apps'][:2],result['apps'])
        self.assertEqual(added['apps'][2],{'executable':'/usr/bin/third','profile':'A'})

    def test_invalid_edits_are_atomic_and_never_call_writer(self):
        original=copy.deepcopy(self.rules)
        with patch('slayer_r9t.settings.atomic_json') as writer:
            for index,path,profile in ((-1,'/usr/bin/new','A'),(2,'/usr/bin/new','A'),
                                       (True,'/usr/bin/new','A'),(0,'relative','A'),
                                       (0,'/usr/bin/second','A'),(0,'/usr/bin/new','missing'),
                                       (0,'/usr/bin/new',[])):
                with self.subTest(index=index,path=path,profile=profile):
                    with self.assertRaises(ValueError):
                        edit_app_rule(self.rules,self.profiles,index,path,profile)
            for index,destination in ((-1,0),(0,2),(True,0),(0,False)):
                with self.assertRaises(ValueError):
                    move_app_rule(self.rules,self.profiles,index,destination)
            writer.assert_not_called()
        self.assertEqual(self.rules,original)

    def test_add_limit_rejects_new_rule_but_allows_updating_existing_slot(self):
        self.rules['apps']=[{'executable':f'/usr/bin/app-{index}','profile':'A'} for index in range(32)]
        with self.assertRaises(ValueError):
            upsert_app_rule(self.rules,self.profiles,'/usr/bin/extra','B')
        result=upsert_app_rule(self.rules,self.profiles,'/usr/bin/app-10','B')
        self.assertEqual(len(result['apps']),32)
        self.assertEqual(result['apps'][10],{'executable':'/usr/bin/app-10','profile':'B'})

    def test_draft_changes_write_only_on_explicit_settings_save(self):
        with tempfile.TemporaryDirectory() as folder:
            store=Settings(Path(folder)/'settings.json')
            store.data.update(profiles=self.profiles,automation=self.rules)
            with patch('slayer_r9t.settings.atomic_json') as writer:
                store.data['automation']=move_app_rule(store.data['automation'],store.profiles,1,0)
                writer.assert_not_called()
                store.save()
                writer.assert_called_once()
                self.assertEqual(writer.call_args.args[1]['automation']['apps'][0]['executable'],'/usr/bin/second')

    def test_configure_reorder_changes_running_priority_without_losing_snapshot(self):
        with tempfile.TemporaryDirectory() as folder:
            device=Mock()
            device.dispatch.return_value={'message':'applied'}
            engine=Automation(device,1234,Path(folder)/'policy.json')
            engine.session=True
            engine.snapshot=Mock(return_value={'gpu_max_mhz':900})
            rules=copy.deepcopy(self.rules)
            rules.update(startup=None,ac=None,battery=None)
            document={'schema':3,'profiles':self.profiles,'automation':rules}
            with patch('slayer_r9t.automation.atomic_json') as writer:
                engine.configure(document)
                running={'/usr/bin/first','/usr/bin/second'}
                engine.tick(now=10,source='ac',running=running)
                document['automation']=move_app_rule(rules,self.profiles,1,0)
                engine.configure(document)
                engine.tick(now=12,source='ac',running=running)
                engine.tick(now=14,source='ac',running=set())
                self.assertEqual(writer.call_count,2)
                self.assertEqual([call.args[0]['settings'] for call in device.dispatch.call_args_list],
                                 [{'gpu_max_mhz':1200},{'gpu_max_mhz':1800},{'gpu_max_mhz':900}])
                engine.snapshot.assert_called_once()

    def test_invalid_configure_keeps_document_and_never_writes(self):
        with tempfile.TemporaryDirectory() as folder:
            engine=Automation(Mock(),1234,Path(folder)/'policy.json')
            previous=copy.deepcopy(engine.document)
            rules=copy.deepcopy(self.rules)
            rules['apps'][1]['executable']=rules['apps'][0]['executable']
            with patch('slayer_r9t.automation.atomic_json') as writer:
                with self.assertRaises(ValueError):
                    engine.configure({'schema':3,'profiles':self.profiles,'automation':rules})
                writer.assert_not_called()
            self.assertEqual(engine.document,previous)


if __name__=='__main__':
    unittest.main()
