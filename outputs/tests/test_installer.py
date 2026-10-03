"""Deployment rollback checks with temporary files and a fake systemd runner."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location('r9t_installer', Path(__file__).parents[1] / 'install-r9t.py')
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.source = self.base / 'source'
        self.source.mkdir()
        for name in ('r9t-service.py', 'r9t-client.py', 'r9t-control.py', 'r9t-control-center.py',
                     'r9t-session.py', 'r9t-desktop-client.py', 'r9t-gpu-diagnostics.py', 'r9t-dynamic-boost.py'):
            (self.source / name).write_text('# new version\n')
        (self.source / 'slayer_r9t').mkdir()
        (self.source / 'slayer_r9t/__init__.py').write_text("__version__ = 'test'\n")
        self.target = self.base / 'lib/slayer-r9t'
        self.unit = self.base / 'system.service'
        self.desktop = self.base / 'app.desktop'
        self.session_unit = self.base / 'session.service'
        self.user = SimpleNamespace(pw_dir=str(self.base / 'home'), pw_name='test-user')
        self.old_launcher = Path(self.user.pw_dir) / '.local/share/applications/slayer-r9t-control-center.desktop'
        self.old_launcher.parent.mkdir(parents=True)
        self.states = {'root': {'active': False, 'enabled': 'disabled'},
                       'session': {'active': False, 'enabled': 'disabled'}}
        self.calls = []
        self.failure = None
        self.failure_used = False
        self.addCleanup(patch.stopall)
        patch.object(installer.subprocess, 'run', side_effect=self.run_command).start()
        patch.object(installer.subprocess, 'check_output', return_value='').start()
        patch.object(installer.os, 'chown').start()
        patch.object(installer.time, 'sleep').start()
        patch('builtins.print').start()

    def run_command(self, command, **kwargs):
        self.calls.append(command)
        if command[0] == 'c++':
            Path(command[-1]).write_text('fake executable')
        if '/usr/bin/systemctl' not in command:
            return subprocess.CompletedProcess(command, 0, '', '')
        manager = 'session' if '--user' in command else 'root'
        args = command[command.index('--user') + 1:] if manager == 'session' else command[1:]
        operation = args[0]
        state = self.states[manager]
        if self.failure == (manager, operation) and not self.failure_used:
            self.failure_used = True
            raise subprocess.CalledProcessError(1, command)
        if operation == 'is-active' and '--quiet' not in args:
            return subprocess.CompletedProcess(command, 0 if state['active'] else 3,
                                               'active\n' if state['active'] else 'inactive\n', '')
        if operation == 'is-enabled':
            return subprocess.CompletedProcess(command, 0 if state['enabled'].startswith('enabled') else 1,
                                               state['enabled'] + '\n', '')
        if operation == 'enable':
            state['enabled'] = 'enabled-runtime' if '--runtime' in args else 'enabled'
        elif operation == 'disable':
            state['enabled'] = 'disabled'
        elif operation in ('start', 'restart'):
            state['active'] = True
        elif operation == 'stop':
            state['active'] = False
        return subprocess.CompletedProcess(command, 0, '', '')

    def deploy(self):
        installer.install_application(self.source, self.target, 1234, self.user,
                                      self.unit, self.desktop, self.session_unit)

    def existing_installation(self, active=False, enabled='disabled'):
        self.target.mkdir(parents=True)
        (self.target / 'previous.txt').write_text('old application')
        for path in (self.unit, self.desktop, self.session_unit, self.old_launcher):
            path.write_text('previous ' + path.name)
            path.chmod(0o640)
        self.states = {manager: {'active': active, 'enabled': enabled} for manager in ('root', 'session')}
        return {path: path.read_bytes() for path in (self.unit, self.desktop, self.session_unit, self.old_launcher)}

    def assert_restored(self, originals, active=False, enabled='disabled'):
        self.assertEqual((self.target / 'previous.txt').read_text(), 'old application')
        self.assertFalse((self.target / 'r9t-service.py').exists())
        for path, original in originals.items():
            self.assertEqual(path.read_bytes(), original)
            self.assertEqual(path.stat().st_mode & 0o777, 0o640)
        for state in self.states.values():
            self.assertEqual(state, {'active': active, 'enabled': enabled})
        self.assertFalse(list(self.target.parent.glob('.slayer-r9t-stage-*')))

    def test_late_activation_failures_restore_disabled_inactive_installation(self):
        for step in ('daemon-reload', 'enable', 'restart'):
            with self.subTest(step=step):
                originals = self.existing_installation()
                self.failure = ('session', step)
                self.failure_used = False
                with self.assertRaises(subprocess.CalledProcessError):
                    self.deploy()
                self.assert_restored(originals)
                # Reset the fixture directory for the next late failure.
                import shutil
                shutil.rmtree(self.target)

    def test_restart_failure_preserves_active_enabled_runtime_services(self):
        originals = self.existing_installation(active=True, enabled='enabled-runtime')
        self.failure = ('session', 'restart')
        with self.assertRaises(subprocess.CalledProcessError):
            self.deploy()
        self.assert_restored(originals, active=True, enabled='enabled-runtime')

    def test_first_install_failure_removes_all_new_files_and_enable_state(self):
        self.failure = ('session', 'restart')
        with self.assertRaises(subprocess.CalledProcessError):
            self.deploy()
        for path in (self.target, self.unit, self.desktop, self.session_unit):
            self.assertFalse(path.exists())
        for state in self.states.values():
            self.assertEqual(state, {'active': False, 'enabled': 'disabled'})

    def test_desktop_write_failure_restores_root_installation(self):
        originals = self.existing_installation()
        write = installer.write_managed_file
        def fail_desktop(path, text):
            if path == self.desktop:
                raise OSError('desktop write failed')
            write(path, text)
        with patch.object(installer, 'write_managed_file', side_effect=fail_desktop):
            with self.assertRaisesRegex(OSError, 'desktop write failed'):
                self.deploy()
        self.assert_restored(originals)

    def test_user_unit_write_failure_restores_removed_user_launcher(self):
        originals = self.existing_installation(active=True, enabled='enabled')
        write = installer.write_managed_file
        def fail_user_unit(path, text):
            if path == self.session_unit:
                raise OSError('session unit write failed')
            write(path, text)
        with patch.object(installer, 'write_managed_file', side_effect=fail_user_unit):
            with self.assertRaisesRegex(OSError, 'session unit write failed'):
                self.deploy()
        self.assert_restored(originals, active=True, enabled='enabled')

    def test_unsupported_linked_manager_state_aborts_before_mutation(self):
        originals = self.existing_installation(active=True, enabled='linked')
        with self.assertRaisesRegex(RuntimeError, 'Bağlantılı/dolaylı'):
            self.deploy()
        self.assert_restored(originals, active=True, enabled='linked')
        self.assertFalse(any('stop' in command for command in self.calls))

    def test_symlink_targets_unchanged_and_links_restored_after_failure(self):
        self.existing_installation(active=True, enabled='enabled')
        linked = {}
        for path in (self.unit, self.desktop, self.session_unit, self.old_launcher):
            destination = self.base / (path.name + '.external')
            path.rename(destination)
            path.symlink_to(destination)
            linked[path] = (destination, destination.read_bytes())
        self.failure = ('session', 'restart')
        with self.assertRaises(subprocess.CalledProcessError):
            self.deploy()
        for path, (destination, original) in linked.items():
            self.assertTrue(path.is_symlink())
            self.assertEqual(path.resolve(), destination)
            self.assertEqual(destination.read_bytes(), original)

    def test_invalid_snapshot_does_not_create_staging_directory(self):
        self.desktop.mkdir()
        with self.assertRaisesRegex(RuntimeError, 'normal dosya'):
            self.deploy()
        self.assertFalse(list(self.target.parent.glob('.slayer-r9t-stage-*')))
        self.assertEqual(self.calls, [])

    def test_disconnected_user_manager_aborts_before_service_mutation(self):
        originals = self.existing_installation(active=True, enabled='enabled')
        fake = self.run_command
        def disconnected(command, **kwargs):
            if '--user' in command and 'is-active' in command:
                return subprocess.CompletedProcess(command, 1, '', 'Failed to connect to bus')
            return fake(command, **kwargs)
        with patch.object(installer.subprocess, 'run', side_effect=disconnected):
            with self.assertRaisesRegex(RuntimeError, 'servis durumu okunamadı'):
                self.deploy()
        self.assert_restored(originals, active=True, enabled='enabled')
        self.assertFalse(any('stop' in command for command in self.calls))

    def test_rollback_failure_is_reported_and_other_files_still_restored(self):
        originals = self.existing_installation()
        self.failure = ('session', 'restart')
        restore = installer.restore_file
        def fail_one(path, snapshot):
            if path == self.desktop:
                raise OSError('restore denied')
            restore(path, snapshot)
        with patch.object(installer, 'restore_file', side_effect=fail_one):
            with self.assertRaisesRegex(RuntimeError, 'geri alma tamamlanamadı') as result:
                self.deploy()
        self.assertIsInstance(result.exception.__cause__, subprocess.CalledProcessError)
        self.assertEqual(self.unit.read_bytes(), originals[self.unit])
        self.assertEqual(self.session_unit.read_bytes(), originals[self.session_unit])
        self.assertEqual(self.old_launcher.read_bytes(), originals[self.old_launcher])
        self.assertEqual((self.target / 'previous.txt').read_text(), 'old application')

    def test_success_keeps_previous_application_backup(self):
        self.existing_installation(active=True, enabled='enabled')
        self.deploy()
        self.assertTrue((self.target / 'r9t-service.py').exists())
        self.assertEqual((self.target.with_name('.slayer-r9t-previous') / 'previous.txt').read_text(), 'old application')
        self.assertFalse(self.old_launcher.exists())
        self.assertTrue(all(state == {'active': True, 'enabled': 'enabled'} for state in self.states.values()))


if __name__ == '__main__':
    unittest.main()
