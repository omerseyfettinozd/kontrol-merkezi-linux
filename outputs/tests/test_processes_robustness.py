"""Robustness tests for processes panel data collector."""
import subprocess
import time
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from slayer_r9t.processes import ProcessSampler, format_bytes

class ProcessSamplerRobustnessTest(unittest.TestCase):
    def test_format_bytes(self):
        self.assertEqual(format_bytes(0), '0 B')
        self.assertEqual(format_bytes(512), '512 B')
        self.assertEqual(format_bytes(1024), '1.0 KiB')
        self.assertEqual(format_bytes(1024 * 1024 * 5), '5.0 MiB')
        self.assertEqual(format_bytes(1024 * 1024 * 1024 * 3), '3.0 GiB')

    def test_empty_proc_handling(self):
        with TemporaryDirectory() as tmpdir:
            sampler = ProcessSampler(proc=Path(tmpdir))
            sample = sampler.sample()
            self.assertEqual(sample['cpu'], [])
            self.assertEqual(sample['memory'], [])
            self.assertEqual(sample['scanned'], 0)
            self.assertEqual(sample['unreadable'], 0)
            self.assertIsNone(sample['error'])

    def test_corrupt_stat_files(self):
        with TemporaryDirectory() as tmpdir:
            proc = Path(tmpdir)
            pid_dir = proc / '1234'
            pid_dir.mkdir()
            (pid_dir / 'comm').write_text('badproc\n')
            (pid_dir / 'stat').write_text('garbage content\n')
            (pid_dir / 'statm').write_text('not numbers\n')

            sampler = ProcessSampler(proc=proc)
            sample = sampler.sample()
            self.assertEqual(len(sample['cpu']), 0)
            self.assertEqual(len(sample['memory']), 0)
            self.assertGreaterEqual(sample['unreadable'], 1)
