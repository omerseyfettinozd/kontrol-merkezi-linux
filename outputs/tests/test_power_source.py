"""Power policy classification using temporary sysfs-shaped directories."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from slayer_r9t.automation import power_source


class PowerSourceTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)

    def supply(self, name, kind, online=None):
        entry = self.root / name
        entry.mkdir()
        (entry / 'type').write_text(kind)
        if online is not None:
            (entry / 'online').write_text(online)
        return entry

    def source(self, entries=None):
        with patch('slayer_r9t.automation.Path', return_value=self.root):
            if entries is None:
                return power_source()
            with patch.object(Path, 'iterdir', return_value=iter(entries)):
                return power_source()

    def test_online_source_wins_regardless_of_other_offline_sources_or_order(self):
        offline = self.supply('AC0', 'Mains', '0')
        for kind in ('Mains', 'USB'):
            online = self.supply(kind, kind, '1')
            for entries in ([offline, online], [online, offline]):
                with self.subTest(kind=kind, order=[p.name for p in entries]):
                    self.assertEqual(self.source(entries), 'ac')

    def test_all_chargers_offline_select_battery(self):
        self.supply('AC0', 'Mains', '0')
        self.supply('USB0', 'USB', '0')
        self.supply('BAT0', 'Battery')
        self.assertEqual(self.source(), 'battery')

    def test_missing_charger_or_empty_directory_is_unknown(self):
        self.assertIsNone(self.source())
        self.supply('BAT0', 'Battery')
        self.assertIsNone(self.source())

    def test_unreadable_charger_never_implies_battery(self):
        offline = self.supply('AC0', 'Mains', '0')
        unreadable = self.supply('USB0', 'USB')
        self.assertIsNone(self.source([offline, unreadable]))
        online = self.supply('AC1', 'Mains', '1')
        self.assertEqual(self.source([unreadable, online]), 'ac')

    def test_invalid_online_value_is_unknown(self):
        self.supply('AC0', 'Mains', '0')
        broken = self.supply('USB0', 'USB', 'unexpected')
        self.assertIsNone(self.source())
        (broken / 'online').write_bytes(b'\xff')
        self.assertIsNone(self.source())

    def test_missing_root_is_unknown_without_raising(self):
        with patch('slayer_r9t.automation.Path', return_value=self.root / 'missing'):
            self.assertIsNone(power_source())
