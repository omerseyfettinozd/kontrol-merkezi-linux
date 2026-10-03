"""Sleep / battery-drain diagnostics panel (read-only)."""
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton

from . import sleepdiag
from .widgets import Card


def _fmt(value, fmt='{}', unit=''):
    return '' if value is None else fmt.format(value) + unit


class SleepDiagPanel(QWidget):
    def __init__(self, parent=None, state_path=None, ps_root=None, power_root=None):
        super().__init__(parent)
        self.state_path = state_path or sleepdiag.DEFAULT_PATH
        self.ps_root = ps_root or sleepdiag.SYSFS_POWER_SUPPLY
        self.power_root = power_root or sleepdiag.SYSFS_POWER
        lay = QVBoxLayout(self)
        card = Card()
        box = QVBoxLayout(card)
        title = QLabel('Uyku ve pil kaybı tanılaması')
        title.setObjectName('section')
        self.warning = QLabel(sleepdiag.WARNING)
        self.warning.setObjectName('hint')
        self.warning.setWordWrap(True)
        self.status = QLabel('')
        self.status.setObjectName('status')
        self.status.setWordWrap(True)
        self.result = QLabel('')
        self.result.setWordWrap(True)
        row = QHBoxLayout()
        self.prepare_button = QPushButton('Uykuya hazırlan')
        self.prepare_button.setObjectName('primary')
        self.compare_button = QPushButton('Karşılaştır')
        self.prepare_button.clicked.connect(self.prepare)
        self.compare_button.clicked.connect(self.compare)
        row.addWidget(self.prepare_button)
        row.addWidget(self.compare_button)
        for w in (title, self.warning, self.status, self.result):
            box.addWidget(w)
        box.addLayout(row)
        lay.addWidget(card)
        self._refresh_state()

    def _refresh_state(self):
        self.compare_button.setEnabled(sleepdiag.load_snapshot(self.state_path) is not None)

    def prepare(self):
        snap = sleepdiag.take_snapshot(self.ps_root, self.power_root)
        try:
            sleepdiag.save_snapshot(snap, self.state_path)
        except OSError as exc:
            self.status.setText(f'Kaydedilemedi: {exc.strerror or exc}')
            return
        self.status.setText('Anlık görüntü alındı. Şimdi cihazı kendiniz uyutup uyandırın.')
        self.result.setText('')
        self._refresh_state()

    def compare(self):
        before = sleepdiag.load_snapshot(self.state_path)
        if before is None:
            self.status.setText('Önce “Uykuya hazırlan” kullanın.')
            return
        res = sleepdiag.compare(before, sleepdiag.take_snapshot(self.ps_root, self.power_root))
        lines = [
            'Süre (duvar saati): ' + _fmt(res['duration_s'] and round(res['duration_s'] / 60, 1), '{}', ' dk'),
            'Enerji değişimi: ' + _fmt(res['energy_delta_uwh'] is not None and round(res['energy_delta_uwh'] / 1000, 1) or res['energy_delta_uwh'], '{}', ' mWh'),
            'Kayıp/saat: ' + _fmt(res['loss_uwh_per_h'] is not None and round(res['loss_uwh_per_h'] / 1000, 1) or res['loss_uwh_per_h'], '{}', ' mWh/sa'),
            'Uyku sayacı artışı: ' + _fmt(res['suspend_delta']),
            'Başarısız deneme artışı: ' + _fmt(res['fail_delta']),
            'Sonuç: ' + ('geçerli' if res['valid'] else 'GEÇERSİZ'),
        ] + res['warnings']
        self.result.setText('\n'.join(lines))
        self.status.setText('')
