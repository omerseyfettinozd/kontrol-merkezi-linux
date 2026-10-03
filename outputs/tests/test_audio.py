import os
import unittest
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')
from slayer_r9t import audio
from slayer_r9t.audio import (AudioBackend, AudioValidationError, AudioReadbackError,
                              AudioError, parse_status, parse_volume)

STATUS = """PipeWire 'pipewire-0'
Audio
 ├─ Devices:
 │      51. Dev A [alsa]
 │
 ├─ Sinks:
 │  *   58. Speakers Analog Stereo [vol: 0.50]
 │      59. HDMI Out [vol: 1.00 MUTED]
 │
 ├─ Sources:
 │  *   63. Mic Analog Stereo [vol: 1.00]
 │      64. Other Mic [vol: 0.30]
 │
 ├─ Filters:
 │
 └─ Streams:
        44. app
             67. output_FL       > X:playback_FL\t[active]

Video
"""


class FakeWpctl:
    def __init__(self, lie=False):
        self.vol = {'sink': 0.5, 'source': 1.0}
        self.mute = {'sink': False, 'source': False}
        self.default = {'sink': 58, 'source': 63}
        self.lie, self.calls = lie, []

    def kind(self, t):
        return 'sink' if 'SINK' in t else 'source'

    def __call__(self, args):
        self.calls.append(args)
        c = args[0]
        if c == 'status':
            return 0, STATUS, ''
        if c == 'get-volume':
            k = self.kind(args[1])
            return 0, f"Volume: {self.vol[k]:.2f}{' [MUTED]' if self.mute[k] else ''}\n", ''
        if c == 'set-volume':
            if not self.lie:
                self.vol[self.kind(args[3])] = float(args[4])
            return 0, '', ''
        if c == 'set-mute':
            if not self.lie:
                self.mute[self.kind(args[1])] = args[2] == '1'
            return 0, '', ''
        if c == 'set-default':
            return 0, '', ''
        return 1, '', 'bad'


class ParseTests(unittest.TestCase):
    def test_status(self):
        s = parse_status(STATUS)
        self.assertEqual([d.id for d in s.sinks], [58, 59])
        self.assertEqual(s.default('sink').name, 'Speakers Analog Stereo')
        self.assertTrue(s.sinks[1].muted)
        self.assertEqual(s.sources[1].volume, 0.30)
        self.assertEqual(s.default('source').id, 63)

    def test_volume(self):
        self.assertEqual(parse_volume('Volume: 0.50\n'), (50, False))
        self.assertEqual(parse_volume('Volume: 1.20 [MUTED]'), (120, True))
        with self.assertRaises(AudioError):
            parse_volume('garbage')


class BackendTests(unittest.TestCase):
    def test_set_volume_ok(self):
        b = AudioBackend(FakeWpctl())
        self.assertEqual(b.set_volume('sink', 75), 75)

    def test_validation(self):
        f = FakeWpctl(); b = AudioBackend(f)
        for bad in (-1, 151, float('nan'), '50', None, True):
            with self.assertRaises(AudioValidationError):
                b.set_volume('sink', bad)
        with self.assertRaises(AudioValidationError):
            b.set_volume('bogus', 10)
        with self.assertRaises(AudioValidationError):
            b.set_mute('sink', 1)
        with self.assertRaises(AudioValidationError):
            b.set_default('sink', 999)
        self.assertEqual(f.calls, [['status']])  # no writes issued
        self.assertEqual(b.set_volume('sink', 150), 150)
        self.assertEqual(b.set_volume('sink', 0), 0)

    def test_readback_mismatch(self):
        b = AudioBackend(FakeWpctl(lie=True))
        with self.assertRaises(AudioReadbackError):
            b.set_volume('sink', 80)
        with self.assertRaises(AudioReadbackError):
            b.set_mute('source', True)
        with self.assertRaises(AudioReadbackError):
            b.set_default('sink', 59)

    def test_mute_and_default(self):
        f = FakeWpctl(); b = AudioBackend(f)
        self.assertTrue(b.set_mute('source', True))
        self.assertTrue(f.mute['source'])
        f.default['sink'] = 58
        with self.assertRaises(AudioReadbackError):  # fake status never changes default
            b.set_default('sink', 59)

    def test_command_failure_and_unavailable(self):
        b = AudioBackend(lambda a: (1, '', 'no pipewire'))
        with self.assertRaises(AudioError):
            b.status()
        ok, reason = b.availability()
        self.assertFalse(ok); self.assertIn('kapalı', reason)

        def missing(a):
            raise audio.AudioUnavailable('wpctl bulunamadı')
        ok, reason = AudioBackend(missing).availability()
        self.assertFalse(ok); self.assertIn('wpctl bulunamadı', reason)


class WidgetTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from PySide6.QtWidgets import QApplication
        cls.app = QApplication.instance() or QApplication([])

    def test_panel_enabled(self):
        from slayer_r9t.audio_widget import AudioPanel
        f = FakeWpctl()
        p = AudioPanel(AudioBackend(f))
        from PySide6.QtWidgets import QLabel
        self.assertEqual(p.rows['sink']['slider'].value(), 50)
        self.assertEqual(p.rows['source']['combo'].count(), 2)
        self.assertTrue(p.rows['sink']['slider'].isEnabled())
        p.rows['source']['mute'].setChecked(True)
        p.apply_mute('source', True)
        self.assertTrue(f.mute['source'])
        p.rows['sink']['slider'].setValue(80)
        p.apply_volume('sink')
        self.assertEqual(f.vol['sink'], 0.8)
        texts = ' '.join(l.text() for l in p.findChildren(QLabel))
        self.assertIn('donanım bağlantısını kesmez', texts)

    def test_panel_disabled_with_reason(self):
        from slayer_r9t.audio_widget import AudioPanel
        p = AudioPanel(AudioBackend(lambda a: (1, '', 'x')))
        self.assertFalse(p.rows['sink']['slider'].isEnabled())
        self.assertFalse(p.rows['source']['mute'].isEnabled())
        self.assertIn('kapalı', p.status_label.text())

    def test_readback_error_shown(self):
        from slayer_r9t.audio_widget import AudioPanel
        p = AudioPanel(AudioBackend(FakeWpctl(lie=True)))
        p.rows['sink']['slider'].setValue(90)
        p.apply_volume('sink')
        self.assertIn('doğrulanamadı', p.status_label.text())


if __name__ == '__main__':
    unittest.main()
