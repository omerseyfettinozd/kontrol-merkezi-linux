#!/usr/bin/env python3
"""Root-owned application/session deployment with rollback on installation errors."""
import os
from pathlib import Path
import pwd
import shutil
import subprocess
import tempfile
import time
import shlex
import sys
import stat


def systemctl(*args):
    subprocess.run(['/usr/bin/systemctl', *args], check=True)


def service_state(command, name, unit_exists=True):
    """Read both managers before changing files; a missing session bus is an error."""
    active = subprocess.run(command + ['is-active', name], capture_output=True, text=True)
    state = active.stdout.strip()
    if active.returncode not in (0, 3, 4) or state not in ('active', 'reloading', 'activating', 'deactivating', 'inactive', 'failed', 'unknown', 'maintenance'):
        raise RuntimeError(f'{name}: servis durumu okunamadı: {active.stderr.strip()}')
    enabled = subprocess.run(command + ['is-enabled', name], capture_output=True, text=True)
    enabled_state = enabled.stdout.strip()
    if enabled.returncode not in (0, 1, 3, 4) or enabled_state not in ('enabled', 'enabled-runtime', 'disabled', 'static', 'indirect', 'linked', 'linked-runtime', 'masked', 'masked-runtime', 'not-found', ''):
        raise RuntimeError(f'{name}: açılış durumu okunamadı: {enabled.stderr.strip()}')
    if not enabled_state and (unit_exists or enabled.returncode not in (1, 4)):
        raise RuntimeError(f'{name}: açılış durumu okunamadı: {enabled.stderr.strip()}')
    return {'active': state in ('active', 'reloading', 'activating'), 'enabled': enabled_state}


def file_snapshot(path):
    try:
        metadata = path.lstat()
    except FileNotFoundError:
        return None
    if path.is_symlink():
        return ('link', os.readlink(path), metadata)
    if not stat.S_ISREG(metadata.st_mode):
        raise RuntimeError(f'Kurulum hedefi normal dosya değil: {path}')
    return ('file', path.read_bytes(), metadata)


def restore_file(path, snapshot):
    path.unlink(missing_ok=True)
    if snapshot is None:
        return
    kind, data, metadata = snapshot
    if kind == 'link':
        path.symlink_to(data)
        os.chown(path, metadata.st_uid, metadata.st_gid, follow_symlinks=False)
    else:
        path.write_bytes(data)
        os.chown(path, metadata.st_uid, metadata.st_gid)
        os.chmod(path, stat.S_IMODE(metadata.st_mode))
        os.utime(path, ns=(metadata.st_atime_ns, metadata.st_mtime_ns))


def write_managed_file(path, text):
    # Replace a saved symlink rather than modifying its external target.
    if path.is_symlink():
        path.unlink()
    path.write_text(text)


def main():
    if os.geteuid() != 0:
        raise SystemExit('sudo veya pkexec ile çalıştırın.')
    raw_uid = os.environ.get('SUDO_UID', os.environ.get('PKEXEC_UID'))
    if raw_uid is None or not raw_uid.isdigit() or int(raw_uid) <= 0:
        raise SystemExit('Masaüstü kullanıcısı kimliği gerekli; sudo veya pkexec kullanın.')
    uid = int(raw_uid)
    user = pwd.getpwuid(uid)
    source = Path(__file__).resolve().parent
    if '--app-only' in sys.argv:
        installed_driver=Path('/usr/src/slayer-r9t-fan-0.1.0')
        if not Path('/sys/module/r9t_fan').exists() or any((source/'fan-driver'/name).read_bytes()!=(installed_driver/name).read_bytes() for name in ('r9t_fan.c','Makefile','build.sh','dkms.conf')):
            raise RuntimeError('Uygulama güncellemesi için fan sürücüsü kaynaklarının aynı ve modülün yüklü olması gerekli.')
    else:subprocess.run(['/usr/bin/python',str(source/'install-fan-driver.py')],check=True)
    return install_application(source, Path('/usr/local/lib/slayer-r9t'), uid, user,
                               Path('/etc/systemd/system/slayer-r9t.service'),
                               Path('/usr/share/applications/slayer-r9t-control-center.desktop'),
                               Path('/etc/systemd/user/slayer-r9t-session.service'))


