"""Edge cases and mock failure tests for AudioBackend."""
import unittest
from slayer_r9t.audio import AudioBackend, AudioValidationError, AudioReadbackError, AudioUnavailable, AudioError

class AudioBackendEdgeCasesTest(unittest.TestCase):
    def test_invalid_kind_rejected(self):
        backend = AudioBackend()
        with self.assertRaises(AudioValidationError):
            backend.set_volume('invalid_kind', 50)
        with self.assertRaises(AudioValidationError):
            backend.set_mute('invalid_kind', True)

    def test_volume_out_of_bounds(self):
        backend = AudioBackend()
        with self.assertRaises(AudioValidationError):
            backend.set_volume('sink', -5)
        with self.assertRaises(AudioValidationError):
            backend.set_volume('sink', 151)

    def test_unavailable_runner(self):
        def failing_runner(args):
            return 127, '', 'wpctl: command not found'
        backend = AudioBackend(runner=failing_runner)
        ok, reason = backend.availability()
        self.assertFalse(ok)
        self.assertIn('WirePlumber', reason)
        with self.assertRaises(AudioError):
            backend.set_volume('sink', 50)

    def test_readback_mismatch_raises_readback_error(self):
        def mismatch_runner(args):
            if 'set-volume' in args:
                return 0, '', ''
            if 'get-volume' in args:
                return 0, 'Volume: 0.10', ''
            return 0, '', ''
        backend = AudioBackend(runner=mismatch_runner)
        with self.assertRaises(AudioReadbackError):
            backend.set_volume('sink', 80)
