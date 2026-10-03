"""User-scoped privileged service; serialized hardware changes and audit history."""
from collections import deque
from datetime import datetime, timezone
import json
import logging
from logging.handlers import RotatingFileHandler
import os
from pathlib import Path
import signal
import socket
import struct
import sys
import time
from . import __version__
from .hardware import Platform, notify
from .protocol import SOCKET, VERSION, REQUEST_LIMIT, receive, validate


class Service:
    def __init__(self, hardware):
        self.hardware = hardware
        from .automation import Automation
        self.automation=Automation(hardware,int(os.environ.get('R9T_UID',os.getuid())))
        self.events = deque(maxlen=100)
        self.audit = logging.getLogger('slayer.audit')
        directory = Path('/var/lib/slayer-r9t')
        if directory.is_dir():
            history = directory/'actions.jsonl'
            try:
                if history.exists() and history.stat().st_size <= 300000:
                    for line in history.read_text().splitlines()[-100:]:
                        try:
                            event = json.loads(line)
                            if isinstance(event, dict) and set(event)=={'time','id','operation','ok','duration_ms','message'}:
                                self.events.appendleft(event)
                        except ValueError:
                            pass
            except OSError:
                logging.exception('Önceki işlem günlüğü okunamadı')
            handler = RotatingFileHandler(directory/'actions.jsonl', maxBytes=256*1024, backupCount=2)
            handler.setFormatter(logging.Formatter('%(message)s'))
            self.audit.addHandler(handler)
            self.audit.setLevel(logging.INFO)
            self.audit.propagate = False

    def dispatch(self, req):
        validate(req)
        op = req['op']
        if op == 'status':
            return dict(self.hardware.status(), version=__version__,automation=self.automation.status())
        if op == 'events':
            return {'events': list(self.events)}
        started = time.monotonic()
        try:
            if op=='configure':result=self.automation.configure(req['document'])
            elif op=='automation_resume':
                controller=getattr(self.hardware,'temperature_target',None)
                if controller is not None and controller.saved is not None:controller.stop()
                self.automation.resume();result={'verified':True,'message':'Otomasyon yeniden etkinleştirildi.'}
            elif op=='automation_pause':
                self.automation.pause();result={'verified':True,'message':'Elle seçim; otomasyon duraklatıldı.'}
            elif op=='session_ready':
                self.automation.session=True;result={'verified':True,'message':'KDE kullanıcı oturumu hazır.'}
            elif op=='lighting_idle':result=self.hardware.dispatch(req)
            else:
                self.hardware.lighting_idle(False)
                self.automation.pause()
                result = self.hardware.dispatch(req)
            result['outcome']='verified' if result.get('verified') is True else 'accepted'
        except Exception as exc:
            self.record(req, started, False, str(exc))
            raise
        self.record(req, started, True, result['message'])
        return result

    def record(self, req, started, ok, message):
        event = {'time': datetime.now(timezone.utc).isoformat(timespec='seconds'),
                 'id': req['id'], 'operation': req['op'], 'ok': ok,
                 'duration_ms': round((time.monotonic()-started)*1000), 'message': message[:600]}
        self.events.appendleft(event)
        self.audit.info(json.dumps(event, ensure_ascii=False))


def main():
    logging.basicConfig(level=logging.INFO, format='%(levelname)s %(message)s')
    hardware = Platform()
    if '--auto-once' in sys.argv:
        hardware.close()
        return
    uid = int(os.environ['R9T_UID'])
    if uid <= 0:
        raise RuntimeError('Geçerli masaüstü kullanıcı kimliği gerekli.')
    service = Service(hardware)
    running = True
    last_automation=None
    last_target=None

    def stop(*_):
        nonlocal running
        running = False

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    Path(SOCKET).parent.mkdir(mode=0o755, parents=True, exist_ok=True)
    Path(SOCKET).unlink(missing_ok=True)
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        server.bind(SOCKET)
        os.chown(SOCKET, uid, -1)
        os.chmod(SOCKET, 0o600)
        server.listen(8)
        server.settimeout(1)
        notify('READY=1')
        while running:
            hardware.monitor()
            service.automation.tick()
            change=(service.automation.reason,service.automation.error)
            if change!=last_automation and any(change):
                service.record({'id':'automation','op':'automation'},time.monotonic(),not bool(change[1]),' · '.join(change))
                last_automation=change
            target=hardware.temperature_target.status()
            change=(target['active'],target['error'])
            if change!=last_target and (change[0] or change[1]):
                service.record({'id':'temperature-target','op':'temperature_target'},time.monotonic(),not bool(change[1]),change[1] or 'Sıcaklık hedefi etkin.')
            last_target=change
            notify('WATCHDOG=1')
            try:
                connection, _ = server.accept()
            except socket.timeout:
                continue
            with connection:
                connection.settimeout(2)
                identifier = None
                try:
                    _, peer_uid, _ = struct.unpack('3i', connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
                    if peer_uid not in (0, uid):
                        raise PermissionError('Bu kullanıcı yetkili değil.')
                    req = receive(connection, REQUEST_LIMIT)
                    identifier = req.get('id')
                    data = service.dispatch(req)
                    reply = {'version': VERSION, 'id': identifier, 'ok': True, 'data': data}
                except Exception as exc:
                    reply = {'version': VERSION, 'id': identifier, 'ok': False, 'error': str(exc)}
                    logging.warning('İşlem reddedildi: %s', exc)
                try:
                    connection.sendall((json.dumps(reply, ensure_ascii=False)+'\n').encode())
                except OSError:
                    pass
    finally:
        server.close()
        Path(SOCKET).unlink(missing_ok=True)
        hardware.close()
