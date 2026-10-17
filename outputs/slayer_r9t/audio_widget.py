"""AudioPanel: user-session audio/microphone controls (wpctl backend)."""
from PySide6.QtCore import Qt, QObject, QRunnable, QThreadPool, Signal, Slot
from PySide6.QtWidgets import (QWidget, QLabel, QVBoxLayout, QHBoxLayout, QSlider,
                               QPushButton, QComboBox)
from .widgets import Card
from .audio import AudioBackend, AudioError, MAX_PERCENT


class _AudioSignals(QObject):
    completed = Signal(object)


class _AudioTask(QRunnable):
    def __init__(self, backend, operation, message):
        super().__init__()
        self.backend, self.operation, self.message = backend, operation, message
        self.signals = _AudioSignals()

    def run(self):
        message = self.message
        try:
            if self.operation is not None:
                name, args = self.operation
                try:
                    getattr(self.backend, name)(*args)
                except Exception as exc:
                    message = str(exc)
            available, reason = self.backend.availability()
            if not available:
                self.signals.completed.emit((None, {}, reason))
                return
            state = self.backend.status()
            readings = {kind: self.backend.get(kind) for kind in ('sink', 'source')
                        if state.default(kind) is not None}
            self.signals.completed.emit((state, readings, message))
        except Exception as exc:
            self.signals.completed.emit((None, {}, str(exc)))


class AudioPanel(QWidget):
    def __init__(self, backend=None, parent=None):
        super().__init__(parent)
        self.backend = backend or AudioBackend()
        self._busy = False
        self._closed = False
        self._worker = None
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
        message = self.status_label.text() if keep_message else ''
        self._start(None, message)

    def _start(self, operation, message):
        if self._busy or self._closed:
            return
        self._busy = True
        self._set_enabled(False)
        self.status_label.setText('Ses ayarları okunuyor…' if operation is None else 'Ses ayarı uygulanıyor ve doğrulanıyor…')
        task = _AudioTask(self.backend, operation, message)
        self._worker = task
        task.signals.completed.connect(self._completed, Qt.ConnectionType.QueuedConnection)
        QThreadPool.globalInstance().start(task)

    @Slot(object)
    def _completed(self, result):
        self._worker = None
        if self._closed:
            return
        state, readings, message = result
        if state is None:
            self._set_enabled(False)
            self.refresh_button.setEnabled(True)
        else:
            for kind, devs in (('sink', state.sinks), ('source', state.sources)):
                r = self.rows[kind]
                r['combo'].clear()
                for d in devs:
                    r['combo'].addItem(d.name, d.id)
                    if d.default:
                        r['combo'].setCurrentIndex(r['combo'].count() - 1)
                if kind in readings:
                    pct, muted = readings[kind]
                    r['slider'].setValue(pct)
                    r['mute'].setChecked(muted)
                    r['value'].setText(f'%{pct}')
            self._set_enabled(True)
        self.status_label.setText(message)
        self._busy = False

    def _do(self, name, args, ok_text):
        self._start((name, args), ok_text)

    def apply_volume(self, kind):
        v = self.rows[kind]['slider'].value()
        self._do('set_volume', (kind, v), f'Ses düzeyi %{v} olarak doğrulandı')

    def apply_mute(self, kind, muted):
        self._do('set_mute', (kind, muted), 'Susturma durumu doğrulandı')

    def apply_default(self, kind):
        dev = self.rows[kind]['combo'].currentData()
        self._do('set_default', (kind, dev), 'Varsayılan aygıt doğrulandı')

    def shutdown(self):
        self._closed = True
        self._set_enabled(False)

    def closeEvent(self, event):
        self.shutdown()
        super().closeEvent(event)
