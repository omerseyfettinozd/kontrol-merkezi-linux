"""Read-only removal inventory. No executor and no inferred deletion authority."""
import hashlib
import os
from pathlib import Path
import stat

APP = '/usr/local/lib/slayer-r9t'
PROGRAMS = ('r9t-service.py', 'r9t-client.py', 'r9t-control.py',
            'r9t-control-center.py', 'r9t-session.py', 'r9t-desktop-client.py',
            'r9t-gpu-diagnostics.py', 'r9t-dynamic-boost.py')
UNITS = {
    '/etc/systemd/system/slayer-r9t.service':
        b'ExecStart=/usr/bin/python /usr/local/lib/slayer-r9t/r9t-service.py',
    '/etc/systemd/user/slayer-r9t-session.service':
        b'ExecStart=/usr/bin/python /usr/local/lib/slayer-r9t/r9t-session.py',
    '/usr/share/applications/slayer-r9t-control-center.desktop':
        b'Exec=/usr/bin/python /usr/local/lib/slayer-r9t/r9t-control-center.py',
}
MAX_FILE = 2 * 1024 * 1024


def _open(root, logical):
    """Walk every path component through directory descriptors without symlinks."""
    fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        parts = Path(logical).parts[1:]
        for index, part in enumerate(parts):
            if part in ('', '.', '..'):
                raise ValueError('Unsafe path component')
            flags = os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK
            if index < len(parts) - 1:
                flags |= os.O_DIRECTORY
            else:
                mode = os.stat(part, dir_fd=fd, follow_symlinks=False).st_mode
                if not (stat.S_ISREG(mode) or stat.S_ISDIR(mode)):
                    raise ValueError('Symlink or special file is blocked')
            child = os.open(part, flags, dir_fd=fd)
            os.close(fd)
            fd = child
        return fd
    except BaseException:
        os.close(fd)
        raise


def _inspect(root, logical, owner_uid, reference=None, marker=None, listing=False):
    item = {'path': logical, 'status': 'uncertain', 'removable': False}
    try:
        fd = _open(root, logical)
        try:
            metadata = os.fstat(fd)
            item['uid'] = metadata.st_uid
            if metadata.st_uid != owner_uid:
                item.update(status='blocked', reason='Unexpected owner')
                return item, []
            if listing:
                if not stat.S_ISDIR(metadata.st_mode):
                    item.update(status='blocked', reason='Expected directory')
                    return item, []
                names = sorted(os.listdir(fd))
                item['reason'] = 'No installed ownership manifest; directory is not removable'
                return item, names
            if not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1:
                item.update(status='blocked', reason='Expected regular file with one link')
                return item, []
            if metadata.st_size > MAX_FILE:
                item.update(status='blocked', reason='File exceeds inventory read bound')
                return item, []
            with os.fdopen(os.dup(fd), 'rb') as handle:
                data = handle.read(MAX_FILE + 1)
            if len(data) > MAX_FILE:
                item.update(status='blocked', reason='File changed beyond read bound')
                return item, []
            item['sha256'] = hashlib.sha256(data).hexdigest()
            if reference is not None:
                if data != reference:
                    item.update(status='blocked', reason='Content differs from checkout reference')
                else:
                    item.update(content_matches_reference=True,
                                reason='Content matches; installed ownership manifest is absent')
            elif marker is not None:
                if marker not in data.splitlines():
                    item.update(status='blocked', reason='Expected application marker absent')
                else:
                    item.update(marker_matches=True,
                                reason='Marker matches; full trusted installation hash is absent')
            else:
                item['reason'] = 'No trusted installation hash or ownership manifest'
            return item, []
        finally:
            os.close(fd)
    except FileNotFoundError:
        item.update(status='absent', reason='Path absent')
    except (OSError, ValueError) as error:
        item.update(status='blocked', reason='Cannot inspect without following links: ' + str(error))
    return item, []


def inventory(source, root=Path('/'), owner_uid=0):
    """Fixed application paths only; data and all drivers remain preserved."""
    source = Path(source)
    items = []
    parent, names = _inspect(root, APP, owner_uid, listing=True)
    items.append(parent)
    known = set(PROGRAMS) | {'r9t-idle', 'slayer_r9t'}
    if parent['status'] == 'uncertain':
        for name in names:
            logical = APP + '/' + name
            if name not in known:
                items.append({'path': logical, 'status': 'blocked', 'removable': False,
                              'reason': 'Unknown installed entry; never traversed'})
            elif name == 'slayer_r9t':
                package, children = _inspect(root, logical, owner_uid, listing=True)
                items.append(package)
                references = {p.name: p for p in (source / 'slayer_r9t').glob('*.py')}
                for child in children:
                    path = logical + '/' + child
                    if child not in references:
                        items.append({'path': path, 'status': 'blocked', 'removable': False,
                                      'reason': 'Unknown package entry; never traversed'})
                    else:
                        items.append(_inspect(root, path, owner_uid,
                                              reference=references[child].read_bytes())[0])
            else:
                reference = (source / name).read_bytes() if name in PROGRAMS else None
                items.append(_inspect(root, logical, owner_uid, reference=reference)[0])
    for path, marker in UNITS.items():
        items.append(_inspect(root, path, owner_uid, marker=marker)[0])
    preserve = [
        '/usr/local/lib/.slayer-r9t-previous', '/var/lib/slayer-r9t',
        '~/.config/slayer-r9t-control-center', '~/.local/state/slayer-r9t-control-center',
        '~/.local/share/applications/slayer-r9t-control-center.desktop',
        '/usr/src/slayer-r9t-fan-0.1.0', '/var/lib/dkms/slayer-r9t-fan/0.1.0',
        '/run/slayer-r9t', 'r9t_fan kernel module', 'TUXEDO/Uniwill/ITE drivers',
        'systemd enable symlinks (manager inspection not performed)',
    ]
    return {'mode': 'dry-run', 'executor_available': False,
            'ownership_manifest': 'absent: current installer writes no manifest',
            'items': items, 'preserve': preserve,
            'warning': 'Inventory only; no path is authorized for removal. Driver removal is separate.'}
