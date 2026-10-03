import json,tempfile,unittest
from pathlib import Path
from unittest.mock import Mock,patch
from slayer_r9t.settings import Settings,default_automation,validate_document
from slayer_r9t.automation import Automation
from slayer_r9t.fans import validate_fan
from slayer_r9t import hardware,desktop,features,session

class Profiles(unittest.TestCase):
    def test_schema2_backup_and_roundtrip(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'settings.json'
            original={'schema':2,'profiles':{'Old':{'boost_enabled':False}}}
            path.write_text(json.dumps(original));store=Settings(path)
            self.assertEqual(store.data['schema'],3)
            self.assertEqual(json.loads(path.with_name('settings.json.schema2-backup').read_text()),original)
            self.assertEqual(store.data['automation'],default_automation())
            store.profiles['Fan']={'fan':{'mode':'manual','cpu':80,'gpu':70}}
            store.save();self.assertEqual(Settings(path).profiles,store.profiles)
    def test_fan_validation(self):
        for value in ({'mode':'manual','cpu':49,'gpu':70},{'mode':'auto','path':'/tmp'},{'mode':'curve','points':[[50,70],[80,80]]}):
            with self.assertRaises(ValueError):validate_fan(value)
    def test_missing_rule_profile(self):
        rules=default_automation();rules['ac']='Missing'
        with self.assertRaises(ValueError):validate_document({'schema':3,'profiles':{},'automation':rules})
    def test_fan_not_ready_before_other_writes(self):
        fans=Mock();fans.status.return_value={'available':False}
        with patch.object(hardware,'set_power_profile') as write:
            with self.assertRaises(RuntimeError):hardware.apply_profile({'power_profile':'balanced','fan':{'mode':'auto'}},fans=fans)
            write.assert_not_called()
    def test_fan_failure_restores_power_and_auto(self):
        fans=Mock();fans.status.return_value={'available':True};fans.snapshot.return_value={'mode':'auto'}
        fans.apply_profile.side_effect=[RuntimeError('fan failure'),{'verified':True}]
        with patch.object(hardware,'get_power_profile',return_value='power-saver'),patch.object(hardware,'set_power_profile') as setter:
            with self.assertRaises(RuntimeError):hardware.apply_profile({'power_profile':'balanced','fan':{'mode':'boost'}},fans=fans)
            self.assertEqual([c.args[0] for c in setter.call_args_list],['balanced','power-saver'])
            self.assertEqual(fans.apply_profile.call_args.args[0],{'mode':'auto'})
    def test_unavailable_charge_does_not_write(self):
        with patch.object(features,'status',return_value={'charge_limit':{'available':False,'start_available':False,'reason':'missing'}}),patch.object(Path,'write_text') as write:
            with self.assertRaises(RuntimeError):features.charge_limit(80)
            write.assert_not_called()
    def test_idle_light_restores_original(self):
        device=hardware.Platform.__new__(hardware.Platform);device.idle_rgb=None
        original={'rgb':[255,129,3],'brightness':100}
        with patch.object(hardware,'get_rgb',side_effect=[original,dict(original,brightness=0)]),patch.object(hardware,'set_rgb') as setter:
            device.lighting_idle(True);device.lighting_idle(False)
            self.assertEqual([c.args for c in setter.call_args_list],[([255,129,3],0),([255,129,3],100)])
    def test_idle_light_does_not_overwrite_external_change(self):
        device=hardware.Platform.__new__(hardware.Platform);device.idle_rgb=None
        with patch.object(hardware,'get_rgb',side_effect=[{'rgb':[255,0,0],'brightness':80},{'rgb':[0,255,0],'brightness':50}]),patch.object(hardware,'set_rgb') as setter:
            device.lighting_idle(True);device.lighting_idle(False);self.assertEqual(setter.call_count,1)
    def test_auto_available_with_bridge_without_ioctl(self):
        device=hardware.Platform.__new__(hardware.Platform);device.package_power=Mock();device.temperature_target=Mock(saved=None);device.fd=None;device.match=True;device.ec_error=''
        device.fans=Mock();device.fans.status.return_value={'available':True,'mode':'auto','verified':True}
        device.gpu=Mock();device.gpu.status.return_value={'available':False}
        with patch.object(hardware.cooling,'cpu_status',return_value=None),patch.object(hardware,'get_power_profile',return_value=None),patch.object(hardware,'get_boost',return_value=None),patch.object(hardware,'get_rgb',return_value=None):
            self.assertTrue(device.status()['capabilities']['auto_fan'])
    def test_invalid_desktop_does_not_execute(self):
        with patch.object(desktop,'run') as run:
            for value in ({'op':'mode','output':'1;reboot','mode':'2'},{'op':'wifi','enabled':'true'},{'op':'status','command':'sh'}):
                with self.assertRaises(ValueError):desktop.dispatch(value)
            run.assert_not_called()

class Rules(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.hardware=Mock();self.hardware.dispatch.return_value={'message':'applied','verified':True}
        self.controller=Automation(self.hardware,1000,Path(self.temp.name)/'policy.json')
        rules=default_automation();rules.update(ac='AC',battery='BAT',apps=[{'executable':'/usr/bin/game','profile':'GAME'}])
        self.controller.configure({'schema':3,'profiles':{name:{'power_profile':'balanced'} for name in ('AC','BAT','GAME')},'automation':rules})
        self.controller.session=True;self.controller.snapshot=Mock(return_value={'boost_enabled':False})
    def tick(self,t,source='ac',apps=()):self.controller.tick(now=t,source=source,running=set(apps))
    def test_debounce(self):
        self.tick(10);self.tick(12);self.hardware.dispatch.assert_not_called()
        self.tick(13);self.assertEqual(self.controller.active,('ac','AC'))
    def test_app_priority_and_power_return(self):
        self.tick(10);self.tick(13,apps=['/usr/bin/game'])
        self.assertEqual(self.controller.active,('app:/usr/bin/game','GAME'))
        self.tick(15);self.assertEqual(self.controller.active,('ac','AC'))
    def test_previous_snapshot_return_without_power_rule(self):
        self.controller.document['automation']['ac']=None
        self.tick(10,apps=['/usr/bin/game']);self.tick(14)
        self.assertEqual(self.hardware.dispatch.call_args.args[0]['settings'],{'boost_enabled':False})
    def gpu_snapshot(self,requested):
        self.controller.snapshot=Automation.snapshot.__get__(self.controller)
        self.hardware.status.return_value={
            'capabilities':{'power':False,'boost':False,'cpu_limit':False,'gpu_limit':True,'rgb':False,'manual_fan':False},
            'cooling':{'gpu':{'requested_max_mhz':requested}}}
    def test_game_exit_resets_previously_unlimited_gpu(self):
        self.controller.document['automation']['ac']=None
        self.controller.document['profiles']['GAME']={'gpu_max_mhz':1200}
        self.gpu_snapshot(None)
        gpu=Mock();gpu.set_max.return_value=None
        self.hardware.dispatch.side_effect=lambda command: (gpu.set_max(command['settings']['gpu_max_mhz']) or {'message':'applied'})
        self.tick(10,apps=['/usr/bin/game']);self.tick(14)
        self.assertEqual([call.args[0] for call in gpu.set_max.call_args_list],[1200,0])
    def test_edit_active_app_reapplies_and_preserves_original_gpu_limit(self):
        self.controller.document['automation']['ac']=None
        self.controller.document['profiles']['GAME']={'gpu_max_mhz':1200}
        self.gpu_snapshot(900)
        gpu=Mock();gpu.set_max.return_value=None
        self.hardware.dispatch.side_effect=lambda command: (gpu.set_max(command['settings']['gpu_max_mhz']) or {'message':'applied'})
        self.tick(10,apps=['/usr/bin/game'])
        document=json.loads(json.dumps(self.controller.document))
        document['profiles']['GAME']['gpu_max_mhz']=1800
        self.controller.configure(document)
        self.tick(12,apps=['/usr/bin/game']);self.tick(14,apps=['/usr/bin/game']);self.tick(16)
        self.assertEqual([call.args[0] for call in gpu.set_max.call_args_list],[1200,1800,900])
    def test_edit_active_power_profile_reapplies_once(self):
        self.tick(10);self.tick(13)
        document=json.loads(json.dumps(self.controller.document))
        document['profiles']['AC']['power_profile']='performance'
        self.controller.configure(document)
        self.tick(15);self.tick(17)
        self.assertEqual(self.hardware.dispatch.call_count,2)
        self.assertEqual(self.hardware.dispatch.call_args.args[0]['settings'],{'power_profile':'performance'})
    def test_rule_edit_preserves_app_snapshot_until_exit(self):
        self.controller.document['automation']['ac']=None
        self.tick(10,apps=['/usr/bin/game'])
        document=json.loads(json.dumps(self.controller.document))
        document['automation']['apps']=[]
        self.controller.configure(document)
        self.tick(14)
        self.controller.snapshot.assert_called_once()
        self.assertEqual(self.hardware.dispatch.call_args.args[0]['settings'],{'boost_enabled':False})
    def test_pause_prevents_application_until_resume(self):
        self.controller.pause();self.tick(10);self.tick(14)
        self.hardware.dispatch.assert_not_called()
        self.controller.resume();self.tick(16);self.assertEqual(self.controller.active,('ac','AC'))
    def test_defaults_never_write(self):
        self.controller.document['automation']=default_automation();self.tick(10);self.tick(15)
        self.hardware.dispatch.assert_not_called()
    def test_failure_pauses_and_does_not_repeat(self):
        self.hardware.dispatch.side_effect=RuntimeError('failure')
        self.tick(10);self.tick(14);self.tick(16)
        self.assertTrue(self.controller.paused);self.assertEqual(self.hardware.dispatch.call_count,1)
    def test_policy_root_store_roundtrip(self):
        other=Automation(self.hardware,1000,self.controller.path)
        self.assertEqual(other.document,self.controller.document)
        self.assertEqual(self.controller.path.stat().st_mode&0o777,0o600)
    def test_pause_survives_service_restart(self):
        self.controller.pause()
        other=Automation(self.hardware,1000,self.controller.path)
        self.assertTrue(other.paused)
        other.resume();self.assertFalse(other.pause_path.exists())
    def test_async_fan_failure_pauses_automation(self):
        self.controller.document['profiles']['AC']['fan']={'mode':'boost'}
        self.hardware.fans.error='RPM failure';self.hardware.fans.status.return_value={'measured':{'thermal':0}}
        self.tick(10);self.tick(14);self.tick(16)
        self.assertTrue(self.controller.paused);self.assertIn('RPM failure',self.controller.error)

class SessionRulesTests(unittest.TestCase):
    def test_low_hz_internal_only_and_restore(self):
        internal={'id':1,'name':'eDP-1','currentModeId':'fast','modes':[
            {'id':'fast','refreshRate':300,'size':{'width':2560,'height':1600}},
            {'id':'slow','refreshRate':60,'size':{'width':2560,'height':1600}},
            {'id':'wrong','refreshRate':30,'size':{'width':1920,'height':1080}}]}
        external=dict(internal,id=2,name='HDMI-1')
        rules=default_automation();rules['low_hz_on_battery']=True
        state={'automation':{'paused':False,'source':'battery','session_ready':True,'document':{'automation':rules},'reason':'','error':''}}
        controller=session.SessionRules();controller.ready=True
        with patch.object(session,'request',return_value=state),patch.object(desktop,'screens',return_value=[internal,external]),patch.object(desktop,'set_mode') as setter:
            controller.tick();setter.assert_called_once_with(1,'slow')
            state['automation']['source']='ac';controller.last_check=0;controller.tick()
            self.assertEqual(setter.call_args.args,(1,'fast'))
    def test_invalid_icc_does_not_apply(self):
        with patch.object(desktop,'screens',return_value=[{'id':1}]),patch.object(desktop,'run') as run:
            with self.assertRaises(ValueError):desktop.dispatch({'op':'icc','output':1,'path':'/tmp/not-an-icc.txt'})
            run.assert_not_called()

if __name__=='__main__':unittest.main()
