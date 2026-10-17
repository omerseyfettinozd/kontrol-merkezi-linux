"""Real process races, activation forwarding and crash recovery without hardware."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
from PySide6.QtGui import QIcon
from PySide6.QtWidgets import QApplication
from slayer_r9t.gui import APP_ICON
from slayer_r9t.single_instance import SingleInstance


class SingleInstanceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)

    def child(self, code):
        return subprocess.Popen([sys.executable, '-c', code, self.temp.name],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    def test_five_launches_forward_to_one_owner(self):
        owner = SingleInstance(self.temp.name)
        self.addCleanup(owner.close)
        activations = []
        owner.activated.connect(activations.append)
        children = [self.child('''
import sys
from PySide6.QtCore import QCoreApplication
from slayer_r9t.single_instance import SingleInstance
app = QCoreApplication([])
instance = SingleInstance(sys.argv[1])
assert not instance.primary
assert instance.activate_existing(fan=True)
''') for _ in range(5)]
        try:
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline and (any(p.poll() is None for p in children) or len(activations) < 5):
                self.app.processEvents()
                time.sleep(0.01)
            for child in children:
                output, errors = child.communicate(timeout=1)
                self.assertEqual(child.returncode, 0, errors.decode())
            self.assertEqual(activations, [True] * 5)
        finally:
            for child in children:
                if child.poll() is None:
                    child.kill()
                child.communicate()

    def test_crashed_owner_leaves_recoverable_socket(self):
        child = self.child('''
import os, sys
from PySide6.QtCore import QCoreApplication
from slayer_r9t.single_instance import SingleInstance
app = QCoreApplication([])
instance = SingleInstance(sys.argv[1])
assert instance.primary
os._exit(0)
''')
        _, errors = child.communicate(timeout=5)
        self.assertEqual(child.returncode, 0, errors.decode())
        self.assertTrue((Path(self.temp.name) / 'slayer-r9t-control-center.sock').exists())
        owner = SingleInstance(self.temp.name)
        self.addCleanup(owner.close)
        self.assertTrue(owner.primary)

    def test_close_releases_ownership_without_unlinking_lock(self):
        owner = SingleInstance(self.temp.name)
        inode = Path(owner.lock.name).stat().st_ino
        owner.close()
        owner.close()
        replacement = SingleInstance(self.temp.name)
        self.addCleanup(replacement.close)
        self.assertTrue(replacement.primary)
        self.assertEqual(Path(replacement.lock.name).stat().st_ino, inode)

    def test_bundled_icon_renders_at_tray_and_launcher_sizes(self):
        icon = QIcon(str(APP_ICON))
        for size in (16, 22, 32, 48, 256):
            self.assertFalse(icon.pixmap(size, size).isNull())