def install_application(source, target, uid, user, unit_path, desktop_path, user_unit_path):
    """Application transaction only; DKMS installation is an earlier, separate step."""
    target.parent.mkdir(parents=True, exist_ok=True)
    backup = target.with_name('.slayer-r9t-previous')
    old = Path(user.pw_dir)/'.local/share/applications/slayer-r9t-control-center.desktop'
    paths = (unit_path, desktop_path, old, user_unit_path)
    originals = {path: file_snapshot(path) for path in paths}
    environment=['/usr/bin/runuser','-u',user.pw_name,'--','/usr/bin/env',f'XDG_RUNTIME_DIR=/run/user/{uid}',f'DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/{uid}/bus','/usr/bin/systemctl','--user']
    stage = Path(tempfile.mkdtemp(prefix='.slayer-r9t-stage-', dir=target.parent))
    root_command = ['/usr/bin/systemctl']
    root_name = 'slayer-r9t.service'
    session_name = 'slayer-r9t-session.service'
    unit = f'''[Unit]
Description=GameGaraj Slayer R9T hardware control
After=systemd-modules-load.service

[Service]
Type=notify
Environment=R9T_UID={uid}
ExecStartPre=-/usr/bin/modprobe uniwill_wmi
ExecStartPre=-/usr/bin/modprobe tuxedo_io
ExecStartPre=-/usr/bin/modprobe r9t_fan
ExecStart=/usr/bin/python /usr/local/lib/slayer-r9t/r9t-service.py
ExecStopPost=/usr/bin/python /usr/local/lib/slayer-r9t/r9t-service.py --auto-once
RuntimeDirectory=slayer-r9t
RuntimeDirectoryMode=0755
StateDirectory=slayer-r9t
StateDirectoryMode=0700
Restart=on-failure
RestartSec=3
WatchdogSec=20
TimeoutStopSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectHome=true
ProtectSystem=strict
ReadWritePaths=/run/slayer-r9t /var/lib/slayer-r9t
RestrictAddressFamilies=AF_UNIX
UMask=0077

[Install]
WantedBy=multi-user.target
'''
    changed = False
    transaction_started = False
    root_before = session_before = None
    modified = set()
    root_touched = session_touched = False
    try:
        for name in ('r9t-service.py', 'r9t-client.py', 'r9t-control.py', 'r9t-control-center.py','r9t-session.py','r9t-desktop-client.py','r9t-gpu-diagnostics.py','r9t-dynamic-boost.py'):
            shutil.copyfile(source/name, stage/name)
        flags=shlex.split(subprocess.check_output(['pkg-config','--cflags','--libs','Qt6Gui'],text=True))
        subprocess.run(['c++','-std=c++17',str(source/'session-idle.cpp'),'-I/usr/include/KF6/KIdleTime',*flags,'-lKF6IdleTime','-o',str(stage/'r9t-idle')],check=True)
        shutil.copytree(source/'slayer_r9t', stage/'slayer_r9t', ignore=shutil.ignore_patterns('__pycache__', '*.pyc'))
        for path in [stage, *stage.rglob('*')]:
            os.chown(path, 0, 0)
            os.chmod(path, 0o755 if path.is_dir() or path.name=='r9t-idle' else 0o644)
        # Import and dependency check before stopping the installed service.
        subprocess.run(['/usr/bin/python', '-c', 'import slayer_r9t.gui, slayer_r9t.service'], cwd=stage, check=True)
        root_before = service_state(root_command, root_name, originals[unit_path] is not None)
        session_before = service_state(environment, session_name, originals[user_unit_path] is not None)
        if any(state['enabled'] in ('masked', 'masked-runtime') for state in (root_before, session_before)):
            raise RuntimeError('Maskelenmiş servis kurulmadan önce kullanıcı tarafından açılmalı.')
        if any(state['enabled'] in ('indirect', 'linked', 'linked-runtime') for state in (root_before, session_before)):
            raise RuntimeError('Bağlantılı/dolaylı servis düzeni bu kurucuda desteklenmiyor; ayarlar değiştirilmedi.')
        transaction_started = True
        if session_before['active']:
            session_touched = True
            subprocess.run(environment + ['stop', session_name], check=True)
        if root_before['active']:
            root_touched = True
            systemctl('stop', root_name)
        # Load the rebuilt bridge only after the service releases fan control.
        if Path('/sys/module/r9t_fan').exists():
            subprocess.run(['/usr/bin/rmmod', 'r9t_fan'], check=True)
        subprocess.run(['/usr/bin/modprobe', 'r9t_fan'], check=True)
        if backup.exists():
            shutil.rmtree(backup)
        if target.exists():
            target.rename(backup)
            changed = True
        stage.rename(target)
        changed = True
        modified.add(unit_path)
        write_managed_file(unit_path, unit)
        systemctl('daemon-reload')
        root_touched = True
        systemctl('enable', 'slayer-r9t.service')
        systemctl('start', 'slayer-r9t.service')
        time.sleep(1)
        systemctl('is-active', '--quiet', 'slayer-r9t.service')
        modified.add(desktop_path)
        write_managed_file(desktop_path, '''[Desktop Entry]
Type=Application
Name=Slayer R9T Kontrol Merkezi
Comment=GameGaraj güç, RGB ve donanım izleme
Exec=/usr/bin/python /usr/local/lib/slayer-r9t/r9t-control-center.py
Icon=preferences-system
Terminal=false
Categories=Settings;HardwareSettings;
StartupNotify=true
''')
        modified.add(old)
        old.unlink(missing_ok=True)
        modified.add(user_unit_path)
        write_managed_file(user_unit_path, '''[Unit]
Description=Slayer R9T KDE session controls
After=graphical-session.target
PartOf=graphical-session.target

[Service]
ExecStart=/usr/bin/python /usr/local/lib/slayer-r9t/r9t-session.py
Restart=on-failure
RestartSec=5
TimeoutStopSec=10

[Install]
WantedBy=graphical-session.target
''')
        subprocess.run(environment+['daemon-reload'],check=True)
        session_touched = True
        subprocess.run(environment+['enable','slayer-r9t-session.service'],check=True)
        subprocess.run(environment+['restart','slayer-r9t-session.service'],check=True)
        import runpy
        version=runpy.run_path(str(target/'slayer_r9t/__init__.py'))['__version__']
        print(f'Kontrol Merkezi {version} ve kullanıcı oturumu servisi kuruldu. Önceki sürüm /usr/local/lib/.slayer-r9t-previous altında korunuyor.')
    except Exception as error:
        failures = []
        def attempt(label, operation):
            try:
                operation()
            except Exception as failure:
                failures.append(f'{label}: {failure}')

        if transaction_started:
            # Stop/disable while the new units still exist so enable symlinks can
            # be removed on first-install failure. Continue after rollback errors.
            for command, name, touched in ((environment, session_name, session_touched),
                                          (root_command, root_name, root_touched)):
                if touched:
                    attempt(name + ' stop', lambda c=command, n=name: subprocess.run(c + ['stop', n], check=True))
                    attempt(name + ' disable', lambda c=command, n=name: subprocess.run(c + ['disable', n], check=True))
            if changed:
                def restore_application():
                    if target.exists():
                        shutil.rmtree(target)
                    if backup.exists():
                        backup.rename(target)
                attempt('uygulama dosyaları', restore_application)
            for path in modified:
                attempt(str(path), lambda p=path: restore_file(p, originals[p]))
            for command, name, before in ((root_command, root_name, root_before),
                                           (environment, session_name, session_before)):
                attempt(name + ' daemon-reload', lambda c=command: subprocess.run(c + ['daemon-reload'], check=True))
                if before['enabled'] in ('enabled', 'enabled-runtime'):
                    args = ['enable'] + (['--runtime'] if before['enabled'] == 'enabled-runtime' else []) + [name]
                    attempt(name + ' enable', lambda c=command, a=args: subprocess.run(c + a, check=True))
                if before['active']:
                    attempt(name + ' start', lambda c=command, n=name: subprocess.run(c + ['start', n], check=True))
        if failures:
            raise RuntimeError('Kurulum başarısız; geri alma tamamlanamadı: ' + '; '.join(failures)) from error
        raise
    finally:
        if stage.exists():
            shutil.rmtree(stage)



if __name__ == '__main__':
    main()
