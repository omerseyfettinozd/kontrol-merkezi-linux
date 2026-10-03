"""Exercise actual fan build shell logic with isolated kernel and tool fixtures."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class FanBuildTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.build = self.root / 'modules/test-kernel/build'
        self.build.mkdir(parents=True)
        (self.build / 'Makefile').write_text('# fake kernel Makefile\n')
        self.symbols = self.root / 'dkms/4.24.0/test-kernel/x86_64/module/Module.symvers'
        self.symbols.parent.mkdir(parents=True)
        self.symbols.write_text('# fake symbols\n')
        self.bins = self.root / 'bin'
        self.bins.mkdir()
        for executable in ('find', 'sort', 'tail', 'grep'):
            location = shutil.which(executable)
            if location is None:
                self.skipTest(f'{executable} unavailable')
            (self.bins / executable).symlink_to(location)
        make = self.bins / 'make'
        make.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > "$MAKE_ARGUMENTS"\n')
        make.chmod(0o755)
        source = Path(__file__).parents[1] / 'fan-driver/build.sh'
        # Substitute only absolute fixture roots; execute the production shell logic.
        script = source.read_text().replace('/lib/modules', str(self.root / 'modules'))
        script = script.replace('/var/lib/dkms/tuxedo-drivers', str(self.root / 'dkms'))
        self.script = self.root / 'build.sh'
        self.script.write_text(script)
        self.args_path = self.root / 'make-args'

    def run_build(self):
        return subprocess.run(['/bin/sh', str(self.script), 'test-kernel', 'x86_64'],
                              cwd=self.root, capture_output=True, text=True, timeout=5,
                              env=dict(os.environ, PATH=str(self.bins), MAKE_ARGUMENTS=str(self.args_path)))

    def assert_compiler(self, clang, auto_conf=False):
        config = self.build / ('include/config/auto.conf' if auto_conf else '.config')
        config.parent.mkdir(parents=True, exist_ok=True)
        config.write_text('CONFIG_CC_IS_CLANG=y\n' if clang else 'CONFIG_CC_IS_GCC=y\n')
        result = self.run_build()
        self.assertEqual(result.returncode, 0, result.stderr)
        args = self.args_path.read_text().splitlines()
        self.assertEqual('LLVM=1' in args, clang)
        self.assertEqual(args[:2], ['-C', str(self.build)])
        self.assertIn('KBUILD_EXTRA_SYMBOLS=' + str(self.symbols), args)
        self.assertEqual(args[-1], 'modules')
        self.assertFalse((self.bins / 'rg').exists())

    def test_clang_build_uses_llvm_without_ripgrep(self):
        self.assert_compiler(True)

    def test_gcc_build_keeps_default_toolchain(self):
        self.assert_compiler(False)

    def test_header_auto_conf_fallback_keeps_toolchain(self):
        self.assert_compiler(True, auto_conf=True)

    def test_missing_symbols_aborts_before_make(self):
        self.symbols.unlink()
        result = self.run_build()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('sembolleri yok', result.stderr)
        self.assertFalse(self.args_path.exists())

    def test_missing_build_or_config_aborts_before_make(self):
        for missing in ('config', 'Makefile'):
            with self.subTest(missing=missing):
                if missing == 'Makefile':
                    (self.build / 'Makefile').unlink()
                result = self.run_build()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('Kernel', result.stderr)
                self.assertFalse(self.args_path.exists())

    def test_config_query_failure_aborts_before_make(self):
        (self.build / '.config').write_text('CONFIG_CC_IS_CLANG=y\n')
        (self.bins / 'grep').unlink()
        grep = self.bins / 'grep'
        grep.write_text('#!/bin/sh\nexit 2\n')
        grep.chmod(0o755)
        result = self.run_build()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('sorgulanamadı', result.stderr)
        self.assertFalse(self.args_path.exists())
