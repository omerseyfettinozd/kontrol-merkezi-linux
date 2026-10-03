"""Test read-only CLI diagnostic commands and helpers."""
import json
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

class CliDiagnosticsTest(unittest.TestCase):
    def test_gpu_diagnostics_cli_runs(self):
        worker = ROOT / 'r9t-gpu-diagnostics.py'
        res = subprocess.run([sys.executable, str(worker)], capture_output=True, text=True, timeout=10)
        self.assertEqual(res.returncode, 0)
        data = json.loads(res.stdout)
        self.assertIn('cards', data)
        self.assertIn('method', data)

    def test_dynamic_boost_cli_runs(self):
        worker = ROOT / 'r9t-dynamic-boost.py'
        res = subprocess.run([sys.executable, str(worker)], capture_output=True, text=True, timeout=10)
        self.assertEqual(res.returncode, 0)
        data = json.loads(res.stdout)
        self.assertIn('devices', data)
        self.assertIn('daemon', data)
        self.assertIn('behavior_verified', data)
