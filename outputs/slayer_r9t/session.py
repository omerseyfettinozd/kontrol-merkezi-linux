"""Same-UID session RPC and opt-in display rules; survives GUI closure."""
import json
import os
from pathlib import Path
import signal
import socket
import struct
import time
import subprocess
import select
import shutil
from .protocol import receive,request
from . import desktop

def socket_path():return str(Path(os.environ.get('XDG_RUNTIME_DIR',f'/run/user/{os.getuid()}'))/'slayer-r9t-session.sock')

def client(payload):
    with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as connection:
        connection.settimeout(30);connection.connect(socket_path())
        connection.sendall((json.dumps(payload)+'\n').encode())
        reply=receive(connection,131072)
    if not reply.get('ok'):raise RuntimeError(reply.get('error','Oturum işlemi başarısız.'))
    return reply['data']

class SessionRules:
    def __init__(self):
        self.saved_modes={};self.low_active=False;self.error='';self.last_check=0;self.ready=False
        self.idle_helper=None;self.idle_seconds=0;self.idle_error=''
        self.last_notice=None
    def lighting(self,seconds):
        if seconds!=self.idle_seconds:
            if self.idle_helper:self.idle_helper.terminate();self.idle_helper.wait(timeout=2);self.idle_helper=None
            request({'op':'lighting_idle','idle':False});self.idle_seconds=seconds
            if seconds:
                helper=Path(__file__).resolve().parent.parent/'r9t-idle'
                if not helper.is_file():raise RuntimeError('KDE boşta kalma bileşeni kurulu değil.')
                self.idle_helper=subprocess.Popen([str(helper),str(seconds)],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
                os.set_blocking(self.idle_helper.stdout.fileno(),False)
        if self.idle_helper:
            if self.idle_helper.poll() is not None:raise RuntimeError('KDE boşta kalma bileşeni durdu.')
            if select.select([self.idle_helper.stdout],[],[],0)[0]:
                for line in self.idle_helper.stdout.read().decode().splitlines():request({'op':'lighting_idle','idle':line=='idle'})

    def close(self):
        if self.idle_helper:self.idle_helper.terminate();self.idle_helper.wait(timeout=2)
        request({'op':'lighting_idle','idle':False})
    def tick(self):
        if time.monotonic()-self.last_check<3:return
        self.last_check=time.monotonic()
        try:
            if not self.ready:
                desktop.screens();request({'op':'session_ready'});self.ready=True
            state=request({'op':'status'});automation=state['automation']
            notice=(automation['reason'],automation['error'])
            if self.last_notice is not None and notice!=self.last_notice and notice[0].startswith(('app:','ac →','battery →','startup →','Uygulama kapandı','Otomasyon hatası')):
                if shutil.which('notify-send'):subprocess.Popen(['notify-send','Slayer R9T',(' · '.join(notice))[:600]],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            self.last_notice=notice
            if not automation['session_ready']:request({'op':'session_ready'})
            self.lighting(0 if automation['paused'] else automation['document']['automation']['lighting_timeout'])
            low=automation['document']['automation']['low_hz_on_battery'] and not automation['paused'] and automation['source']=='battery'
            if low and not self.low_active:
                for output in desktop.screens():
                    if not output['name'].startswith('eDP'):continue
                    current=next(m for m in output['modes'] if m['id']==output['currentModeId'])
                    choices=[m for m in output['modes'] if m['size']==current['size']]
                    mode=min(choices,key=lambda m:m['refreshRate'])
                    self.saved_modes[output['id']]=output['currentModeId'];desktop.set_mode(output['id'],mode['id'])
                self.low_active=True
            elif not low and (self.low_active or self.saved_modes):
                for identifier,mode in list(self.saved_modes.items()):
                    try:desktop.set_mode(identifier,mode)
                    except (OSError,RuntimeError,ValueError):continue
                    del self.saved_modes[identifier]
                self.low_active=False
            self.error=''
        except (OSError,RuntimeError,ValueError,KeyError,subprocess.SubprocessError) as exc:self.error=str(exc);self.ready=False

def main():
    if os.getuid()==0:raise RuntimeError('Oturum servisi root olarak çalıştırılmaz.')
    running=True
    def stop(*_):
        nonlocal running
        running=False
    signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop)
    path=socket_path()
    # Do not steal an existing live session socket.
    if Path(path).exists():
        try:client({'op':'status'});return
        except (OSError,RuntimeError):Path(path).unlink()
    rules=SessionRules()
    with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as server:
        server.bind(path);os.chmod(path,0o600);server.listen(8);server.settimeout(.5)
        try:
            while running:
                rules.tick()
                try:connection,_=server.accept()
                except socket.timeout:continue
                with connection:
                    connection.settimeout(2)
                    try:
                        _,uid,_=struct.unpack('3i',connection.getsockopt(socket.SOL_SOCKET,socket.SO_PEERCRED,12))
                        if uid!=os.getuid():raise PermissionError('Oturum kullanıcısı eşleşmedi.')
                        payload=receive(connection,8192)
                        if payload.get('op') not in ('status','keyboard_settings','color_settings'):
                            request({'op':'automation_pause'})
                            if payload.get('op')=='mode':rules.saved_modes.pop(payload.get('output'),None)
                        data=desktop.dispatch(payload)
                        if payload['op']=='status':data['session_error']=rules.error
                        reply={'ok':True,'data':data}
                    except Exception as exc:reply={'ok':False,'error':str(exc)}
                    try:connection.sendall((json.dumps(reply,ensure_ascii=False)+'\n').encode())
                    except OSError:pass
        finally:
            try:rules.close()
            except (OSError,RuntimeError):pass
            Path(path).unlink(missing_ok=True)
