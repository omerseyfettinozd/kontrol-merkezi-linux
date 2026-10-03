"""Read-only 'Resource-using applications' panel."""
import time
from PySide6.QtCore import QTimer
from PySide6.QtWidgets import QWidget, QLabel, QVBoxLayout, QHBoxLayout, QGridLayout

from .processes import ProcessSampler, format_bytes
from .widgets import Card


class ProcessesPanel(QWidget):
    """Shows top CPU / RAM / GPU processes. Refreshes slowly and only while visible."""

    def __init__(self, sampler=None, interval_ms=5000, parent=None):
        super().__init__(parent)
        self.sampler = sampler or ProcessSampler()
        self.refreshes = 0
        self.last = None
        self.timer = QTimer(self)
        self.timer.setInterval(max(1000, int(interval_ms)))
        self.timer.timeout.connect(self._tick)
        root = QVBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(14)
        head = QLabel('Kaynak kullanan uygulamalar')
        head.setObjectName('section')
        self.status = QLabel('Henüz örnek alınmadı')
        self.status.setObjectName('status')
        self.limits = QLabel('')
        self.limits.setObjectName('hint')
        self.limits.setWordWrap(True)
        root.addWidget(head)
        root.addWidget(self.status)
        row = QHBoxLayout()
        row.setSpacing(14)
        self.lists = {}
        for key, title in (('cpu', 'En çok CPU'), ('memory', 'En çok RAM (RSS)'), ('gpu', 'GPU (VRAM)')):
            card = Card()
            lay = QVBoxLayout(card)
            lay.setContentsMargins(16, 14, 16, 14)
            t = QLabel(title)
            t.setObjectName('metricTitle')
            body = QLabel('—')
            body.setObjectName('hint')
            body.setWordWrap(True)
            lay.addWidget(t)
            lay.addWidget(body)
            lay.addStretch(1)
            row.addWidget(card, 1)
            self.lists[key] = body
        root.addLayout(row)
        root.addWidget(self.limits)
        root.addStretch(1)

    def showEvent(self, event):
        super().showEvent(event)
        self.refresh()
        self.timer.start()

    def hideEvent(self, event):
        self.timer.stop()
        super().hideEvent(event)

    def _tick(self):
        if self.isVisible():
            self.refresh()
        else:
            self.timer.stop()

    def refresh(self):
        self.last = self.sampler.sample()
        self.refreshes += 1
        self.render(self.last)

    def render(self, data):
        if data.get('error'):
            self.status.setText(data['error'])
        else:
            stamp = time.strftime('%H:%M:%S', time.localtime(data['timestamp']))
            iv = data.get('interval_s')
            span = f'{iv:.1f} sn aralık' if iv else 'CPU% için ikinci örnek bekleniyor'
            self.status.setText(f'Örnek: {stamp} · {span} · {data["scanned"]} süreç tarandı')
        self.lists['cpu'].setText('\n'.join(
            f'{r["name"]} — {r["cpu_percent"]:.1f}%' for r in data['cpu']) or 'Henüz CPU% hesaplanmadı')
        self.lists['memory'].setText('\n'.join(
            f'{r["name"]} — {format_bytes(r["rss_bytes"])}' for r in data['memory']) or 'Veri yok')
        gpu = '\n'.join(f'{r["name"]} — {r["vram_mib"]} MiB' for r in data['gpu'])
        note = data.get('gpu_note') or ''
        self.lists['gpu'].setText((gpu + '\n' if gpu else '') + note if (gpu or note) else 'Veri yok')
        self.limits.setText(data.get('limits', ''))
