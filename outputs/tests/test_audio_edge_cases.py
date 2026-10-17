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


class MalformedAudioOutputTests(unittest.TestCase):
    def test_malformed_or_overflowing_volume_is_audio_error(self):
        from slayer_r9t.audio import parse_volume, AudioError
        for text in ('Volume: ...', 'Volume: 1.2.3', 'Volume: ' + '9' * 400):
            with self.subTest(text=text[:30]), self.assertRaises(AudioError):
                parse_volume(text)

    def test_command_os_or_decoding_error_is_unavailable(self):
        from unittest.mock import patch
        from slayer_r9t.audio import _run, AudioUnavailable
        errors = [PermissionError('denied'), UnicodeDecodeError('utf8', b'\xff', 0, 1, 'bad')]
        for error in errors:
            with self.subTest(error=type(error).__name__), \
                 patch('slayer_r9t.audio.subprocess.run', side_effect=error), \
                 self.assertRaises(AudioUnavailable):
                _run(['status'])
