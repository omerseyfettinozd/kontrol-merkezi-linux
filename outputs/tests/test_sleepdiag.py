import json
import tempfile
import unittest
from pathlib import Path

from slayer_r9t import sleepdiag


def build(root, energy=50_000_000, ac=0, status='Discharging', success=3, fail=0, power=8_000_000):
    ps, pw = Path(root) / 'ps', Path(root) / 'power'
    (ps / 'BAT0').mkdir(parents=True, exist_ok=True)
    (ps / 'AC').mkdir(exist_ok=True)
    (pw / 'suspend_stats').mkdir(parents=True, exist_ok=True)
    (ps / 'BAT0/type').write_text('Battery\n')
    (ps / 'BAT0/energy_now').write_text(f'{energy}\n')
    (ps / 'BAT0/power_now').write_text(f'{power}\n')
    (ps / 'BAT0/status').write_text(status + '\n')
    (ps / 'AC/type').write_text('Mains\n')
    (ps / 'AC/online').write_text(f'{ac}\n')
    (pw / 'suspend_stats/success').write_text(f'{success}\n')
    (pw / 'suspend_stats/fail').write_text(f'{fail}\n')
    (pw / 'mem_sleep').write_text('s2idle [deep]\n')
    return ps, pw


class SleepDiagTest(unittest.TestCase):
    def snap(self, now, **kw):
        with tempfile.TemporaryDirectory() as d:
            ps, pw = build(d, **kw)
            return sleepdiag.take_snapshot(ps, pw, now=now)

    def test_snapshot_fields(self):
        s = self.snap(100.0)
        self.assertEqual(s['energy_uwh'], 50_000_000)
        self.assertEqual(s['stats']['success'], 3)
        self.assertEqual(s['stats']['mem_sleep'], 's2idle [deep]')
        self.assertFalse(s['ac_online'])

    def test_compare_valid(self):
        a = self.snap(0, energy=50_000_000)
        b = self.snap(7200, energy=49_000_000, success=4, fail=1)
        r = sleepdiag.compare(a, b)
        self.assertTrue(r['valid'])
        self.assertEqual(r['loss_uwh_per_h'], 500_000)
        self.assertEqual(r['suspend_delta'], 1)
        self.assertEqual(r['fail_delta'], 1)
        self.assertIn(sleepdiag.WARNING, r['warnings'])

    def test_ac_invalid(self):
        r = sleepdiag.compare(self.snap(0), self.snap(10, ac=1, status='Charging'))
        self.assertFalse(r['valid'])

    def test_clock_backwards(self):
        r = sleepdiag.compare(self.snap(10), self.snap(5))
        self.assertFalse(r['valid'])
        self.assertIsNone(r['loss_uwh_per_h'])

    def test_missing_sensors(self):
        with tempfile.TemporaryDirectory() as d:
            s = sleepdiag.take_snapshot(Path(d) / 'no', Path(d) / 'no2', now=1)
        self.assertIsNone(s['energy_uwh'])
        self.assertIsNone(s['stats']['success'])
        r = sleepdiag.compare(s, dict(s, time=100))
        self.assertIsNone(r['energy_delta_uwh'])
        self.assertIsNone(r['suspend_delta'])
        self.assertFalse(r['valid'])

    def test_charge_voltage_fallback(self):
        with tempfile.TemporaryDirectory() as d:
            ps, pw = build(d)
            (ps / 'BAT0/energy_now').unlink()
            (ps / 'BAT0/charge_now').write_text('4000000\n')
            (ps / 'BAT0/voltage_now').write_text('12000000\n')
            self.assertEqual(sleepdiag.read_battery(ps)['energy_uwh'], 48_000_000)

    def test_save_load_atomic(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / 'sub/s.json'
            sleepdiag.save_snapshot({'time': 1.0}, p)
            self.assertEqual(sleepdiag.load_snapshot(p), {'time': 1.0})
            self.assertEqual([f.name for f in p.parent.iterdir()], ['s.json'])
            p.write_text('garbage')
            self.assertIsNone(sleepdiag.load_snapshot(p))

    def test_malformed_snapshot_fields_are_rejected(self):
        valid = self.snap(100)
        changes = [
            {'time': True}, {'time': 10 ** 400}, {'time': float('nan')}, {'time': float('inf')},
            {'time': float('-inf')}, {'energy_uwh': 'bad'}, {'energy_uwh': -1},
            {'energy_uwh': True}, {'energy_uwh': 10 ** 400}, {'power_uw': []}, {'ac_online': 'false'},
            {'stats': []}, {'stats': {'success': 'bad'}},
            {'stats': {'fail': -1}}, {'status': {}},
        ]
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / 'snapshot.json'
            for change in changes:
                with self.subTest(change=change):
                    bad = dict(valid, **change)
                    path.write_text(json.dumps(bad))
                    self.assertIsNone(sleepdiag.load_snapshot(path))
                    result = sleepdiag.compare(bad, self.snap(200))
                    self.assertFalse(result['valid'])
                    self.assertIsNone(result['loss_uwh_per_h'])

    def test_unverified_sleep_or_missing_measurement_is_invalid(self):
        before, after = self.snap(0), self.snap(3600, success=4)
        changes = [
            {'ac_online': None}, {'energy_uwh': None}, {'stats': {}},
            {'stats': {'success': 3}}, {'stats': {'success': 2}},
        ]
        for change in changes:
            with self.subTest(change=change):
                self.assertFalse(sleepdiag.compare(before, dict(after, **change))['valid'])

    def test_extreme_interval_cannot_overflow_rate(self):
        before = self.snap(0)
        after = self.snap(5e-324, energy=49_000_000, success=4)
        result = sleepdiag.compare(before, after)
        self.assertFalse(result['valid'])
        self.assertIsNone(result['loss_uwh_per_h'])

    def test_widget_recovers_from_corrupt_snapshot(self):
        from PySide6.QtWidgets import QApplication
        from slayer_r9t.sleepdiag_widget import SleepDiagPanel
        QApplication.instance() or QApplication([])
        with tempfile.TemporaryDirectory() as d:
            ps, pw = build(d)
            path = Path(d) / 'st.json'
            path.write_text('{"time":1,"energy_uwh":"bad"}')
            w = SleepDiagPanel(state_path=path, ps_root=ps, power_root=pw)
            self.assertFalse(w.compare_button.isEnabled())
            w.compare()
            self.assertIn('Önce', w.status.text())
            w.prepare()
            self.assertTrue(w.compare_button.isEnabled())
            self.assertIsNotNone(sleepdiag.load_snapshot(path))

    def test_widget(self):
        from PySide6.QtWidgets import QApplication
        from slayer_r9t.sleepdiag_widget import SleepDiagPanel
        QApplication.instance() or QApplication([])
        with tempfile.TemporaryDirectory() as d:
            ps, pw = build(d)
            w = SleepDiagPanel(state_path=Path(d) / 'st.json', ps_root=ps, power_root=pw)
            self.assertFalse(w.compare_button.isEnabled())
            w.prepare()
            self.assertTrue(w.compare_button.isEnabled())
            w.compare()
            self.assertIn('Sonuç: GEÇERSİZ', w.result.text())
            self.assertIn('fiziksel uyku', w.warning.text())


if __name__ == '__main__':
    unittest.main()
