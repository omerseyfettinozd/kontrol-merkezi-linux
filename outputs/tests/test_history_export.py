"""Telemetry CSV saves replace the destination only after a complete write."""
import csv
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from slayer_r9t.history import History


class HistoryExportTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.target = self.root / 'measurements.csv'
        self.target.write_bytes(b'previous measurements\n')
        self.history = History()
        self.history.add({'cpu_temp': 50, 'gpu_temp': None}, now=1, utc='2026-10-04T00:00:00Z')

    def assert_preserved(self):
        self.assertEqual(self.target.read_bytes(), b'previous measurements\n')
        self.assertEqual(list(self.root.iterdir()), [self.target])

    def test_failed_row_write_preserves_existing_csv_and_cleans_temp(self):
        with patch('slayer_r9t.history.csv.DictWriter.writerows', side_effect=OSError('disk full')):
            with self.assertRaisesRegex(OSError, 'disk full'):
                self.history.export(self.target, 300, now=2)
        self.assert_preserved()

    def test_failed_replace_preserves_existing_csv_and_cleans_temp(self):
        with patch('slayer_r9t.history.os.replace', side_effect=PermissionError('replace denied')):
            with self.assertRaisesRegex(PermissionError, 'replace denied'):
                self.history.export(self.target, 300, now=2)
        self.assert_preserved()

    def test_successful_export_keeps_data_format_and_no_temp_files(self):
        self.assertEqual(self.history.export(self.target, 300, now=2), 1)
        with self.target.open(newline='', encoding='utf-8') as stream:
            rows = list(csv.DictReader(stream))
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]['cpu_temp'], '50.0')
        self.assertEqual(rows[0]['gpu_temp'], '')
        self.assertEqual(rows[0]['utc'], '2026-10-04T00:00:00Z')
        self.assertEqual(list(self.root.iterdir()), [self.target])
