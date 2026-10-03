import os
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

from slayer_r9t import processes as pr

os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')


def stat_line(comm, ticks, pid=1, starttime=1):
    # fields after comm: state ppid ... utime(idx11) stime(idx12)
    rest = ['S'] + ['0'] * 23
    rest[11] = str(ticks)
    rest[19] = str(starttime)
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

    def add(self, pid, comm, ticks, rss_pages, starttime=1):
        d = self.proc / str(pid)
        d.mkdir(exist_ok=True)
        (d / 'stat').write_text(stat_line(comm, ticks, pid, starttime))
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
        self.assertEqual(pr.parse_stat(stat_line('my (odd) app', 30, 1)), ('my (odd) app', 30, 1))

    def test_bad_input(self):
        self.assertIsNone(pr.parse_stat('garbage'))
        self.assertIsNone(pr.parse_statm_rss('x', 4096))

    def test_invalid_or_missing_starttime_is_rejected(self):
        for starttime in ('broken', -1):
            with self.subTest(starttime=starttime):
                self.assertIsNone(pr.parse_stat(stat_line('app', 30, starttime=starttime)))
        incomplete = '1 (app) ' + ' '.join(['S'] + ['0'] * 13)
        self.assertIsNone(pr.parse_stat(incomplete))


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

    def test_same_name_pid_reuse_discards_old_cpu_baseline(self):
        f = Fake(self)
        f.add(42, 'same-app', 10, 1, starttime=100)
        sampler = f.sampler()
        sampler.sample()
        f.now = 1.0
        f.add(42, 'same-app', 110, 1, starttime=200)
        reused = sampler.sample()
        self.assertEqual(reused['cpu'], [])
        self.assertEqual(reused['memory'][0]['name'], 'same-app')
        f.now = 2.0
        f.add(42, 'same-app', 110 + pr._clk_tck(), 1, starttime=200)
        self.assertAlmostEqual(sampler.sample()['cpu'][0]['cpu_percent'], 100.0)


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
    def wait_for(self,condition):
        from PySide6.QtTest import QTest
        for _ in range(100):
            if condition():return
            QTest.qWait(10)
        self.assertTrue(condition(),'Timed out waiting for worker')

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
        from PySide6.QtTest import QTest
        for _ in range(100):
            app.processEvents()
            if panel.refreshes:break
            QTest.qWait(10)
        self.assertEqual(panel.refreshes, 1)
        self.assertTrue(panel.timer.isActive())
        self.assertIn('busy', panel.lists['memory'].text())
        self.assertIn('Örnek:', panel.status.text())
        panel.hide()
        app.processEvents()
        self.assertFalse(panel.timer.isActive())
        panel.close()

    def test_slow_sample_keeps_gui_responsive_and_single_flight(self):
        import threading
        from PySide6.QtCore import QTimer,QThreadPool
        from PySide6.QtWidgets import QApplication
        from slayer_r9t.processes_widget import ProcessesPanel
        app=QApplication.instance() or QApplication([])
        entered=threading.Event();release=threading.Event();calls=[]
        f=Fake(self);f.add(1,'busy',0,10);data=f.sampler().sample()
        def sample():
            calls.append(threading.get_ident());entered.set()
            if not release.wait(2):raise RuntimeError('release timeout')
            return data
        panel=ProcessesPanel(sampler=SimpleNamespace(sample=sample))
        try:
            panel.show();self.wait_for(entered.is_set)
            ticks=[];QTimer.singleShot(0,lambda:ticks.append(True))
            self.wait_for(lambda:bool(ticks))
            panel.refresh();panel._tick()
            self.assertEqual(len(calls),1)
            self.assertNotEqual(calls[0],threading.get_ident())
            self.assertEqual(panel.refreshes,0)
            release.set();self.wait_for(lambda:panel.refreshes==1)
            self.assertIn('busy',panel.lists['memory'].text())
        finally:release.set();panel.close();QThreadPool.globalInstance().waitForDone(2000)

    def test_close_during_sample_discards_result_and_can_reopen(self):
        import threading
        from PySide6.QtCore import QThreadPool
        from PySide6.QtWidgets import QApplication
        from slayer_r9t.processes_widget import ProcessesPanel
        app=QApplication.instance() or QApplication([])
        entered=threading.Event();release=threading.Event();f=Fake(self)
        data=f.sampler().sample()
        def sample():
            entered.set()
            if not release.wait(2):raise RuntimeError('release timeout')
            return data
        panel=ProcessesPanel(sampler=SimpleNamespace(sample=sample))
        try:
            panel.refresh();self.assertFalse(entered.is_set())
            panel.show();self.wait_for(entered.is_set)
            panel.close();self.assertFalse(panel.timer.isActive())
            release.set();self.wait_for(lambda:panel._worker is None)
            self.assertEqual(panel.refreshes,0)
            self.assertIsNone(panel.last)
            panel.show();self.wait_for(lambda:panel.refreshes==1)
        finally:release.set();panel.close();QThreadPool.globalInstance().waitForDone(2000)

    def test_sample_failure_reports_error_and_allows_retry(self):
        from PySide6.QtWidgets import QApplication
        from slayer_r9t.processes_widget import ProcessesPanel
        app=QApplication.instance() or QApplication([])
        f=Fake(self);data=f.sampler().sample();results=[RuntimeError('sample denied'),data]
        def sample():
            value=results.pop(0)
            if isinstance(value,Exception):raise value
            return value
        panel=ProcessesPanel(sampler=SimpleNamespace(sample=sample))
        try:
            panel.show();self.wait_for(lambda:'sample denied' in panel.status.text())
            panel.refresh();self.wait_for(lambda:panel.refreshes==1)
            self.assertEqual(panel.last,data)
        finally:panel.close()


if __name__ == '__main__':
    unittest.main()
