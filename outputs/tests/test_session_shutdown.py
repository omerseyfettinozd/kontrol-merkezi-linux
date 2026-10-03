"""Session shutdown restores temporary display changes without real D-Bus calls."""
import signal
import socket
import struct
from types import SimpleNamespace
import unittest
from unittest.mock import MagicMock, Mock, patch
from slayer_r9t import session


class SessionShutdownTests(unittest.TestCase):
    def test_close_restores_all_saved_modes_and_lighting(self):
        rules = session.SessionRules()
        rules.saved_modes = {'internal': '144', 'external': '120'}
        rules.low_active = True
        with patch.object(session.desktop, 'set_mode') as restore, patch.object(session, 'request') as request:
            rules.close()
        self.assertEqual(restore.call_args_list, [unittest.mock.call('internal', '144'),
                                                unittest.mock.call('external', '120')])
        self.assertEqual(rules.saved_modes, {})
        self.assertFalse(rules.low_active)
        request.assert_called_once_with({'op': 'lighting_idle', 'idle': False})

    def test_failure_keeps_only_failed_modes_and_attempts_other_cleanup(self):
        rules = session.SessionRules()
        rules.saved_modes = {'first': '144', 'second': '120'}
        rules.idle_helper = SimpleNamespace(terminate=Mock(side_effect=OSError('helper denied')), wait=Mock())
        failure = ValueError('display disconnected')
        with patch.object(session.desktop, 'set_mode', side_effect=[failure, None]) as restore, \
             patch.object(session, 'request', side_effect=OSError('socket unavailable')) as request:
            with self.assertRaisesRegex(RuntimeError, 'display disconnected') as result:
                rules.close()
        self.assertIs(result.exception.__cause__, failure)
        self.assertIn('helper denied', str(result.exception))
        self.assertIn('socket unavailable', str(result.exception))
        self.assertEqual(restore.call_count, 2)
        self.assertEqual(rules.saved_modes, {'first': '144'})
        self.assertTrue(rules.low_active)
        request.assert_called_once()
        with patch.object(session.desktop, 'set_mode'), patch.object(session, 'request'):
            rules.idle_helper = None
            rules.close()
        self.assertEqual(rules.saved_modes, {})

    def test_manual_request_preserves_user_mode_during_shutdown(self):
        rules = session.SessionRules()
        rules.saved_modes = {'automatic': '144', 'manual': '120'}
        handlers = {}
        ticks = [0]
        def tick():
            ticks[0] += 1
            if ticks[0] == 2:
                handlers[signal.SIGTERM]()
        server, connection = MagicMock(), MagicMock()
        server.__enter__.return_value = server
        connection.__enter__.return_value = connection
        connection.getsockopt.return_value = struct.pack('3i', 1, 1000, 1000)
        server.accept.side_effect = [(connection, None), socket.timeout]
        path = MagicMock()
        path.exists.return_value = False
        payload = {'op': 'mode', 'output': 'manual', 'mode': 'user-selected'}
        with patch.object(session.os, 'getuid', return_value=1000), \
             patch.object(session.signal, 'signal', side_effect=lambda n, handler: handlers.update({n: handler})), \
             patch.object(session, 'socket_path', return_value='/mock/session.sock'), \
             patch.object(session, 'Path', return_value=path), \
             patch.object(session, 'SessionRules', return_value=rules), \
             patch.object(rules, 'tick', side_effect=tick), \
             patch.object(session.socket, 'socket', return_value=server), \
             patch.object(session.os, 'chmod'), \
             patch.object(session, 'receive', return_value=payload), \
             patch.object(session, 'request'), \
             patch.object(session.desktop, 'dispatch', return_value={}) as dispatch, \
             patch.object(session.desktop, 'set_mode') as restore:
            session.main()
        dispatch.assert_called_once_with(payload)
        restore.assert_called_once_with('automatic', '144')
        self.assertEqual(rules.saved_modes, {})

    def test_main_sigterm_and_loop_failure_both_close_and_remove_socket(self):
        for fail in (False, True):
            with self.subTest(fail=fail):
                handlers = {}
                rules = Mock()
                def tick():
                    if fail:
                        raise ValueError('loop failed')
                    handlers[signal.SIGTERM]()
                rules.tick.side_effect = tick
                server = MagicMock()
                server.__enter__.return_value = server
                server.accept.side_effect = socket.timeout
                path = MagicMock()
                path.exists.return_value = False
                with patch.object(session.os, 'getuid', return_value=1000), \
                     patch.object(session.signal, 'signal', side_effect=lambda signum, handler: handlers.update({signum: handler})), \
                     patch.object(session, 'socket_path', return_value='/mock/session.sock'), \
                     patch.object(session, 'Path', return_value=path), \
                     patch.object(session, 'SessionRules', return_value=rules), \
                     patch.object(session.socket, 'socket', return_value=server), \
                     patch.object(session.os, 'chmod'):
                    if fail:
                        with self.assertRaisesRegex(ValueError, 'loop failed'):
                            session.main()
                    else:
                        session.main()
                rules.close.assert_called_once()
                path.unlink.assert_called_once_with(missing_ok=True)
