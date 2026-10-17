"""Read-only Steam library scan for automation suggestions."""
import re
from pathlib import Path

SKIP = ('proton', 'runtime', 'redistributable', 'steamworks', 'steam linux runtime')


def steam_games(steam_root=None):
    roots = []
    if steam_root is not None:
        roots.append(Path(steam_root))
    else:
        roots.append(Path.home() / '.local/share/Steam')
        roots.append(Path.home() / '.steam/steam')
    games = []
    seen = set()
    for root in roots:
        apps = root / 'steamapps'
        if not apps.is_dir():
            continue
        for manifest in sorted(apps.glob('appmanifest_*.acf')):
            try:
                text = manifest.read_text(encoding='utf-8', errors='replace')
            except OSError:
                continue
            name = re.search(r'"name"\s+"([^"]+)"', text)
            folder = re.search(r'"installdir"\s+"([^"]+)"', text)
            appid = re.search(r'"appid"\s+"([^"]+)"', text)
            if not name:
                continue
            title = name.group(1).strip()
            if not title or title.lower() in seen or any(k in title.lower() for k in SKIP):
                continue
            seen.add(title.lower())
            games.append({'name': title,
                          'appid': appid.group(1) if appid else '',
                          'folder': folder.group(1) if folder else ''})
    return sorted(games, key=lambda g: g['name'].lower())
