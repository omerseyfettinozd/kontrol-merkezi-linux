"""One GUI per desktop user, including launches from different checkouts."""
import fcntl
from pathlib import Path
import time

from PySide6.QtCore import QObject, QStandardPaths, Signal
from PySide6.QtNetwork import QLocalServer, QLocalSocket


class SingleInstance(QObject):
    activated = Signal(bool)

    def __init__(self, runtime_dir=None, parent=None):
        super().__init__(parent)
        directory = Path(runtime_dir or QStandardPaths.writableLocation(QStandardPaths.RuntimeLocation))
        self.path = str(directory / 'slayer-r9t-control-center.sock')
        self.lock = open(directory / 'slayer-r9t-control-center.lock', 'a+b')
        self.server = None
        self.clients = set()
        self.primary = False
        try:
            fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            self.lock.close()
            self.lock = None
            return
        self.primary = True
        try:
            # Only the lock owner may remove a socket left by a crashed process.
            QLocalServer.removeServer(self.path)
            self.server = QLocalServer(self)
            self.server.setSocketOptions(QLocalServer.SocketOption.UserAccessOption)
            if not self.server.listen(self.path):
                raise RuntimeError(self.server.errorString())
            self.server.newConnection.connect(self._accept)
        except Exception:
            self.close()
            raise

    def activate_existing(self, fan=False):
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            socket = QLocalSocket()
            socket.connectToServer(self.path)
            if socket.waitForConnected(100):
                socket.write(b'fan\n' if fan else b'show\n')
                if socket.waitForBytesWritten(1000):
                    socket.disconnectFromServer()
                    return True
            socket.abort()
            time.sleep(0.05)
        return False

    def _accept(self):
        while self.server.hasPendingConnections():
            socket = self.server.nextPendingConnection()
            self.clients.add(socket)
            socket.readyRead.connect(lambda s=socket: self._read(s))
            socket.disconnected.connect(lambda s=socket: self._discard(s))
            self._read(socket)

    def _read(self, socket):
        if socket.canReadLine():
            command = bytes(socket.readLine(32)).strip()
            if command in (b'show', b'fan'):
                self.activated.emit(command == b'fan')
            socket.disconnectFromServer()

    def _discard(self, socket):
        self.clients.discard(socket)
        socket.deleteLater()

    def close(self):
        for socket in list(self.clients):
            socket.abort()
        if self.server is not None:
            self.server.close()
            self.server = None
        if self.lock is not None:
            # Keep the inode: unlinking a lock allows racing launches to bypass it.
            self.lock.close()
            self.lock = None
        self.primary = False
