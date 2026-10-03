import os
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

from slayer_r9t import processes as pr

os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')


def stat_line(comm, ticks, pid=1):
    # fields after comm: state ppid ... utime(idx11) stime(idx12)
    rest = ['S'] + ['0'] * 11 + [str(ticks), '0'] + ['0'] * 10
    return f'{pid} ({comm}) ' + ' '.join(rest)


class Fake:
    def __init__(self, test):
        tmp = tempfile.TemporaryDirectory()
        test.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.proc = self.root / 'proc'
        self.proc.mkdir()
        self.pci = self.root / 'pci'
        self.pci.mkdir()
        self.now = 0.0

    def add(self, pid, comm, ticks, rss_pages):
        d = self.proc / str(pid)
        d.mkdir(exist_ok=True)
        (d / 'stat').write_text(stat_line(comm, ticks, pid))
        (d / 'statm').write_text(f'100 {rss_pages} 0 0 0 0 0')
        (d / 'comm').write_text(comm + '\n')
        (d / 'cmdline').write_text('SECRET-ARG\0')

    def gpu(self, status):
        d = self.pci / '0000:01:00.0'
        (d / 'power').mkdir(parents=True, exist_ok=True)
        (d / 'vendor').write_text('0x10de\n')
        (d / 'class').write_text('0x030200\n')
        (d / 'power/runtime_status').write_text(status + '\n')

    def sampler(self, **kw):
        return pr.ProcessSampler(proc=self.proc, pci=self.pci, clock=lambda: self.now,
                                 wall=lambda: 1000.0, **kw)


class ParseTests(unittest.TestCase):
    def test_comm_with_spaces_and_parens(self):
        self.assertEqual(pr.parse_stat(stat_line('my (odd) app', 30)), ('my (odd) app', 30))

    def test_bad_input(self):
        self.assertIsNone(pr.parse_stat('garbage'))
        self.assertIsNone(pr.parse_statm_rss('x', 4096))


class SamplerTests(unittest.TestCase):
    def test_cpu_percent_and_ranking(self):
        f = Fake(self)
        f.add(1, 'idle', 0, 5)
        f.add(2, 'busy', 0, 10)
        s = f.sampler(limit=2)
        first = s.sample()
        self.assertEqual(first['cpu'], [])
        self.assertEqual(first['memory'][0]['name'], 'busy')
        f.now = 2.0
        f.add(2, 'busy', int(pr._clk_tck() * 1.0), 10)  # 1 s CPU in 2 s
        r = s.sample()
        self.assertEqual(r['cpu'][0]['name'], 'busy')
        self.assertAlmostEqual(r['cpu'][0]['cpu_percent'], 50.0, places=3)
        self.assertEqual(r['interval_s'], 2.0)
        self.assertEqual(r['timestamp'], 1000.0)

    def test_rss_bytes(self):
        f = Fake(self)
        f.add(1, 'a', 0, 3)
        self.assertEqual(f.sampler().sample()['memory'][0]['rss_bytes'], 3 * pr._page_size())

    def test_unreadable_and_nonnumeric_counted(self):
        f = Fake(self)
        f.add(1, 'ok', 0, 1)
        (f.proc / '2').mkdir()  # no stat file: exited / denied
        (f.proc / 'self').mkdir()
        r = f.sampler().sample()
        self.assertEqual(r['scanned'], 1)
        self.assertEqual(r['unreadable'], 1)
        self.assertIn('okunamadı', r['limits'])

    def test_no_cmdline_leaks(self):
        f = Fake(self)
        f.add(1, 'a', 0, 1)
        r = f.sampler().sample()
        self.assertNotIn('SECRET', repr(r))
        self.assertEqual(set(r['memory'][0]), {'pid', 'name', 'cpu_percent', 'rss_bytes'})

    def test_missing_proc(self):
        s = pr.ProcessSampler(proc=Path('/nonexistent-proc'), pci=Path('/nonexistent-pci'))
        self.assertTrue(s.sample()['error'])

    def test_pid_reuse_name_change_gives_no_cpu(self):
        f = Fake(self)
        f.add(1, 'old', 500, 1)
        s = f.sampler()
        s.sample()
        f.now = 1.0
        f.add(1, 'new', 600, 1)
        self.assertEqual(s.sample()['cpu'], [])


class GpuTests(unittest.TestCase):
    def test_sleeping_gpu_not_queried(self):
        f = Fake(self)
        f.add(1, 'a', 0, 1)
        f.gpu('suspended')
        called = []
        s = f.sampler(run=lambda *a, **k: called.append(a))
        r = s.sample()
        self.assertEqual(called, [])
        self.assertEqual(r['gpu'], [])
        self.assertIn('uyku', r['gpu_note'])

    def test_no_gpu(self):
        f = Fake(self)
        called = []
        r = f.sampler(run=lambda *a, **k: called.append(a)).sample()
        self.assertEqual(called, [])
        self.assertIn('bulunamadı', r['gpu_note'])

    def test_active_gpu_parsed(self):
        f = Fake(self)
        f.add(7, 'game', 0, 1)
        f.add(8, 'tool', 0, 1)
        f.gpu('active')
        out = '7, 900\n8, 50\nbad line\n'
        run = lambda *a, **k: SimpleNamespace(returncode=0, stdout=out)
        r = f.sampler(nvidia_smi='/x/nvidia-smi', run=run).sample()
        self.assertEqual([(g['name'], g['vram_mib']) for g in r['gpu']], [('game', 900), ('tool', 50)])

    def test_smi_failure_visible(self):
        f = Fake(self)
        f.gpu('active')

        def boom(*a, **k):
            raise OSError('x')
        r = f.sampler(nvidia_smi='/x', run=boom).sample()
        self.assertEqual(r['gpu'], [])
        self.assertIn('çalıştırılamadı', r['gpu_note'])


class WidgetTests(unittest.TestCase):
    def test_panel_refreshes_only_when_visible(self):
        from PySide6.QtWidgets import QApplication
        from slayer_r9t.processes_widget import ProcessesPanel
        app = QApplication.instance() or QApplication([])
        f = Fake(self)
        f.add(1, 'busy', 0, 10)
        panel = ProcessesPanel(sampler=f.sampler(), interval_ms=1000)
        self.assertFalse(panel.timer.isActive())
        self.assertEqual(panel.refreshes, 0)
        panel._tick()  # hidden: no refresh
        self.assertEqual(panel.refreshes, 0)
        panel.show()
        app.processEvents()
        self.assertEqual(panel.refreshes, 1)
        self.assertTrue(panel.timer.isActive())
        self.assertIn('busy', panel.lists['memory'].text())
        self.assertIn('Örnek:', panel.status.text())
        panel.hide()
        app.processEvents()
        self.assertFalse(panel.timer.isActive())
        panel.close()


if __name__ == '__main__':
    unittest.main()
