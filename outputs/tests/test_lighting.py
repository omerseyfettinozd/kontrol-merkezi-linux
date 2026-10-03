import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch,Mock
from slayer_r9t import hardware,features
from slayer_r9t.lighting import pattern,validate_map

class LightingTests(unittest.TestCase):
    def test_patterns_and_profile_validation(self):
        for name in ('zones','rainbow','gradient'):
            colors=pattern(name,[255,129,3])
            self.assertEqual(len(validate_map(colors)),126)
            hardware.validate_profile({'rgb_map':colors,'brightness':80})
        for colors in ([],[[0,0,0]]*125,[[True,0,0]]*126):
            with self.assertRaises(ValueError):validate_map(colors)
        with self.assertRaises(ValueError):hardware.validate_profile({'rgb_map':pattern('zones',[0,0,0]),'rgb':[0,0,0],'brightness':80})

    def test_map_writes_and_readback(self):
        with tempfile.TemporaryDirectory() as temp:
            entries=[]
            for i in range(126):
                p=Path(temp)/str(i);p.mkdir();entries.append(p)
                for field,value in [('multi_index','red green blue'),('multi_intensity','255 129 3'),('brightness','50'),('max_brightness','50')]:
                    (p/field).write_text(value)
            with patch.object(hardware,'leds',return_value=entries),patch.object(hardware,'perkey_available',return_value=True),patch.object(hardware,'rgb_range_supported',return_value=True):
                colors=pattern('rainbow',[255,0,0]);hardware.set_rgb_map(colors,80)
                self.assertEqual(hardware.get_rgb()['rgb_map'],colors)
                self.assertEqual(hardware.get_rgb()['brightness'],80)
                device=hardware.Platform.__new__(hardware.Platform);device.idle_rgb=None
                device.lighting_idle(True)
                self.assertEqual(hardware.get_rgb()['brightness'],0)
                device.lighting_idle(False)
                self.assertEqual(hardware.get_rgb()['brightness'],80)
                self.assertEqual(hardware.get_rgb()['rgb_map'],colors)


    def test_automation_snapshot_preserves_map(self):
        from slayer_r9t.automation import Automation
        automation=Automation.__new__(Automation);automation.hardware=Mock()
        colors=pattern('zones',[255,129,3])
        automation.hardware.status.return_value={'capabilities':{'power':False,'boost':False,'cpu_limit':False,'gpu_limit':False,'rgb':True,'manual_fan':False},'rgb_state':{'rgb':None,'rgb_map':colors,'brightness':80}}
        saved=automation.snapshot()
        self.assertNotIn('rgb',saved)
        self.assertEqual(saved['rgb_map'],colors)
        hardware.validate_profile(saved)

    def test_profile_map_file_roundtrip(self):
        from slayer_r9t.settings import Settings
        with tempfile.TemporaryDirectory() as temp:
            path=Path(temp)/'settings.json';store=Settings(path)
            colors=pattern('rainbow',[255,0,0])
            store.profiles['Renkler']={'rgb_map':colors,'brightness':80};store.save()
            self.assertEqual(Settings(path).profiles['Renkler']['rgb_map'],colors)

    def test_unsupported_preflight_does_not_change_power(self):
        with patch.object(hardware,'perkey_available',return_value=False),patch.object(hardware,'set_power_profile') as power:
            with self.assertRaises(RuntimeError):hardware.apply_profile({'power_profile':'balanced','rgb_map':pattern('zones',[0,0,0]),'brightness':100})
            power.assert_not_called()

    def test_invalid_map_never_writes(self):
        with patch.object(hardware,'leds',side_effect=AssertionError('No hardware access')):
            with self.assertRaises(ValueError):hardware.set_rgb_map([],100)

    def test_camera_mismatch_never_writes(self):
        with tempfile.TemporaryDirectory() as temp:
            device=Path(temp)
            for field,value in [('idVendor','ffff'),('idProduct','c906'),('product','FHD WebCam'),('authorized','1')]:
                (device/field).write_text(value)
            with patch.object(features,'CAMERA',device):
                self.assertFalse(features.camera_status()['available'])
                with self.assertRaises(RuntimeError):features.camera(False)
                self.assertEqual((device/'authorized').read_text(),'1')

    def test_camera_same_state_and_invalid_input(self):
        with tempfile.TemporaryDirectory() as temp:
            device=Path(temp)
            for field,value in [('idVendor','2b7e'),('idProduct','c906'),('product','FHD WebCam'),('authorized','1')]:
                (device/field).write_text(value)
            with patch.object(features,'CAMERA',device):
                self.assertTrue(features.camera_status()['available'])
                self.assertTrue(features.camera(True)['verified'])
                with self.assertRaises(ValueError):features.camera(1)
