import json
import os
from pathlib import Path
import socket
import tempfile
import unittest
from unittest.mock import patch, Mock

from slayer_r9t import hardware
from slayer_r9t.protocol import receive, validate, REQUEST_LIMIT
from slayer_r9t.settings import Settings, atomic_json, validate_document
from slayer_r9t.service import Service


class ProtocolTests(unittest.TestCase):
    def test_valid_status(self):
        self.assertEqual(validate({'version': 1, 'id': 'abc', 'op': 'status'})['op'], 'status')

    def test_reject_bad_fields(self):
        for value in [[], {}, {'version': True, 'id': 'a', 'op': 'status'},
                      {'version': 1, 'id': 'a', 'op': 'status', 'path': '/tmp/a'},
                      {'version': 1, 'id': 'a', 'op': 'rgb'}]:
            with self.assertRaises((ValueError, AttributeError)):
                validate(value)

    def test_oversize(self):
        a, b = socket.socketpair()
        with a, b:
            a.sendall(b'x'*(REQUEST_LIMIT+1)+b'\n')
            with self.assertRaises(ValueError):
                receive(b, REQUEST_LIMIT)

    def test_non_object(self):
        a, b = socket.socketpair()
        with a, b:
            a.sendall(b'[]\n')
            with self.assertRaises(ValueError):
                receive(b, REQUEST_LIMIT)

    def test_multiple_frames(self):
        a, b = socket.socketpair()
        with a, b:
            a.sendall(b'{}\n{}\n')
            with self.assertRaises(ValueError):
                receive(b, REQUEST_LIMIT)


class SettingsTests(unittest.TestCase):
    def test_atomic_roundtrip_permissions(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)/'settings.json'
            store = Settings(path)
            store.profiles['Dengeli'] = {'power_profile': 'balanced', 'boost_enabled': False}
            store.save()
            self.assertEqual(Settings(path).profiles, store.profiles)
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)

    def test_migration_preserves_legacy(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)/'settings.json'
            original = {'profiles': {'Sessiz': {'cpu': 35, 'gpu': 35}}, 'curve': [[55, 30], [92, 100]]}
            path.write_text(json.dumps(original))
            store = Settings(path)
            store.save()
            self.assertEqual(Settings(path).data['legacy_fan_profiles'], original)
            self.assertEqual(store.profiles, {})

    def test_corrupt_settings_preserved(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)/'settings.json'
            path.write_text('{bad')
            store = Settings(path)
            self.assertEqual(store.profiles, {})
            self.assertEqual(len(list(Path(temp).glob('*.invalid-*'))), 1)

    def test_bad_import_preserves_current(self):
        with tempfile.TemporaryDirectory() as temp:
            store = Settings(Path(temp)/'settings.json')
            store.profiles['A'] = {'boost_enabled': False}
            store.save()
            imported = Path(temp)/'bad.json'
            atomic_json(imported, {'schema': 2, 'profiles': {'evil': {'path': '/tmp'}}})
            with self.assertRaises(ValueError):
                store.import_file(imported)
            self.assertEqual(Settings(store.path).profiles, {'A': {'boost_enabled': False}})

    def test_bad_profiles(self):
        for settings in [{'boost_enabled': 1}, {'rgb': [255, 0, 0]}, {'brightness': 50},
                         {'rgb': [False, 0, 0], 'brightness': 100}, {'power_profile': 'turbo'}, {}]:
            with self.assertRaises(ValueError):
                hardware.validate_profile(settings)


class HardwareTests(unittest.TestCase):
    def test_manual_disabled_without_write(self):
        device = hardware.Platform.__new__(hardware.Platform)
        device.temperature_target = Mock(saved=None)
        device.match = True
        device.fans = Mock()
        device.fans.status.return_value = {'available':False}
        for operation in ('manual', 'curve'):
            with patch.object(hardware.io, 'write_int') as write:
                with self.assertRaises(RuntimeError):
                    device.dispatch({'op': operation})
                write.assert_not_called()

    def test_power_only_profile_does_not_read_leds(self):
        with patch.object(hardware, 'get_power_profile', return_value='power-saver'), \
             patch.object(hardware, 'set_power_profile') as setter, \
             patch.object(hardware, 'leds', side_effect=AssertionError('unrelated RGB access')):
            hardware.apply_profile({'power_profile': 'balanced'})
            setter.assert_called_once_with('balanced')

    def test_profile_failure_restores_power(self):
        with patch.object(hardware, 'get_power_profile', return_value='power-saver'), \
             patch.object(hardware, 'set_power_profile') as setter, \
             patch.object(hardware, 'set_boost', side_effect=RuntimeError('boost failure')), \
             patch.object(hardware.Path, 'glob', return_value=[]):
            with self.assertRaisesRegex(RuntimeError, 'Önceki ayarlar geri yüklendi'):
                hardware.apply_profile({'power_profile': 'balanced', 'boost_enabled': True})
            self.assertEqual([call.args[0] for call in setter.call_args_list], ['balanced', 'power-saver'])

    def test_rgb_precision_and_readback(self):
        with tempfile.TemporaryDirectory() as temp:
            entry = Path(temp)
            for name, value in {'multi_index': 'red green blue', 'multi_intensity': '23 255 255',
                                'brightness': '50', 'max_brightness': '50'}.items():
                (entry/name).write_text(value)
            with patch.object(hardware, 'leds', return_value=[entry]):
                hardware.set_rgb([23, 255, 255], 51)
                self.assertEqual(hardware.get_rgb()['brightness'], 52)

    def test_rgb_failure_restores_original(self):
        with tempfile.TemporaryDirectory() as temp:
            entry = Path(temp)
            for name, value in {'multi_index': 'red green blue', 'multi_intensity': '23 255 255',
                                'brightness': '50', 'max_brightness': '50'}.items():
                (entry/name).write_text(value)
            with patch.object(hardware, 'leds', return_value=[entry]), \
                 patch.object(hardware, 'get_rgb', return_value={'rgb': [0, 0, 0], 'brightness': 0}):
                with self.assertRaises(RuntimeError):
                    hardware.set_rgb([255, 0, 0], 50)
            self.assertEqual((entry/'multi_intensity').read_text(), '23 255 255')
            self.assertEqual((entry/'brightness').read_text(), '50')


if __name__ == '__main__':
    unittest.main()
