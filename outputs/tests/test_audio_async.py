"""Event-controlled audio workers keep the Qt event loop responsive."""
import threading
import time
import unittest
from PySide6.QtCore import QTimer, QThreadPool
from PySide6.QtWidgets import QApplication
from slayer_r9t.audio import AudioBackend
from slayer_r9t.audio_widget import AudioPanel
from test_audio import FakeWpctl


class AudioAsyncTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def wait_for(self, predicate):
        deadline = time.monotonic() + 3
        while not predicate() and time.monotonic() < deadline:
            self.app.processEvents()
            time.sleep(0.001)
        self.assertTrue(predicate())

    def slow_panel(self):
        entered, release = threading.Event(), threading.Event()
        fake = FakeWpctl()
        threads = []
        def runner(args):
            threads.append(threading.get_ident())
            entered.set()
            if not release.wait(3):
                raise RuntimeError('test release deadline exceeded')
            return fake(args)
        panel = AudioPanel(AudioBackend(runner))
        self.addCleanup(panel.close)
        self.addCleanup(lambda: QThreadPool.globalInstance().waitForDone(3000))
        self.addCleanup(release.set)
        self.wait_for(entered.is_set)
        return panel, release, threads

    def test_slow_refresh_keeps_gui_responsive_and_single_flight(self):
        panel, release, threads = self.slow_panel()
        timer_fired = []
        QTimer.singleShot(0, lambda: timer_fired.append(True))
        self.wait_for(lambda: bool(timer_fired))
        self.assertTrue(panel._busy)
        self.assertFalse(panel.refresh_button.isEnabled())
        task = panel._worker
        panel.refresh()
        panel.apply_volume('sink')
        self.assertIs(panel._worker, task)
        release.set()
        self.wait_for(lambda: not panel._busy)
        self.assertEqual(panel.rows['sink']['slider'].value(), 50)
        self.assertTrue(panel.rows['sink']['slider'].isEnabled())
        self.assertTrue(all(t != threading.get_ident() for t in threads))
        self.assertEqual(len(set(threads)), 1)

    def test_close_does_not_wait_or_apply_late_result(self):
        panel, release, threads = self.slow_panel()
        panel.close()
        text = panel.status_label.text()
        timer_fired = []
        QTimer.singleShot(0, lambda: timer_fired.append(True))
        self.wait_for(lambda: bool(timer_fired))
        self.assertTrue(panel._closed)
        release.set()
        self.wait_for(lambda: panel._worker is None)
        self.assertEqual(panel.status_label.text(), text)
        self.assertFalse(panel.refresh_button.isEnabled())

    def test_unexpected_backend_error_is_reported_and_retry_enabled(self):
        def runner(args):
            raise ValueError('malformed audio output')
        panel = AudioPanel(AudioBackend(runner))
        self.addCleanup(panel.close)
        self.wait_for(lambda: not panel._busy)
        self.assertIn('malformed audio output', panel.status_label.text())
        self.assertTrue(panel.refresh_button.isEnabled())
        self.assertFalse(panel.rows['sink']['slider'].isEnabled())
