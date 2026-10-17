"""Removal inventory cannot execute commands or mutate installed files."""
import os
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import patch

from slayer_r9t.uninstall_plan import APP, MAX_FILE, PROGRAMS, UNITS, _inspect, inventory


class UninstallPlanTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.root = self.base / 'root'
        self.source = self.base / 'source'
        self.root.mkdir()
        self.source.mkdir()
        (self.source / 'slayer_r9t').mkdir()
        (self.source / 'slayer_r9t/__init__.py').write_bytes(b'version = "test"\n')
        for name in PROGRAMS:
            (self.source / name).write_bytes(b'# application\n')

    def install(self):
        app = self.root / APP.lstrip('/')
        (app / 'slayer_r9t').mkdir(parents=True)
        for name in PROGRAMS:
            (app / name).write_bytes((self.source / name).read_bytes())
        (app / 'slayer_r9t/__init__.py').write_bytes(b'version = "test"\n')
        (app / 'r9t-idle').write_bytes(b'binary without trusted hash')
        for logical, marker in UNITS.items():
            path = self.root / logical.lstrip('/')
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(marker + b'\n')
        return app

    def plan(self):
        return inventory(self.source, root=self.root, owner_uid=os.getuid())

    def test_matching_content_stays_uncertain_without_ownership_manifest(self):
        self.install()
        plan = self.plan()
        program = next(i for i in plan['items'] if i['path'].endswith('/r9t-service.py'))
        self.assertTrue(program['content_matches_reference'])
        self.assertEqual(program['status'], 'uncertain')
        self.assertFalse(any(i['removable'] for i in plan['items']))
        self.assertFalse(plan['executor_available'])
        self.assertIn('/var/lib/slayer-r9t', plan['preserve'])
        self.assertIn('/usr/src/slayer-r9t-fan-0.1.0', plan['preserve'])

    def test_modified_unknown_symlink_and_special_entries_are_blocked(self):
        app = self.install()
        (app / 'r9t-service.py').write_bytes(b'# modified\n')
        (app / 'unknown.txt').write_bytes(b'personal')
        (app / 'r9t-client.py').unlink()
        outside = self.base / 'outside'
        outside.write_bytes(b'never read this')
        (app / 'r9t-client.py').symlink_to(outside)
        (app / 'r9t-control.py').unlink()
        os.mkfifo(app / 'r9t-control.py')
        result = {i['path']: i for i in self.plan()['items']}
        for name in ('r9t-service.py', 'unknown.txt', 'r9t-client.py', 'r9t-control.py'):
            self.assertEqual(result[APP + '/' + name]['status'], 'blocked')
        self.assertNotIn('sha256', result[APP + '/r9t-client.py'])
        self.assertNotIn('sha256', result[APP + '/r9t-control.py'])

    def test_symlink_parent_never_traversed(self):
        (self.root / 'usr').symlink_to(self.source)
        plan = self.plan()
        self.assertEqual(plan['items'][0]['status'], 'blocked')
        self.assertNotIn('sha256', plan['items'][0])

    def test_oversized_regular_file_is_blocked_before_reading(self):
        app = self.install()
        target = app / 'r9t-service.py'
        with target.open('wb') as handle:
            handle.truncate(MAX_FILE + 1)
        with patch('os.fdopen', side_effect=AssertionError('Must not read oversized file')):
            item, _ = _inspect(self.root, APP + '/r9t-service.py', os.getuid())
        self.assertEqual(item['status'], 'blocked')
        self.assertNotIn('sha256', item)

    def test_wrong_owner_and_missing_installation(self):
        self.assertEqual(self.plan()['items'][0]['status'], 'absent')
        self.install()
        plan = inventory(self.source, self.root, owner_uid=os.getuid() + 1)
        self.assertTrue(all(i['status'] == 'blocked' for i in plan['items']))

    def test_inventory_performs_no_writes_or_subprocesses(self):
        self.install()
        real_open = os.open
        def read_only_open(path, flags, *args, **kwargs):
            self.assertFalse(flags & (os.O_WRONLY | os.O_RDWR | os.O_CREAT | os.O_TRUNC))
            return real_open(path, flags, *args, **kwargs)
        with patch('os.open', side_effect=read_only_open), \
             patch('subprocess.run', side_effect=AssertionError('No commands allowed')), \
             patch.object(Path, 'write_bytes', side_effect=AssertionError('No writes')), \
             patch.object(Path, 'write_text', side_effect=AssertionError('No writes')), \
             patch('os.unlink', side_effect=AssertionError('No removal')), \
             patch('os.replace', side_effect=AssertionError('No replacement')):
            self.assertEqual(self.plan()['mode'], 'dry-run')

    def test_cli_requires_dry_run_and_prints_json_without_executor(self):
        import contextlib
        import io
        import json
        script = Path(__file__).parents[1] / 'r9t-uninstall-plan.py'
        entry = runpy.run_path(str(script))['main']
        with contextlib.redirect_stderr(io.StringIO()), \
             patch('slayer_r9t.uninstall_plan.inventory') as inspect:
            # The script holds its imported function; replace that global directly.
            with patch.dict(entry.__globals__, inventory=inspect):
                with self.assertRaises(SystemExit):
                    entry([])
                inspect.assert_not_called()
                inspect.return_value = {'mode': 'dry-run', 'executor_available': False}
                output = io.StringIO()
                with contextlib.redirect_stdout(output):
                    entry(['--dry-run'])
                self.assertFalse(json.loads(output.getvalue())['executor_available'])
                inspect.assert_called_once()
