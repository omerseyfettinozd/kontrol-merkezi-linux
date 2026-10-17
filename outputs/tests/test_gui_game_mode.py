"""Game mode and power/battery UX improvements."""
import unittest
from PySide6.QtWidgets import QApplication, QPushButton
from slayer_r9t.gui import ControlCenter

app = QApplication.instance() or QApplication([])


class GuiGameModeTest(unittest.TestCase):
    def setUp(self):
        self.win = ControlCenter()

    def tearDown(self):
        self.win.timer.stop()
        self.win.close()

    def test_game_mode_buttons_exist(self):
        texts = [b.text() for b in self.win.findChildren(QPushButton)]
        self.assertIn('Oyun Modunu Aç', texts)
        self.assertIn('Günlük Moda Dön', texts)
        self.assertIn('Sessiz Fan', texts)
        self.assertIn('Dengeli Fan', texts)
        self.assertIn('Serin Fan', texts)

    def test_game_mode_queues_both_commands(self):
        calls = []
        self.win.command = lambda op, *a, **k: calls.append((op, a))
        self.win.game_mode_on()
        self.assertEqual([c[0] for c in calls], ['thermal_preset', 'fan_preset'])
        self.assertEqual(calls[0][1], ('gaming_cool',))
        self.assertEqual(calls[1][1], ('cool',))
        calls.clear()
        self.win.game_mode_off()
        self.assertEqual([c[0] for c in calls], ['thermal_preset', 'auto'])

    def test_charge_controls_hidden_when_unsupported(self):
        state = {'cooling': {'epp': {'available': True, 'choices': [], 'value': None, 'policy_count': 0, 'reason': ''}},
                 'features': {'battery': {'capacity': 100, 'status': 'Full', 'health': 100.0, 'cycles': 0, 'voltage_v': 16.8, 'current_a': 0.0, 'power_w': 0.0},
                              'charge_limit': {'available': False, 'reason': 'yok'},
                              'camera': {}, 'fn_lock': {}},
                 'power_profile': 'balanced', 'boost_enabled': True,
                 'automation': {'document': {'automation': {}}}, 'fans': {}}
        self.win.update_state(state)
        self.assertFalse(self.win.charge_end.isEnabled())
        self.assertFalse(self.win.charge_end.isVisible())
        self.assertIn('şarj sınırı yok', self.win.charge_reason.text())
        self.assertIn('normal', self.win.battery_text.text())

    def test_power_combined_hint(self):
        state = {'cooling': {'epp': {'available': True, 'choices': [], 'value': 'power', 'policy_count': 16, 'reason': ''}},
                 'features': {'battery': {}, 'charge_limit': {'available': False, 'reason': ''}, 'camera': {}, 'fn_lock': {}},
                 'power_profile': 'power-saver', 'boost_enabled': False,
                 'automation': {'document': {'automation': {}}}, 'fans': {}}
        self.win.update_state(state)
        self.assertIn('en sessiz', self.win.power_state.text())

    def test_scenario_buttons_queue(self):
        calls = []
        self.win.command = lambda op, *a, **k: calls.append((op, a))
        self.win.scenario_quiet()
        self.assertEqual([c[0] for c in calls], ['thermal_preset', 'auto'])
        calls.clear()
        self.win.scenario_game()
        self.assertEqual(calls[0][1], ('gaming_cool',))
        calls.clear()
        self.win.scenario_turbo()
        self.assertIn('fan_boost', [c[0] for c in calls])

    def test_scenario_state_text(self):
        base = {'cooling': {'epp': {'available': True, 'choices': [], 'value': 'power', 'policy_count': 0, 'reason': ''},
                            'cpu': {'max_mhz': 1800}, 'gpu': {}},
                'features': {'battery': {}, 'charge_limit': {'available': False, 'reason': ''}, 'camera': {}, 'fn_lock': {}},
                'power_profile': 'power-saver', 'boost_enabled': False,
                'automation': {'document': {'automation': {}}}, 'fans': {}}
        self.win.update_state(dict(base))
        self.assertIn('Sessiz', self.win.scenario_state.text())
        self.assertIn('2dd4bf', self.win.scenario_buttons['Sessiz'].styleSheet())
        self.assertEqual(self.win.scenario_buttons['Dengeli'].styleSheet(), '')

    def test_scenario_turbo_blocked_on_battery(self):
        self.win.state = {'capabilities': {'power': True, 'boost': True, 'cpu_limit': True, 'gpu_limit': True},
                          'features': {'battery': {'status': 'Discharging'}}}
        msgs = []
        self.win.feedback = lambda m, error=False, **k: msgs.append((m, error))
        self.win.scenario_turbo()
        self.assertTrue(msgs and msgs[0][1])
        self.win.update_buttons()
        self.assertFalse(self.win.scenario_buttons['Turbo'].isEnabled())

    def test_save_game_profile(self):
        self.win.state = {'capabilities': {'cpu_limit': True, 'gpu_limit': True}}
        self.win.store.profiles.pop('Oyun - Serin', None)
        calls = []
        self.win.command = lambda op, *a, **k: calls.append((op, a))
        self.win.save_game_profile()
        self.assertIn('Oyun - Serin', self.win.store.profiles)
        saved = self.win.store.profiles['Oyun - Serin']
        self.assertEqual(saved['cpu_max_mhz'], 3500)
        self.assertEqual(saved['gpu_max_mhz'], 2100)
        self.assertEqual(saved['fan'], {'mode': 'preset', 'preset': 'cool'})
        self.win.store.profiles.pop('Oyun - Serin', None)
