#!/usr/bin/env python3
"""Install the restricted fan bridge through DKMS."""
import os
from pathlib import Path
import shutil
import subprocess


def main():
    if os.geteuid()!=0:raise SystemExit('Root yetkisi gerekli.')
    package='slayer-r9t-fan';version='0.1.0'
    source=Path(__file__).resolve().parent/'fan-driver'
    target=Path('/usr/src')/f'{package}-{version}'
    target.mkdir(exist_ok=True)
    for name in ('r9t_fan.c','Makefile','build.sh','dkms.conf'):
        shutil.copyfile(source/name,target/name)
        os.chown(target/name,0,0);os.chmod(target/name,0o644)
    if not Path('/var/lib/dkms',package,version).exists():
        subprocess.run(['dkms','add','-m',package,'-v',version],check=True)
    current=os.uname().release
    kernels=[current]+sorted(p.name for p in Path('/lib/modules').iterdir() if p.name!=current and (p/'build/Makefile').exists())
    for kernel in kernels:
        try:
            subprocess.run(['dkms','build','-m',package,'-v',version,'-k',kernel,'--force'],check=True)
            subprocess.run(['dkms','install','-m',package,'-v',version,'-k',kernel,'--force'],check=True)
        except subprocess.CalledProcessError:
            if kernel==current:raise
            print(f'{kernel}: fan modülü derlenemedi; bu kernelde kontrol devre dışı kalabilir.')
    subprocess.run(['modprobe','r9t_fan'],check=True)
    print('R9T fan modülü kuruldu.')


if __name__=='__main__':main()
