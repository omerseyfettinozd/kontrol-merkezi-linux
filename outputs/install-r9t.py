#!/usr/bin/env python3
"""Root-owned deployment with rollback if the new service cannot start."""
import os
from pathlib import Path
import pwd
import shutil
import subprocess
import tempfile
import time
import shlex
import sys


def systemctl(*args):
    subprocess.run(['/usr/bin/systemctl', *args], check=True)


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
    target = Path('/usr/local/lib/slayer-r9t')
    target.parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix='.slayer-r9t-stage-', dir=target.parent))
    backup = target.with_name('.slayer-r9t-previous')
    unit_path = Path('/etc/systemd/system/slayer-r9t.service')
    original_unit = unit_path.read_text() if unit_path.exists() else None
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
    stopped = False
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
        if original_unit is not None:
            systemctl('stop', 'slayer-r9t.service')
            stopped = True
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
        unit_path.write_text(unit)
        systemctl('daemon-reload')
        systemctl('enable', 'slayer-r9t.service')
        systemctl('start', 'slayer-r9t.service')
        time.sleep(1)
        systemctl('is-active', '--quiet', 'slayer-r9t.service')
    except Exception:
        if changed:
            subprocess.run(['/usr/bin/systemctl', 'stop', 'slayer-r9t.service'])
            shutil.rmtree(target, ignore_errors=True)
            if backup.exists():
                backup.rename(target)
            if original_unit is not None:
                unit_path.write_text(original_unit)
            else:
                unit_path.unlink(missing_ok=True)
            systemctl('daemon-reload')
            if original_unit is not None:
                systemctl('start', 'slayer-r9t.service')
        elif stopped:
            systemctl('start', 'slayer-r9t.service')
        raise
    finally:
        if stage.exists():
            shutil.rmtree(stage)
    Path('/usr/share/applications/slayer-r9t-control-center.desktop').write_text('''[Desktop Entry]
Type=Application
Name=Slayer R9T Kontrol Merkezi
Comment=GameGaraj güç, RGB ve donanım izleme
Exec=/usr/bin/python /usr/local/lib/slayer-r9t/r9t-control-center.py
Icon=preferences-system
Terminal=false
Categories=Settings;HardwareSettings;
StartupNotify=true
''')
    old = Path(user.pw_dir)/'.local/share/applications/slayer-r9t-control-center.desktop'
    old.unlink(missing_ok=True)
    Path('/etc/systemd/user/slayer-r9t-session.service').write_text('''[Unit]
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
    environment=['/usr/bin/runuser','-u',user.pw_name,'--','/usr/bin/env',f'XDG_RUNTIME_DIR=/run/user/{uid}',f'DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/{uid}/bus','/usr/bin/systemctl','--user']
    subprocess.run(environment+['daemon-reload'],check=True)
    subprocess.run(environment+['enable','slayer-r9t-session.service'],check=True)
    subprocess.run(environment+['restart','slayer-r9t-session.service'],check=True)
    import runpy
    version=runpy.run_path(str(target/'slayer_r9t/__init__.py'))['__version__']
    print(f'Kontrol Merkezi {version} ve kullanıcı oturumu servisi kuruldu. Önceki sürüm /usr/local/lib/.slayer-r9t-previous altında korunuyor.')


if __name__ == '__main__':
    main()
