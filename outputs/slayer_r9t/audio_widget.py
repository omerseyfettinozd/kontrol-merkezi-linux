"""AudioPanel: user-session audio/microphone controls (wpctl backend)."""
from PySide6.QtCore import Qt
from PySide6.QtWidgets import (QWidget, QLabel, QVBoxLayout, QHBoxLayout, QSlider,
                               QPushButton, QComboBox)
from .widgets import Card
from .audio import AudioBackend, AudioError, MAX_PERCENT


class AudioPanel(QWidget):
    def __init__(self, backend=None, parent=None):
        super().__init__(parent)
        self.backend = backend or AudioBackend()
        self._busy = False
        self.rows = {}
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        card = Card()
        cl = QVBoxLayout(card)
        cl.setContentsMargins(20, 18, 20, 18)
        cl.setSpacing(10)
        t = QLabel('Ses ve mikrofon'); t.setObjectName('section'); cl.addWidget(t)
        for kind, title, mute_text in (('sink', 'Çıkış', 'Sessize al'), ('source', 'Mikrofon', 'Mikrofonu sustur')):
            lab = QLabel(title); lab.setObjectName('metricTitle'); cl.addWidget(lab)
            combo = QComboBox()
            slider = QSlider(Qt.Orientation.Horizontal); slider.setRange(0, MAX_PERCENT)
            value = QLabel('–'); value.setObjectName('metricDetail')
            mute = QPushButton(mute_text); mute.setCheckable(True)
            row = QHBoxLayout(); row.addWidget(slider, 1); row.addWidget(value); row.addWidget(mute)
            cl.addWidget(combo); cl.addLayout(row)
            self.rows[kind] = dict(combo=combo, slider=slider, value=value, mute=mute)
            slider.valueChanged.connect(lambda v, k=kind: self.rows[k]['value'].setText(f'%{v}'))
            slider.sliderReleased.connect(lambda k=kind: self.apply_volume(k))
            slider.actionTriggered.connect(lambda _a, k=kind: self._key(k))
            mute.clicked.connect(lambda checked, k=kind: self.apply_mute(k, checked))
            combo.activated.connect(lambda _i, k=kind: self.apply_default(k))
        hint = QLabel('Mikrofon susturma yazılımsaldır (PipeWire); donanım bağlantısını kesmez. '
                      'Ses düzeyi 0–150% arasındadır; her değişiklik wpctl ile geri okunarak doğrulanır.')
        hint.setObjectName('hint'); hint.setWordWrap(True); cl.addWidget(hint)
        self.status_label = QLabel(''); self.status_label.setObjectName('status')
        self.status_label.setWordWrap(True); cl.addWidget(self.status_label)
        refresh = QPushButton('Yenile'); refresh.clicked.connect(lambda: self.refresh()); cl.addWidget(refresh)
        self.refresh_button = refresh
        lay.addWidget(card)
        self.refresh()

    def _controls(self):
        for r in self.rows.values():
            yield from (r['combo'], r['slider'], r['mute'])
        yield self.refresh_button

    def _set_enabled(self, on):
        for w in self._controls():
            w.setEnabled(on)

    def _key(self, kind):
        # keyboard/wheel changes: apply after the slider value has updated
        from PySide6.QtCore import QTimer
        if not self.rows[kind]['slider'].isSliderDown():
            QTimer.singleShot(0, lambda: self.apply_volume(kind))

    def refresh(self, keep_message=False):
        ok, reason = self.backend.availability()
        if not ok:
            self._set_enabled(False)
            self.refresh_button.setEnabled(True)
            self.status_label.setText(reason)
            return
        try:
            state = self.backend.status()
            self._busy = True
            for kind, devs in (('sink', state.sinks), ('source', state.sources)):
                r = self.rows[kind]
                r['combo'].clear()
                for d in devs:
                    r['combo'].addItem(d.name, d.id)
                    if d.default:
                        r['combo'].setCurrentIndex(r['combo'].count() - 1)
                cur = state.default(kind)
                if cur is not None:
                    pct, muted = self.backend.get(kind)
                    r['slider'].setValue(pct); r['mute'].setChecked(muted)
                    r['value'].setText(f'%{pct}')
            self._busy = False
            self._set_enabled(True)
            if not keep_message:
                self.status_label.setText('')
        except AudioError as e:
            self._busy = False
            self._set_enabled(False)
            self.refresh_button.setEnabled(True)
            self.status_label.setText(str(e))

    def _do(self, fn, ok_text):
        if self._busy:
            return
        try:
            fn()
            self.status_label.setText(ok_text)
        except AudioError as e:
            self.status_label.setText(str(e))
        self.refresh(keep_message=True)

    def apply_volume(self, kind):
        v = self.rows[kind]['slider'].value()
        self._do(lambda: self.backend.set_volume(kind, v), f'Ses düzeyi %{v} olarak doğrulandı')

    def apply_mute(self, kind, muted):
        self._do(lambda: self.backend.set_mute(kind, muted), 'Susturma durumu doğrulandı')

    def apply_default(self, kind):
        dev = self.rows[kind]['combo'].currentData()
        self._do(lambda: self.backend.set_default(kind, dev), 'Varsayılan aygıt doğrulandı')
