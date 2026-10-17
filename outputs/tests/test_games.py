"""Steam library scan tests."""
import unittest
from pathlib import Path
import tempfile
from slayer_r9t.games import steam_games


class SteamGamesTest(unittest.TestCase):
    def test_parses_manifests_and_skips_runtimes(self):
        with tempfile.TemporaryDirectory() as tmp:
            apps = Path(tmp) / 'steamapps'
            apps.mkdir()
            (apps / 'appmanifest_730.acf').write_text(
                '"AppState"\n{\n\t"appid"\t\t"730"\n\t"name"\t\t"Counter-Strike 2"\n'
                '\t"installdir"\t\t"Counter-Strike Global Offensive"\n}\n')
            (apps / 'appmanifest_1.acf').write_text(
                '"AppState"\n{\n\t"appid"\t\t"1"\n\t"name"\t\t"Proton Experimental"\n'
                '\t"installdir"\t\t"Proton Experimental"\n}\n')
            games = steam_games(steam_root=tmp)
            self.assertEqual([g['name'] for g in games], ['Counter-Strike 2'])
            self.assertEqual(games[0]['appid'], '730')

    def test_real_library_found(self):
        games = steam_games()
        names = [g['name'] for g in games]
        self.assertIn('Counter-Strike 2', names)
        self.assertFalse(any('Proton' in n for n in names))
