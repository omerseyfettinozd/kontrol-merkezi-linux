"""Desktop control center with separate drafts, actual state and durable feedback."""
from datetime import datetime
import json
import copy
import platform
import time
from pathlib import Path
import sys
from PySide6.QtCore import QProcess, QTimer, Qt
from PySide6.QtGui import QColor, QFont
from PySide6.QtGui import QAction,QIcon
from PySide6.QtWidgets import (QApplication, QCheckBox, QColorDialog, QComboBox,
    QFileDialog, QGridLayout, QHBoxLayout, QInputDialog, QLabel, QMainWindow,
    QPushButton, QScrollArea, QDialog, QDialogButtonBox, QLineEdit, QSpinBox, QTabWidget, QTextEdit, QVBoxLayout, QWidget,QListWidget,QSystemTrayIcon,QMenu)
from . import __version__
from .settings import Settings, atomic_json, edit_app_rule, move_app_rule, upsert_app_rule
from . import telemetry
from .lighting import pattern, validate_map, ROWS, COLUMNS
from .widgets import Card, MetricCard, STYLE, HistoryPlot
from .history import History,METRICS

CLIENT = Path(__file__).resolve().parent.parent/'r9t-client.py'
DESKTOP_CLIENT=CLIENT.with_name('r9t-desktop-client.py')
EPP_NAMES={'performance':'Performans öncelikli','balance_performance':'Dengeli · performans','balance_power':'Dengeli · tasarruf','power':'Tasarruf öncelikli'}
POWER_NAMES = {'power-saver': 'Güç tasarrufu', 'balanced': 'Dengeli', 'performance': 'Performans'}


class LightingEditor(QDialog):
    """Editable color draft, with no device operations or persistence."""
    def __init__(self, colors, parent=None):
        super().__init__(parent)
        self.original = copy.deepcopy(validate_map(colors))
        self.draft = copy.deepcopy(self.original)
        self.color = QColor('#ffffff')
        self.setWindowTitle('Klavye renk düzeni')
        page = QVBoxLayout(self)
        note = QLabel('Hücreye tıklayarak seçilen rengi boya veya bir bölge seç. Satır ve sütun numaraları fiziksel tuş isimleri değildir. Değişiklikler yalnız taslakta kalır.')
        note.setWordWrap(True);page.addWidget(note)
        self.color_button = QPushButton('Boya rengi: '+self.color.name());self.color_button.clicked.connect(self.pick_color);page.addWidget(self.color_button)
        grid = QGridLayout();grid.setSpacing(3)
        for col in range(COLUMNS):grid.addWidget(QLabel(str(col+1)),0,col+1,alignment=Qt.AlignmentFlag.AlignCenter)
        self.cells = []
        for row in range(ROWS):
            grid.addWidget(QLabel(str(row+1)),row+1,0)
            for col in range(COLUMNS):
                index = row*COLUMNS+col
                button = QPushButton();button.setFixedSize(26,26)
                button.setToolTip(f'Satır {row+1}, sütun {col+1}')
                button.clicked.connect(lambda checked=False,i=index:self.paint_cell(i))
                grid.addWidget(button,row+1,col+1);self.cells.append(button)
        page.addLayout(grid)
        region = QGridLayout();self.region = {}
        for index,(key,title,upper,default) in enumerate([('row_start','İlk satır',ROWS,1),('row_end','Son satır',ROWS,ROWS),('col_start','İlk sütun',COLUMNS,1),('col_end','Son sütun',COLUMNS,COLUMNS)]):
            spin = QSpinBox();spin.setRange(1,upper);spin.setValue(default)
            region.addWidget(QLabel(title),0,index);region.addWidget(spin,1,index);self.region[key] = spin
        page.addLayout(region)
        self.region_button = QPushButton('Bölgeyi boya');self.region_button.clicked.connect(self.paint_region);page.addWidget(self.region_button)
        row = QHBoxLayout();self.presets = QComboBox()
        for title,key in [('Tek renk','uniform'),('Gökkuşağı geçişi','rainbow'),('Üç renk bölgesi','zones'),('Parlaklık geçişi','gradient')]:self.presets.addItem(title,key)
        row.addWidget(self.presets)
        button = QPushButton('Hazır düzeni yükle');button.clicked.connect(self.load_preset);row.addWidget(button);page.addLayout(row)
        row = QHBoxLayout()
        button = QPushButton('Tüm renkleri temizle');button.clicked.connect(self.clear_colors);row.addWidget(button)
        button = QPushButton('Açılıştaki düzene dön');button.clicked.connect(self.reset_colors);row.addWidget(button);page.addLayout(row)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Save|QDialogButtonBox.StandardButton.Cancel)
        buttons.button(QDialogButtonBox.StandardButton.Save).setText('Taslağı kullan')
        buttons.button(QDialogButtonBox.StandardButton.Cancel).setText('İptal')
        buttons.accepted.connect(self.accept);buttons.rejected.connect(self.reject);page.addWidget(buttons)
        self.render_colors()

    def colors(self):
        return copy.deepcopy(validate_map(self.draft))

    def rgb_values(self):
        return [self.color.red(),self.color.green(),self.color.blue()]

    def pick_color(self):
        color = QColorDialog.getColor(self.color,self,'Boya rengi')
        if color.isValid():self.color = color;self.color_button.setText('Boya rengi: '+color.name())

    def render_colors(self):
        for button,color in zip(self.cells,self.draft):
            button.setStyleSheet('QPushButton { background:'+QColor(*color).name()+'; border:1px solid #68768a; border-radius:3px; }')

    def paint_cell(self, index):
        self.draft[index] = self.rgb_values();self.render_colors()

    def paint_region(self):
        r0,r1 = sorted([self.region['row_start'].value()-1,self.region['row_end'].value()-1])
        c0,c1 = sorted([self.region['col_start'].value()-1,self.region['col_end'].value()-1])
        for row in range(r0,r1+1):
            for col in range(c0,c1+1):self.draft[row*COLUMNS+col] = self.rgb_values()
        self.render_colors()

    def load_preset(self):
        key = self.presets.currentData();color = self.rgb_values()
        self.draft = [color.copy() for _ in range(ROWS*COLUMNS)] if key=='uniform' else pattern(key,color)
        self.render_colors()

    def clear_colors(self):
        self.draft = [[0,0,0] for _ in range(ROWS*COLUMNS)];self.render_colors()

    def reset_colors(self):
        self.draft = copy.deepcopy(self.original);self.render_colors()


class ProfileEditor(QDialog):
    """A separate draft: opening or cancelling never changes the main controls."""
    def __init__(self, name, settings, parent=None, copying=False):
        super().__init__(parent)
        from .hardware import validate_profile
        from .fans import PRESETS
        self.original = copy.deepcopy(validate_profile(settings))
        self.setWindowTitle('Profili kopyala' if copying else 'Profili düzenle')
        self.resize(580, 700)
        page = QVBoxLayout(self)
        note = QLabel('İşaretli alanlar profile eklenir. Bu profil etkin otomasyonda kullanılıyorsa kaydedilen değişiklikler otomatik uygulanabilir.')
        note.setWordWrap(True);page.addWidget(note)
        self.name = QLineEdit(name+' kopyası' if copying else name)
        self.name.setReadOnly(not copying)
        page.addWidget(QLabel('Profil adı'))
        page.addWidget(self.name)
        scroll = QScrollArea();scroll.setWidgetResizable(True)
        content = QWidget();box = QVBoxLayout(content);scroll.setWidget(content);page.addWidget(scroll)
        self.includes = {};self.fields = {}
        for key,title,options in [('power_profile','Güç profili',POWER_NAMES),('cpu_epp','CPU enerji tercihi',EPP_NAMES)]:
            combo = QComboBox()
            for value,label in options.items():combo.addItem(label,value)
            if key in settings:combo.setCurrentIndex(combo.findData(settings[key]))
            self.add_field(box,key,title,combo)
        self.boost = QCheckBox('CPU boost açık');self.boost.setChecked(settings.get('boost_enabled',False))
        self.add_field(box,'boost_enabled','CPU boost',self.boost)
        for key,title,upper in [('cpu_max_mhz','CPU üst sınırı',6000),('gpu_max_mhz','GPU üst sınırı',4000)]:
            spin = QSpinBox();spin.setRange(0,upper);spin.setSuffix(' MHz');spin.setSpecialValueText('Otomatik')
            spin.setValue(settings.get(key,0));self.add_field(box,key,title,spin)
        self.light_include = QCheckBox('Klavye aydınlatmasını ekle')
        self.light_include.setChecked('brightness' in settings);box.addWidget(self.light_include)
        self.light_pattern = QComboBox()
        if 'rgb_map' in settings:self.light_pattern.addItem('Kayıtlı renk düzenini koru','stored')
        for title,key in [('Tek renk','uniform'),('Gökkuşağı geçişi','rainbow'),('Üç renk bölgesi','zones'),('Parlaklık geçişi','gradient')]:self.light_pattern.addItem(title,key)
        box.addWidget(self.light_pattern)
        self.color = QColor(*settings.get('rgb',[255,255,255]))
        self.color_button = QPushButton('Renk seç: '+self.color.name());self.color_button.clicked.connect(self.pick_color);box.addWidget(self.color_button)
        self.brightness = QSpinBox();self.brightness.setRange(0,100);self.brightness.setSuffix(' % parlaklık');self.brightness.setValue(settings.get('brightness',100));box.addWidget(self.brightness)
        box.addWidget(QLabel('Kayıtlı renk düzenini koru seçimi, özel renkleri değiştirmeden saklar.'))
        self.light_edit = QPushButton('Renk düzenini düzenle');self.light_edit.clicked.connect(self.edit_lighting);box.addWidget(self.light_edit)
        box.addWidget(QLabel('Fan ayarı'))
        self.fan_mode = QComboBox()
        for title,key in [('Fan ayarı ekleme',None),('EC otomatik','auto'),('Manuel','manual'),('Özel eğri','curve'),('Hazır fan profili','preset'),('Maksimum soğutma','boost')]:self.fan_mode.addItem(title,key)
        fan = settings.get('fan',{})
        self.fan_mode.setCurrentIndex(max(0,self.fan_mode.findData(fan.get('mode'))));box.addWidget(self.fan_mode)
        self.fan_manual = QWidget();row = QHBoxLayout(self.fan_manual)
        self.fan_cpu = QSpinBox();self.fan_gpu = QSpinBox()
        for key,spin in [('cpu',self.fan_cpu),('gpu',self.fan_gpu)]:
            spin.setRange(50,100);spin.setValue(fan.get(key,70));spin.setSuffix(' % '+key.upper());row.addWidget(spin)
        box.addWidget(self.fan_manual)
        self.fan_preset = QComboBox()
        for key,value in PRESETS.items():self.fan_preset.addItem(value['name'],key)
        if fan.get('preset'):self.fan_preset.setCurrentIndex(self.fan_preset.findData(fan['preset']))
        box.addWidget(self.fan_preset)
        self.curve = QWidget();curve_box = QVBoxLayout(self.curve)
        self.point_count = QSpinBox();self.point_count.setRange(2,10);self.point_count.setSuffix(' eğri noktası');curve_box.addWidget(self.point_count)
        points = fan.get('points',[[45,50],[60,60],[75,80],[85,100]])
        self.point_rows = [];self.fan_points = []
        for i in range(10):
            widget = QWidget();row = QHBoxLayout(widget);t = QSpinBox();p = QSpinBox()
            t.setRange(40,95);p.setRange(50,100);t.setSuffix(' °C');p.setSuffix(' % fan')
            t.setValue(points[i][0] if i<len(points) else 85);p.setValue(points[i][1] if i<len(points) else 100)
            row.addWidget(t);row.addWidget(p);curve_box.addWidget(widget)
            self.point_rows.append(widget);self.fan_points.append((t,p))
        self.point_count.setValue(len(points));self.point_count.valueChanged.connect(self.show_points);self.show_points()
        box.addWidget(self.curve)
        self.fan_mode.currentIndexChanged.connect(self.show_fan);self.show_fan()
        self.error = QLabel();self.error.setWordWrap(True);page.addWidget(self.error)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Save|QDialogButtonBox.StandardButton.Cancel)
        buttons.button(QDialogButtonBox.StandardButton.Save).setText('Kaydet')
        buttons.button(QDialogButtonBox.StandardButton.Cancel).setText('İptal')
        buttons.accepted.connect(self.accept);buttons.rejected.connect(self.reject);page.addWidget(buttons)

    def add_field(self, box, key, title, widget):
        include = QCheckBox(title+' ayarını ekle');include.setChecked(key in self.original)
        widget.setEnabled(include.isChecked());include.toggled.connect(widget.setEnabled)
        box.addWidget(include);box.addWidget(widget);self.includes[key] = include;self.fields[key] = widget

    def show_points(self, *_):
        for i,row in enumerate(self.point_rows):row.setVisible(i<self.point_count.value())

    def show_fan(self, *_):
        mode = self.fan_mode.currentData()
        self.fan_manual.setVisible(mode=='manual');self.fan_preset.setVisible(mode=='preset');self.curve.setVisible(mode=='curve')

    def pick_color(self):
        color = QColorDialog.getColor(self.color,self,'Profil klavye rengi')
        if color.isValid():self.color = color;self.color_button.setText('Renk seç: '+color.name())

    def edit_lighting(self):
        key = self.light_pattern.currentData();color = [self.color.red(),self.color.green(),self.color.blue()]
        colors = self.original['rgb_map'] if key=='stored' else [color.copy() for _ in range(ROWS*COLUMNS)] if key=='uniform' else pattern(key,color)
        dialog = LightingEditor(colors,self)
        try:
            if dialog.exec()!=QDialog.DialogCode.Accepted:return
            self.original['rgb_map'] = dialog.colors()
            if self.light_pattern.findData('stored')<0:self.light_pattern.insertItem(0,'Kayıtlı renk düzenini koru','stored')
            self.light_pattern.setCurrentIndex(self.light_pattern.findData('stored'));self.light_include.setChecked(True)
        finally:dialog.deleteLater()

    def profile_settings(self):
        from .hardware import validate_profile
        settings = {}
        for key,include in self.includes.items():
            if not include.isChecked():continue
            widget = self.fields[key]
            settings[key] = widget.currentData() if isinstance(widget,QComboBox) else widget.isChecked() if isinstance(widget,QCheckBox) else widget.value()
        if self.light_include.isChecked():
            key = self.light_pattern.currentData();rgb = [self.color.red(),self.color.green(),self.color.blue()]
            settings['brightness'] = self.brightness.value()
            if key=='stored':settings['rgb_map'] = copy.deepcopy(self.original['rgb_map'])
            elif key=='uniform':settings['rgb'] = rgb
            else:settings['rgb_map'] = pattern(key,rgb)
        mode = self.fan_mode.currentData()
        if mode:
            fan = {'mode':mode}
            if mode=='manual':fan.update(cpu=self.fan_cpu.value(),gpu=self.fan_gpu.value())
            elif mode=='curve':fan['points'] = [[t.value(),p.value()] for t,p in self.fan_points[:self.point_count.value()]]
            elif mode=='preset':fan['preset'] = self.fan_preset.currentData()
            settings['fan'] = fan
        return validate_profile(settings)

    def accept(self):
        try:
            name = self.name.text().strip()
            if not name or len(name)>80:raise ValueError('Profil adı 1–80 karakter olmalı.')
            self.profile_settings()
        except ValueError as exc:self.error.setText(str(exc));return
        super().accept()


class ControlCenter(QMainWindow):
    def __init__(self):
        super().__init__()
        self.setWindowTitle('Slayer R9T Kontrol Merkezi')
        self.resize(1180, 820)
        self.setMinimumSize(850, 620)
        self.setStyleSheet(STYLE)
        self.store = Settings()
        self.state = {}
        self.state_received=0;self.gpu_received=0;self.gpu_values=[None]*3
        self.history=History();self.sensor_cards={};self.epp_initialized=False
        self.events = []
        self.process = None
        self.gpu_process = None
        self.dynamic_boost_process = None
        self.dynamic_boost_state = {}
        self.dynamic_boost_received = 0
        self.gpu_diagnostic_process = None
        self.queue = []
        self.action_busy = False
        self.fan_feedback_pending = False
        self.desktop_process=None;self.desktop_state={};self.desktop_busy=False;self.desktop_initialized=False
        self.cpu_usage = telemetry.CpuUsage()
        self.initialized = False
        self.closing = False
        self.color = QColor('#ffffff')
        self.action_buttons = []
        self.root = QWidget()
        self.root.setObjectName('root')
        self.setCentralWidget(self.root)
        page = QVBoxLayout(self.root)
        page.setContentsMargins(24, 20, 24, 20)
        header = QHBoxLayout()
        heading = QVBoxLayout()
        heading.addWidget(self.label('GAME GARAJ  /  SLAYER R9T', 'eyebrow'))
        heading.addWidget(self.label('Kontrol Merkezi', 'title'))
        heading.addWidget(self.label(f'Sürüm {__version__} · Linux donanım yönetimi', 'subtitle'))
        header.addLayout(heading)
        header.addStretch()
        refresh = QPushButton('Şimdi yenile')
        refresh.clicked.connect(self.poll)
        header.addWidget(refresh)
        page.addLayout(header)
        self.connection_status = self.label('Servise bağlanılıyor…', 'hint')
        self.operation_status = self.label(self.store.warning or 'Hazır · Uygulama düğmeleri yalnızca seçilen ayarı değiştirir.', 'hint')
        page.addWidget(self.connection_status)
        page.addWidget(self.operation_status)
        self.tabs = QTabWidget()
        page.addWidget(self.tabs)
        self.build_overview()
        self.build_power()
        self.build_fans()
        self.build_cooling()
        self.build_dynamic_boost()
        self.build_gpu_diagnostics()
        self.build_keyboard()
        self.build_profiles()
        self.build_battery()
        self.build_display()
        self.build_automation()
        self.build_devices()
        self.build_diagnostics()
        self.build_history()
        self.build_sensors()
        self.build_tray()
        self.tabs.currentChanged.connect(lambda _: self.poll())
        self.timer = QTimer(self)
        self.timer.timeout.connect(self.poll)
        self.timer.start(3000)
        self.poll()

    def build_dynamic_boost(self):
        page=self.tab('Dynamic Boost')
        self.dynamic_boost_index=self.tabs.count()-1
        box=self.card(page,'Destek ve servis sağlığı')
        self.dynamic_support=self.label('Firmware desteği okunuyor…');self.dynamic_support.setTextFormat(Qt.TextFormat.PlainText)
        self.dynamic_daemon=self.label('nvidia-powerd durumu okunuyor…');self.dynamic_daemon.setTextFormat(Qt.TextFormat.PlainText)
        box.addWidget(self.dynamic_support);box.addWidget(self.dynamic_daemon)
        self.dynamic_behavior=self.label('Yük altında güç aktarımı doğrulanmadı.')
        box.addWidget(self.dynamic_behavior)
        self.button(box,'Dynamic Boost durumunu yenile',self.poll_dynamic_boost)
        box=self.card(page,'CPU paketi ve NVIDIA güç ölçümleri')
        grid=QGridLayout();self.dynamic_power_cards={}
        for i,(key,title,color) in enumerate([('cpu_power','CPU paket gücü','#50c9ba'),('gpu_power','NVIDIA GPU tüketimi','#a596dd')]):
            metric=MetricCard(title,'Ölçülen tüketim',color,'W','power',150)
            self.dynamic_power_cards[key]=metric;grid.addWidget(metric,0,i)
        box.addLayout(grid)
        self.dynamic_measurements=self.label('Güncel güç ölçümleri bekleniyor…')
        box.addWidget(self.dynamic_measurements)
        box.addWidget(self.label('CPU değeri paket enerji sayacının iki okumasından hesaplanan ortalamadır; NVIDIA değeri sürücü ölçümüdür. Örnekler tam eşzamanlı değildir. Paket + GPU toplamı tüm sistem tüketimi değildir; GPU güç tavanı tüketim olarak gösterilmez.'))
        box=self.card(page,'Son 5 dakika · aynı iş yükündeki güç değişimi')
        self.dynamic_plots={};row=QHBoxLayout()
        for key in ('cpu_power','gpu_power'):
            plot=HistoryPlot();plot.setMinimumHeight(200);self.dynamic_plots[key]=plot;row.addWidget(plot)
        box.addLayout(row)
        box.addWidget(self.label('Geçmiş GUI açıkken toplanır; Ölçüm geçmişi sekmesinden CSV alınabilir. Kendi oyununu veya iş yükünü çalıştırıp değişimi izleyebilirsin. Yüksek GPU kullanımı veya değişen watt, tek başına Dynamic Boost etkinliği kanıtı değildir. Bu sayfa güç ayarı veya servis durumu değiştirmez.'))
        page.addStretch()

    def poll_dynamic_boost(self):
        if self.dynamic_boost_process:return
        process=QProcess(self);self.dynamic_boost_process=process
        process.setProgram(sys.executable);process.setArguments([str(CLIENT.with_name('r9t-dynamic-boost.py'))])
        process.finished.connect(lambda code,_:self.dynamic_boost_done(process,code))
        process.errorOccurred.connect(lambda _:self.dynamic_boost_done(process,-1) if process.state()==QProcess.ProcessState.NotRunning else None)
        process.start()
        QTimer.singleShot(4000,lambda:process.kill() if self.dynamic_boost_process is process else None)

    def dynamic_boost_done(self,process,code):
        if self.dynamic_boost_process is not process or self.closing:return
        self.dynamic_boost_process=None
        try:
            if code:raise RuntimeError('Dynamic Boost bilgisi okunamadı veya zaman aşımına uğradı.')
            self.dynamic_boost_state=json.loads(bytes(process.readAllStandardOutput()).decode())
            self.dynamic_boost_received=time.monotonic()
            self.render_dynamic_boost_status()
        except (ValueError,RuntimeError,KeyError,TypeError) as exc:
            self.dynamic_boost_state={};self.dynamic_boost_received=0
            self.dynamic_support.setText(str(exc));self.dynamic_daemon.setText('Servis durumu bilinmiyor.')
        finally:process.deleteLater()

    def render_dynamic_boost_status(self):
        state=self.dynamic_boost_state
        if not state or time.monotonic()-self.dynamic_boost_received>6:
            self.dynamic_support.setText('Firmware destek örneği eski veya henüz yok.')
            self.dynamic_daemon.setText('Güncel servis bilgisi yok.');return
        names={True:'Destekleniyor',False:'Desteklenmiyor',None:'Bilinmiyor'}
        self.dynamic_support.setText('Firmware: '+' · '.join(d['pci']+' '+names[d['supported']] for d in state['devices']) if state['devices'] else 'NVIDIA firmware destek bilgisi bulunamadı.')
        daemon=state['daemon'];props=daemon['properties']
        if daemon['error']:text='nvidia-powerd: Bilinmiyor · '+daemon['error']
        else:text=f"nvidia-powerd: {'Çalışıyor' if daemon['healthy'] else 'Hazır değil'} · {props['ActiveState']} / {props['SubState']} · Sonuç {props['Result']} · Çıkış {props['ExecMainStatus']} · Yeniden başlatma {props['NRestarts']} · Açılış {props['UnitFileState']}"
        self.dynamic_daemon.setText(text)
        self.dynamic_behavior.setText('Destek ve servis durumu ayrı ölçüldü; yük altında güç aktarımı henüz doğrulanmadı.')

    def cpu_power_sample(self,now):
        import math
        sample=self.state.get('cooling',{}).get('cpu_power',{})
        stamp=sample.get('sample_monotonic');value=sample.get('watts')
        if now-self.state_received>5 or type(stamp) not in (float,int) or not 0<=now-stamp<=5:return None
        return value if type(value) in (float,int) and math.isfinite(value) and value>=0 else None

    def render_dynamic_boost_measurements(self):
        now=time.monotonic();cpu=self.cpu_power_sample(now)
        gpu=self.gpu_values[2] if now-self.gpu_received<=5 else None
        self.dynamic_power_cards['cpu_power'].set_value(cpu);self.dynamic_power_cards['gpu_power'].set_value(gpu)
        limits=self.state.get('cooling',{}) if now-self.state_received<=5 else {}
        limit=limits.get('gpu',{}).get('enforced_power_w')
        source=limits.get('cpu_power',{})
        parts=['GPU uygulanan güç tavanı: '+(f'{limit:.1f} W' if limit is not None else 'Veri yok')]
        if source.get('interval_seconds') is not None and cpu is not None:parts.append(f"CPU örnek ortalaması: {source['interval_seconds']:.1f} saniye")
        if cpu is None:parts.append('CPU: '+(source.get('error') or 'Güncel paket güç ölçümü yok.'))
        if now-self.gpu_received<=5 and self.gpu_values[1] is not None:parts.append(f'GPU kullanımı: %{self.gpu_values[1]:.0f}')
        if cpu is not None and gpu is not None:parts.append(f'CPU paketi + NVIDIA ölçümleri: {cpu+gpu:.1f} W (tüm sistem tüketimi değildir)')
        if limits.get('temperature_target',{}).get('active'):parts.append('Sıcaklık hedefi de frekansları yönetiyor; ölçüm buna bağlı değişebilir.')
        self.dynamic_measurements.setText(' · '.join(parts))
        rows=self.history.window(300,now)
        for key,plot in self.dynamic_plots.items():plot.set_series(rows,key,300,now)
        self.render_dynamic_boost_status()

    def build_gpu_diagnostics(self):
        page = self.tab('GPU neden açık?')
        self.gpu_diagnostic_index = self.tabs.count()-1
        box = self.card(page, 'Ekran, güç durumu ve aygıtı açık tutan süreçler')
        box.addWidget(self.label('Bu sekme açıkken uygulamanın düzenli NVIDIA sıcaklık/güç ve servis durum sorguları durur. Sistem ayarları değiştirilmez; servis ve etkin sıcaklık hedefi çalışmayı sürdürür. Başka uygulamalar GPU’yu sorgulayabilir.'))
        self.gpu_diagnostic_summary = self.label('Salt okunur tanılama bekleniyor…')
        self.gpu_diagnostic_summary.setTextFormat(Qt.TextFormat.PlainText)
        box.addWidget(self.gpu_diagnostic_summary)
        self.gpu_diagnostic_text = QTextEdit()
        self.gpu_diagnostic_text.setReadOnly(True); self.gpu_diagnostic_text.setMinimumHeight(360)
        box.addWidget(self.gpu_diagnostic_text)
        self.button(box, 'GPU tanılamasını yenile', self.poll_gpu_diagnostics)
        box.addWidget(self.label('Süreç listesi açık GPU aygıt dosyalarından çıkarılır; gerçek GPU kullanım yüzdesi değildir. Yetki engelleri ve tarama sınırları gösterilir. Bu görünüm GPU’yu uyutmaz veya MUX değiştirmez.'))
        page.addStretch()

    def poll_gpu_diagnostics(self):
        if self.gpu_diagnostic_process:
            return
        process = QProcess(self)
        self.gpu_diagnostic_process = process
        process.setProgram(sys.executable)
        process.setArguments([str(CLIENT.with_name('r9t-gpu-diagnostics.py'))])
        process.finished.connect(lambda code, _: self.gpu_diagnostics_done(process, code))
        process.errorOccurred.connect(lambda _: self.gpu_diagnostics_done(process, -1) if process.state()==QProcess.ProcessState.NotRunning else None)
        process.start()
        QTimer.singleShot(5000, lambda: process.kill() if self.gpu_diagnostic_process is process else None)

    def gpu_diagnostics_done(self, process, code):
        if self.gpu_diagnostic_process is not process or self.closing:
            return
        self.gpu_diagnostic_process = None
        try:
            if code: raise RuntimeError('GPU tanılama okunamadı veya zaman aşımına uğradı.')
            state = json.loads(bytes(process.readAllStandardOutput()).decode())
            self.render_gpu_diagnostics(state)
        except (ValueError, RuntimeError, KeyError, TypeError) as exc:
            self.gpu_diagnostic_summary.setText(str(exc))
            self.gpu_diagnostic_text.clear()
        finally:
            process.deleteLater()

    def render_gpu_diagnostics(self, state):
        lines = []
        names = {'0x10de':'NVIDIA','0x1002':'AMD','0x8086':'Intel'}
        states={'active':'Etkin','suspended':'Askıda','suspending':'Askıya alınıyor','resuming':'Uyanıyor','on':'Otomatik uyku kapalı','auto':'Otomatik','connected':'Bağlı','disconnected':'Bağlı değil','enabled':'Etkin','disabled':'Kapalı'}
        display=lambda value: states.get(value,value or 'Veri yok')
        for card in state['cards']:
            power = card['power_after']
            lines.append(f"{names.get(card['vendor'],card['vendor'])} · {card['card']} · PCI {card['pci']}")
            lines.append(f"Güç durumu: {display(power['runtime_status'])} · Politika: {display(power['control'])}")
            for key, label in [('runtime_active_time','Etkin süre'),('runtime_suspended_time','Askıda süre')]:
                value=power[key]
                lines.append(label+': '+(f'{value/1000:.1f} saniye' if value is not None else 'Veri yok'))
            for connector in card['connectors']:
                lines.append(f"  Ekran {connector['name']}: {display(connector['status'])} / {display(connector['enabled'])}")
            driver=card['driver_power']
            if driver: lines.append('Runtime D3: '+driver.get('Runtime D3 status','Veri yok')+' · Video belleği: '+driver.get('Video Memory','Veri yok'))
            for sibling in card['siblings']:
                lines.append(f"  Diğer PCI işlevi {sibling['address']}: {display(sibling['runtime_status'])} / {display(sibling['control'])}")
            lines.extend(card['evidence'])
            lines.append(card['observation'])
            lines.append('Bu GPU aygıtını açık tutan görünür süreçler:')
            lines.extend(f"  PID {u['pid']} · {u['name']} · {', '.join(u['nodes'])}" for u in card['users'])
            if not card['users']: lines.append('  Görünür süreç bulunamadı; bu sonuç GPU’nun boşta olduğunu kanıtlamaz.')
            lines.append('')
        if state['shared_users']:
            lines.append('Ortak NVIDIA aygıtları (belirli bir GPU’ya atfedilemez):')
            lines.extend(f"  PID {u['pid']} · {u['name']} · {', '.join(u['nodes'])}" for u in state['shared_users'])
        coverage=state['coverage']
        lines.append(f"Tarama: {coverage.get('permission_denied',0)} yetki engeli, {coverage.get('vanished',0)} kapanan/değişen süreç, {coverage.get('fd_truncated',0)} dosya sınırı.")
        if coverage.get('timed_out') or coverage.get('process_truncated'): lines.append('Tarama sınırlı: süre veya süreç üst sınırına ulaşıldı.')
        summary=['Son okuma (UTC): '+state['time']+' · '+str(coverage.get('permission_denied',0))+' süreçte yetki engeli.']
        for card in state['cards']:
            summary.append(names.get(card['vendor'],card['vendor'])+': '+display(card['power_after']['runtime_status'])+' · '+card['evidence'][0])
        self.gpu_diagnostic_summary.setText('\n'.join(summary) if state['cards'] else 'Desteklenen DRM GPU bulunamadı.')
        self.gpu_diagnostic_text.setPlainText('\n'.join(lines))

    def build_history(self):
        page=self.tab('Ölçüm geçmişi')
        box=self.card(page,'Son 30 dakikanın ölçümleri')
        controls=QHBoxLayout();self.history_metric=QComboBox();self.history_window=QComboBox()
        for key,(title,unit,_,_) in METRICS.items():self.history_metric.addItem(title+' · '+unit,key)
        for title,seconds in [('Son 5 dakika',300),('Son 30 dakika',1800)]:self.history_window.addItem(title,seconds)
        controls.addWidget(self.history_metric);controls.addWidget(self.history_window);box.addLayout(controls)
        self.history_plot=HistoryPlot();box.addWidget(self.history_plot)
        self.history_summary=self.label('Ölçümler toplanıyor…');box.addWidget(self.history_summary)
        self.button(box,'Seçilen zaman aralığını CSV olarak kaydet',self.export_history)
        box.addWidget(self.label('Geçmiş bu arayüz açıkken yaklaşık 3 saniyede bir toplanır; kapatıldığında silinir. Eksik veya eski sensör verisi grafikte boşluk bırakır. CPU frekansı tüm politikaların ortalamasıdır; pil gücü şarj veya deşarj akışıdır. GPU tüketimi ile güç tavanı ayrı değerlerdir.'))
        self.history_metric.currentIndexChanged.connect(self.render_history)
        self.history_window.currentIndexChanged.connect(self.render_history)
        page.addStretch()

    def build_sensors(self):
        page=self.tab('SSD ve RAM')
        self.sensor_grid=QGridLayout();self.sensor_grid.setSpacing(14);page.addLayout(self.sensor_grid)
        self.sensor_details=self.label('Donanım sıcaklıkları okunuyor…');page.addWidget(self.sensor_details)
        page.addWidget(self.label('Değerler Linux hwmon sensörlerinden okunur. SSD iç sensörleri ile bileşik sıcaklık ayrı gösterilir. Bu sayfa SMART sağlık testi veya donanım arıza teşhisi değildir.'))
        page.addStretch()

    def render_sensors(self):
        sensors=telemetry.storage_memory_sensors()
        primary=[v for v in sensors if v['id'].endswith(':temp1_input')]
        for value in primary:
            key=value['id']
            if key not in self.sensor_cards:
                card=MetricCard(value['name'],'Linux donanım sensörü','#dfb477' if value['driver']=='nvme' else '#70a9ec','°C','temperature',110)
                index=len(self.sensor_cards);self.sensor_grid.addWidget(card,index//2,index%2);self.sensor_cards[key]=card
            self.sensor_cards[key].set_value(value['celsius'])
        present={v['id'] for v in primary}
        for key,card in self.sensor_cards.items():
            if key not in present:card.set_value(None)
        self.sensor_details.setText('\n'.join(f"{v['name']}: {v['celsius']:.1f}°C" for v in sensors) or 'Desteklenen SSD / RAM sıcaklık sensörü bulunamadı.')

    def record_history(self,cpu_temp,cpu_usage,battery):
        now=time.monotonic();fresh=now-self.state_received<=5
        fan=(self.state.get('fans',{}).get('measured') or {}) if fresh else {}
        gpu=self.gpu_values if now-self.gpu_received<=5 else [None]*3
        self.history.add({'cpu_temp':cpu_temp,'cpu_usage':cpu_usage,'cpu_mhz':telemetry.cpu_frequency(),
            'cpu_power':self.cpu_power_sample(now),
            'gpu_temp':gpu[0],'gpu_usage':gpu[1],'gpu_power':gpu[2],
            'cpu_rpm':fan.get('cpu_rpm'),'gpu_rpm':fan.get('gpu_rpm'),
            'gpu_mhz':self.state.get('cooling',{}).get('gpu',{}).get('current_mhz') if fresh else None,
            'battery_power':(battery or {}).get('power_w')},now=now)
        self.render_history()
        self.render_dynamic_boost_measurements()

    def render_history(self,*args):
        now=time.monotonic();key=self.history_metric.currentData();seconds=self.history_window.currentData()
        rows=self.history.window(seconds,now);self.history_plot.set_series(rows,key,seconds,now)
        values=[r[key] for r in rows if r[key] is not None]
        self.history_summary.setText(f'{len(values)} geçerli ölçüm · Ortalama {sum(values)/len(values):.1f} · Min {min(values):.1f} · Maks {max(values):.1f} {METRICS[key][1]}' if values else 'Bu aralıkta geçerli ölçüm yok.')

    def export_history(self):
        path,_=QFileDialog.getSaveFileName(self,'Ölçüm geçmişini kaydet','slayer-olcumler.csv','CSV (*.csv)')
        if path:self.guarded(lambda:self.feedback(f'{self.history.export(path,self.history_window.currentData())} ölçüm CSV dosyasına kaydedildi.'))

    def build_tray(self):
        self.tray=QSystemTrayIcon(QIcon.fromTheme('preferences-system'),self)
        self.tray.setToolTip('Slayer R9T Kontrol Merkezi')
        self.tray_menu=QMenu()
        self.tray_menu.addAction('Kontrol merkezini göster',lambda:(self.show(),self.raise_(),self.activateWindow()))
        self.tray_profiles=self.tray_menu.addMenu('Profiller')
        self.tray_menu.addAction('EC otomatik fan',lambda:self.command('auto'))
        self.tray_menu.addAction('Otomasyonu sürdür',lambda:self.command('automation_resume'))
        self.tray_menu.addAction('Arayüzü kapat',self.close)
        self.tray.setContextMenu(self.tray_menu)
        self.tray.activated.connect(lambda _:self.show())
        self.tray.show();self.update_tray_profiles()

    def update_tray_profiles(self):
        if not hasattr(self,'tray_profiles'):return
        self.tray_profiles.clear()
        for name,value in self.store.profiles.items():
            self.tray_profiles.addAction(name,lambda checked=False,v=value:self.command('profile',json.dumps(v)))

    @staticmethod
    def label(text, role='hint'):
        label = QLabel(text)
        label.setObjectName(role)
        label.setWordWrap(True)
        return label

    def tab(self, title):
        root = QWidget()
        root.setObjectName('page')
        layout = QVBoxLayout(root)
        layout.setContentsMargins(0, 4, 0, 0)
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QScrollArea.Shape.NoFrame)
        scroll.setWidget(root)
        self.tabs.addTab(scroll, title)
        return layout

    def card(self, layout, title):
        card = Card()
        box = QVBoxLayout(card)
        box.setContentsMargins(18, 16, 18, 16)
        box.addWidget(self.label(title, 'section'))
        layout.addWidget(card)
        return box

    def button(self, layout, title, callback, capability=None):
        button = QPushButton(title)
        button.clicked.connect(callback)
        layout.addWidget(button)
        if capability:
            self.action_buttons.append((button, capability))
            button.setEnabled(False)
        return button

    def build_overview(self):
        page = self.tab('Genel durum')
        grid = QGridLayout()
        self.metrics = {}
        grid.setHorizontalSpacing(14)
        grid.setVerticalSpacing(14)
        for index, (key, title, subtitle, color, unit, kind, upper) in enumerate([
            ('cpu', 'İşlemci sıcaklığı', 'AMD Ryzen · Tctl', '#50c9ba', '°C', 'temperature', 110),
            ('cpu_util', 'İşlemci kullanımı', 'Tüm çekirdekler', '#70a9ec', '%', 'chip', 100),
            ('gpu', 'GPU sıcaklığı', 'GeForce RTX 5070', '#a596dd', '°C', 'temperature', 110),
            ('gpu_util', 'GPU kullanımı', 'GeForce RTX 5070', '#a596dd', '%', 'chip', 100),
            ('gpu_power', 'GPU güç tüketimi', 'Anlık ölçüm', '#dfb477', 'W', 'power', 150),
            ('battery', 'Pil seviyesi', 'Kalan kapasite', '#50c9ba', '%', 'battery', 100)]):
            card = MetricCard(title, subtitle, color, unit, kind, upper)
            grid.addWidget(card, index//3, index%3)
            self.metrics[key] = card
        page.addLayout(grid)
        self.sensor_summary = self.label('Sensörler okunuyor…')
        page.addWidget(self.sensor_summary)
        box = self.card(page, 'Soğutma desteği')
        box.addWidget(self.label('Ayrı CPU/GPU fan kontrolü ve fan eğrileri Fanlar sekmesinde. Hedefler PWM ve gerçek RPM geri bildirimiyle izlenir.'))
        self.fan_state = self.label('EC ölçümleri bekleniyor…')
        box.addWidget(self.fan_state)
        self.button(box, 'EC otomatik fan komutunu gönder', lambda: self.command('auto'), 'auto_fan')
        box.addWidget(self.label('Fan sürücüsüyle bağlantı kesilirse 15 saniyelik zaman aşımıyla EC otomatiğine dönüş denenir.'))
        page.addStretch()

    def build_power(self):
        page = self.tab('Güç ve işlemci')
        box = self.card(page, 'Linux güç profili')
        self.power_state = self.label('Etkin profil okunuyor…')
        box.addWidget(self.power_state)
        self.power_combo = QComboBox()
        for value, name in POWER_NAMES.items():
            self.power_combo.addItem(name, value)
        box.addWidget(self.power_combo)
        self.button(box, 'Seçilen güç profilini uygula', lambda: self.command('power', self.power_combo.currentData()), 'power')
        box.addWidget(self.label('Bu seçenek Linux güç profili servisinin ayarıdır. OEM fan modu veya GPU güç limiti ayarı içermez.'))
        box = self.card(page, 'CPU boost')
        self.boost_state = self.label('Etkin boost okunuyor…')
        box.addWidget(self.boost_state)
        self.boost_check = QCheckBox('Yüksek frekans boost etkin')
        box.addWidget(self.boost_check)
        self.button(box, 'Boost seçimini uygula', lambda: self.command('boost', '1' if self.boost_check.isChecked() else '0'), 'boost')
        box=self.card(page,'CPU enerji tercihi')
        self.epp_state=self.label('AMD P-State okunuyor…');box.addWidget(self.epp_state)
        self.epp_combo=QComboBox();box.addWidget(self.epp_combo)
        self.button(box,'Enerji tercihini uygula',lambda:self.command('profile',json.dumps({'cpu_epp':self.epp_combo.currentData()})),'cpu_epp')
        box.addWidget(self.label('İşlemcinin performans / enerji tasarrufu tercihini değiştirir; voltaj ayarı değildir. Linux güç profilini değiştirirsen enerji tercihi yeniden değişebilir. Kaydedilmiş birleşik profilde enerji tercihi en son uygulanır.'))
        page.addStretch()

    def build_fans(self):
        page = self.tab('Fanlar')
        grid = QGridLayout()
        grid.setSpacing(14)
        self.fan_metrics = {}
        for index, (key,title,color) in enumerate([('cpu_rpm','CPU fanı','#50c9ba'),('gpu_rpm','GPU fanı','#a596dd')]):
            card=MetricCard(title,'Gerçek dönüş hızı',color,'RPM','chip',6500)
            grid.addWidget(card,0,index)
            self.fan_metrics[key]=card
        page.addLayout(grid)
        self.fan_validation=self.label('Fan sürücüsüne bağlanılıyor…')
        page.addWidget(self.fan_validation)
        self.fan_targets=self.label('Sıcaklıklar ve etkin fan hedefleri bekleniyor…')
        page.addWidget(self.fan_targets)
        box=self.card(page,'Fan modu')
        row=QHBoxLayout()
        self.button(row,'EC otomatik',lambda:self.command('auto'),'auto_fan')
        self.button(row,'Maksimum soğutma',lambda:self.command('fan_boost'),'fan_boost')
        box.addLayout(row)
        self.fan_preset_combo=QComboBox()
        box.addWidget(self.fan_preset_combo)
        self.button(box,'Hazır fan profilini uygula',lambda:self.command('fan_preset',self.fan_preset_combo.currentData()),'curve')
        box=self.card(page,'Ayrı fan hızları')
        row=QHBoxLayout()
        self.fan_cpu=QSpinBox();self.fan_gpu=QSpinBox()
        for spin,label in [(self.fan_cpu,'CPU'),(self.fan_gpu,'GPU')]:
            spin.setRange(50,100);spin.setValue(70);spin.setSuffix(' % '+label);row.addWidget(spin)
        box.addLayout(row)
        self.button(box,'CPU / GPU fan hedeflerini uygula',lambda:self.command('set',str(self.fan_cpu.value()),str(self.fan_gpu.value())),'manual_fan')
        box.addWidget(self.label('Yüzde, fan sürüş hedefidir; RPM doğrusal olmak zorunda değildir. İlk mod geçişinde fanlar kısa süre tam hıza çıkabilir. Koruma nedeniyle %50 altı hedef ve fan kapatma yoktur.'))
        box=self.card(page,'Kendi fan eğrin')
        self.fan_points=[]
        for temperature,duty in [(45,50),(60,60),(75,80),(85,100)]:
            row=QHBoxLayout();t=QSpinBox();p=QSpinBox()
            t.setRange(40,95);t.setValue(temperature);t.setSuffix(' °C')
            p.setRange(50,100);p.setValue(duty);p.setSuffix(' % fan')
            row.addWidget(t);row.addWidget(p);box.addLayout(row);self.fan_points.append((t,p))
        self.button(box,'Eğriyi uygula',lambda:self.command('curve',json.dumps([[t.value(),p.value()] for t,p in self.fan_points])),'curve')
        box.addWidget(self.label('Aynı eğri CPU ve GPU sıcaklıklarına ayrı ayrı uygulanır. Noktalar arasında geçiş hesaplanır. CPU 95°C veya GPU 87°C’ye ulaştığında iki fan %100’e zorlanır. Uykuya geçerken manuel kontrol bırakılır.'))
        box.addWidget(self.label('Eğride ısınmaya tepki hemen verilir. Fan hızı ancak 2°C soğuma ve 6 saniye bekleme sonrasında, her 2 saniyede en fazla 2 yüzde puanı düşer. Sıcaklık koruması bu beklemeyi atlar.'))
        page.addStretch()

    def build_cooling(self):
        page = self.tab('Serin çalışma')
        box = self.card(page, 'Hazır soğutma profilleri')
        box.addWidget(self.label('Güç ve frekans sınırlarını birlikte uygular. Sıcaklık ve FPS etkisi iş yüküne göre değişir; ölçülmüş sıcaklık düşüşü garantisi değildir.'))
        self.thermal_combo = QComboBox()
        self.thermal_description = self.label('Destek bilgisi okunuyor…')
        box.addWidget(self.thermal_combo)
        box.addWidget(self.thermal_description)
        self.thermal_combo.currentIndexChanged.connect(self.preview_thermal)
        self.button(box, 'Seçilen soğutma profilini uygula', lambda: self.command('thermal_preset', self.thermal_combo.currentData()), 'thermal')
        box = self.card(page, 'Sıcaklık hedefi')
        box.addWidget(self.label('Fanı artırır; hedef aşılırsa frekansları kademeli düşürür. Hedef sıcaklık garanti edilmez. Kapatınca önceki ayarlara döner. Elle profil seçimi bu kontrolü durdurur.'))
        row = QHBoxLayout()
        self.target_cpu = QSpinBox(); self.target_cpu.setRange(60,85); self.target_cpu.setValue(75); self.target_cpu.setSuffix(' °C CPU')
        self.target_gpu = QSpinBox(); self.target_gpu.setRange(60,80); self.target_gpu.setValue(72); self.target_gpu.setSuffix(' °C GPU')
        row.addWidget(self.target_cpu); row.addWidget(self.target_gpu); box.addLayout(row)
        self.target_state = self.label('Sıcaklık hedefi kapalı.')
        box.addWidget(self.target_state)
        self.button(box, 'Sıcaklık hedefini başlat', lambda: self.command('temperature_target', str(self.target_cpu.value()), str(self.target_gpu.value())), 'temperature_target')
        self.button(box, 'Durdur ve önceki ayarlara dön', lambda: self.command('temperature_target_stop'))
        box = self.card(page, 'CPU frekans sınırı')
        self.cpu_limit_state = self.label('Sistemden okunuyor…')
        box.addWidget(self.cpu_limit_state)
        self.cpu_max = QSpinBox()
        self.cpu_max.setRange(0, 6000)
        self.cpu_max.setSingleStep(100)
        self.cpu_max.setSpecialValueText('Otomatik üst sınır')
        self.cpu_max.setSuffix(' MHz')
        box.addWidget(self.cpu_max)
        self.button(box, 'CPU sınırını uygula', lambda: self.command('profile', json.dumps({'cpu_max_mhz':self.cpu_max.value()})), 'cpu_limit')
        box.addWidget(self.label('Sadece üst frekans sınırını değiştirir. Boost kapalıysa sürücü boost dışı üst sınıra göre kısıtlayabilir.'))
        box = self.card(page, 'GPU frekans sınırı')
        self.gpu_limit_state = self.label('Sürücüden okunuyor…')
        box.addWidget(self.gpu_limit_state)
        self.gpu_max = QSpinBox()
        self.gpu_max.setRange(0, 4000)
        self.gpu_max.setSingleStep(100)
        self.gpu_max.setSpecialValueText('Otomatik üst sınır')
        self.gpu_max.setSuffix(' MHz')
        box.addWidget(self.gpu_max)
        self.button(box, 'GPU sınırını uygula', lambda: self.command('profile', json.dumps({'gpu_max_mhz':self.gpu_max.value()})), 'gpu_limit')
        box.addWidget(self.label('NVIDIA frekans sınırı komutu gönderilir. Anlık frekans okunabilir; ayarlanan sınırın bağımsız geri okuması yoktur. Otomatik seçimi bu uygulamanın GPU frekans sınırını kaldırır.'))
        box = self.card(page, 'Undervolt desteği')
        box.addWidget(self.label('Doğrudan CPU/GPU voltaj azaltma bu cihazda doğrulanmadı; bu nedenle voltaj ayarı kapalı. Buradaki profiller frekans sınırı ve güç modu kullanır. GPU watt sınırı da mevcut sürücüde desteklenmiyor.'))
        page.addStretch()
        self.cooling_initialized = False

    def preview_thermal(self, *_):
        presets = self.state.get('cooling', {}).get('presets', {})
        preset = presets.get(self.thermal_combo.currentData())
        if preset:
            settings = preset['settings']
            cpu = str(settings['cpu_max_mhz'])+' MHz' if settings['cpu_max_mhz'] else 'Otomatik'
            gpu = str(settings['gpu_max_mhz'])+' MHz' if settings['gpu_max_mhz'] else 'Otomatik'
            self.thermal_description.setText(preset['description']+f'\nCPU: {cpu} · GPU: {gpu} · Boost: '+('Açık' if settings['boost_enabled'] else 'Kapalı'))

    def build_keyboard(self):
        page = self.tab('Klavye')
        box = self.card(page, 'RGB aydınlatma')
        self.rgb_state = self.label('Sürücü ayarları okunuyor…')
        box.addWidget(self.rgb_state)
        self.color_button = QPushButton('Renk seç')
        self.color_button.clicked.connect(self.pick_color)
        box.addWidget(self.color_button)
        self.brightness = QSpinBox()
        self.brightness.setRange(0, 100)
        self.brightness.setSuffix(' % parlaklık')
        box.addWidget(self.brightness)
        self.button(box, 'Klavyeye uygula', lambda: self.command('rgb', json.dumps(self.rgb_values()), str(self.brightness.value())), 'rgb')
        box.addWidget(self.label('Tüm klavyeye tek renk uygulanır. %0 ışığı kapatır. Donanımın parlaklık adımlarına göre gerçek değer yuvarlanabilir; aşağıdaki etkin değer sürücüden okunur.'))
        self.light_pattern=QComboBox()
        for title,key in [('Tek renk','uniform'),('Gökkuşağı geçişi','rainbow'),('Üç renk bölgesi','zones'),('Seçilen renkle parlaklık geçişi','gradient')]:self.light_pattern.addItem(title,key)
        box.addWidget(self.light_pattern)
        self.rgb_map_draft = None
        self.button(box,'Renk düzenini düzenle',self.edit_rgb_map)
        self.button(box,'Seçilen renk düzenini uygula',lambda:self.command('profile',json.dumps(self.lighting_settings())),'rgb_map')
        box.addWidget(self.label('Statik düzenler 6 × 21 sürücü kanalına uygulanır ve tüm kanallar geri okunur. Fiziksel tuş konumları kasa düzenine göre farklı olabilir; bunlar donanım animasyonu değildir. Renk düzeni kayıtlı profile de eklenir.'))
        self.fn_check=QCheckBox('Fn Lock etkin')
        box.addWidget(self.fn_check)
        self.fn_actual=self.label('Fn Lock sistemden okunuyor…');box.addWidget(self.fn_actual)
        self.button(box,'Fn Lock seçimini uygula',lambda:self.command('fn_lock','1' if self.fn_check.isChecked() else '0'),'fn_lock')
        self.button(box,'KDE tuş yeniden atama seçeneklerini aç',lambda:self.desktop_command({'op':'keyboard_settings'}))
        self.remap_combo=QComboBox()
        for title,key in [('Caps Lock normal','default'),('Caps Lock → Escape','caps_escape'),('Caps Lock → Control','caps_ctrl'),('Control / Caps Lock yer değiştir','swap_ctrl_caps')]:self.remap_combo.addItem(title,key)
        box.addWidget(self.remap_combo)
        self.button(box,'Tuş düzenini KDE’ye uygula',lambda:self.desktop_command({'op':'remap','preset':self.remap_combo.currentData()}))
        box.addWidget(self.label('Klavye ışığının boşta kalma süresi ve priz/pil aydınlatması Otomasyon sekmesinden kayıtlı profillerle yönetilir.'))
        page.addStretch()

    def build_profiles(self):
        page = self.tab('Profiller')
        box = self.card(page, 'Kayıtlı donanım profilleri')
        box.addWidget(self.label('Güç, frekans, klavye ve seçersen fan ayarları birlikte kaydedilir. Fan alanı eklenmezse profil fan modunu değiştirmez. Uygulama hatasında önceki ayarlara dönüş denenir.'))
        self.profile_combo = QComboBox()
        box.addWidget(self.profile_combo)
        self.profile_preview = self.label('Henüz profil yok.')
        box.addWidget(self.profile_preview)
        self.profile_fan=QComboBox()
        for title,key in [('Fan ayarını profile ekleme',None),('EC otomatik fan','auto'),('Manuel fan seçimleri','manual'),('Özel eğri seçimleri','curve'),('Seçili hazır fan profili','preset'),('Maksimum soğutma','boost')]:self.profile_fan.addItem(title,key)
        box.addWidget(self.profile_fan)
        self.profile_combo.currentTextChanged.connect(self.preview_profile)
        self.button(box, 'Seçimleri yeni profil olarak kaydet', self.save_profile)
        self.button(box, 'Seçilen profili düzenle', self.edit_profile)
        self.button(box, 'Seçilen profili kopyala', self.copy_profile)
        self.button(box, 'Seçilen profili uygula', self.apply_profile, 'profile')
        self.button(box, 'Seçilen profili sil', self.delete_profile)
        row = QHBoxLayout()
        self.button(row, 'Profilleri içe aktar', self.import_profiles)
        self.button(row, 'Profilleri dışa aktar', self.export_profiles)
        box.addLayout(row)
        if self.store.data.get('legacy_fan_profiles'):
            box.addWidget(self.label('Eski fan profilleriniz ayar dosyasında arşiv olarak korunuyor. Desteklenmeyen fan ayarları yeni profillere uygulanmaz.'))
        self.reload_profiles()
        page.addStretch()

    def build_battery(self):
        page=self.tab('Pil')
        box=self.card(page,'Pil sağlığı ve şarj')
        self.battery_text=self.label('Pil ölçümleri bekleniyor…');box.addWidget(self.battery_text)
        self.charge_reason=self.label('Destek bilgisi bekleniyor…');box.addWidget(self.charge_reason)
        self.charge_end=QSpinBox();self.charge_end.setRange(50,100);self.charge_end.setValue(80);self.charge_end.setSuffix(' % şarj sınırı');box.addWidget(self.charge_end)
        self.button(box,'Şarj sınırını uygula',lambda:self.command('charge_limit',str(self.charge_end.value())),'charge_limit')
        page.addStretch()

    def build_display(self):
        page=self.tab('Ekran')
        box=self.card(page,'Bağlı ekran ve yenileme hızı')
        self.display_combo=QComboBox();self.display_modes=QComboBox();self.display_text=self.label('KDE oturumu bekleniyor…')
        box.addWidget(self.display_text);box.addWidget(self.display_combo);box.addWidget(self.display_modes)
        self.display_combo.currentIndexChanged.connect(self.refresh_modes)
        self.display_apply=QPushButton('Seçili ekran modunu uygula');self.display_apply.clicked.connect(lambda:self.desktop_command({'op':'mode','output':self.display_combo.currentData(),'mode':self.display_modes.currentData()}));box.addWidget(self.display_apply)
        self.display_brightness=QSpinBox();self.display_brightness.setRange(10,100);self.display_brightness.setValue(70);self.display_brightness.setSuffix(' % ekran parlaklığı');box.addWidget(self.display_brightness)
        self.display_brightness_button=QPushButton('Parlaklığı uygula');self.display_brightness_button.clicked.connect(lambda:self.desktop_command({'op':'brightness','output':self.display_combo.currentData(),'value':self.display_brightness.value()}));box.addWidget(self.display_brightness_button)
        self.night_check=QCheckBox('KDE gece rengi etkin');box.addWidget(self.night_check)
        self.night_actual=self.label('Gece rengi sistemden okunuyor…');box.addWidget(self.night_actual)
        self.night_button=QPushButton('Gece rengini uygula');self.night_button.clicked.connect(lambda:self.desktop_command({'op':'nightlight','enabled':self.night_check.isChecked()}));box.addWidget(self.night_button)
        self.button(box,'KDE renk sıcaklığı ve zamanlama ayarlarını aç',lambda:self.desktop_command({'op':'color_settings'}))
        self.button(box,'Seçili ekrana ICC renk profili yükle',self.choose_icc)
        box.addWidget(self.label('Pilde düşük Hz isteğe bağlıdır. Yalnız dahili eDP ekranında aynı çözünürlükteki en düşük mevcut hız seçilir; prizde önceki moda dönülür.'))
        page.addStretch()

    def choose_icc(self):
        path,_=QFileDialog.getOpenFileName(self,'Ekran için ICC renk profili','','Renk profilleri (*.icc *.icm)')
        if path:self.desktop_command({'op':'icc','output':self.display_combo.currentData(),'path':path})

    def refresh_modes(self,*_):
        self.display_modes.clear()
        output=next((s for s in self.desktop_state.get('screens',[]) if s['id']==self.display_combo.currentData()),None)
        if output:
            for mode in output['modes']:self.display_modes.addItem(mode['name'],mode['id'])
            self.display_modes.setCurrentIndex(self.display_modes.findData(output['currentModeId']))

    def build_automation(self):
        page=self.tab('Otomasyon')
        box=self.card(page,'Olaylara göre kayıtlı profil seçimi')
        self.automation_text=self.label('Otomasyon durumu bekleniyor…');box.addWidget(self.automation_text)
        box.addWidget(self.label('Varsayılan kapalıdır. Elle bir ayar uygulamak otomasyonu duraklatır. Uygulama kuralları priz/pil kuralından önce gelir; listede ilk eşleşen uygulama seçilir.'))
        self.rule_combos={}
        for key,title in [('startup','Oturum açılışında'),('ac','Prizde'),('battery','Pilde')]:
            box.addWidget(self.label(title));combo=QComboBox();box.addWidget(combo);self.rule_combos[key]=combo
        self.app_rules=[dict(r) for r in self.store.data['automation']['apps']]
        self.app_list=QListWidget();self.app_list.setMaximumHeight(120)
        self.app_list.setStyleSheet('QListWidget { background:#132033; color:#c8d5e5; border:1px solid #2c4055; border-radius:8px; padding:8px; } QListWidget::item:selected { background:#204a50; }')
        box.addWidget(self.app_list)
        self.button(box,'Uygulama kuralı ekle',self.add_app_rule)
        self.button(box,'Çalışan programdan kural ekle',self.add_running_rule)
        self.button(box,'Seçili uygulama kuralını düzenle',self.edit_selected_app_rule)
        row=QHBoxLayout()
        self.button(row,'Önceliği yükselt',lambda:self.move_selected_app_rule(-1))
        self.button(row,'Önceliği düşür',lambda:self.move_selected_app_rule(1))
        box.addLayout(row)
        self.button(box,'Seçili uygulama kuralını sil',self.remove_app_rule)
        self.low_hz=QCheckBox('Pilde dahili ekranı düşük Hz’ye geçir');self.low_hz.setChecked(self.store.data['automation']['low_hz_on_battery']);box.addWidget(self.low_hz)
        self.light_timeout=QSpinBox();self.light_timeout.setRange(0,3600);self.light_timeout.setSpecialValueText('Klavye ışığı zaman aşımı kapalı');self.light_timeout.setSuffix(' saniye boşta kalınca ışığı kapat');self.light_timeout.setValue(self.store.data['automation']['lighting_timeout']);box.addWidget(self.light_timeout)
        self.button(box,'Profilleri ve kuralları servise kaydet',self.save_automation)
        self.button(box,'Otomasyonu yeniden etkinleştir',lambda:self.command('automation_resume'))
        self.automation_initialized=False;self.refresh_rules();page.addStretch()

    def refresh_rules(self, selected=None):
        if not hasattr(self,'rule_combos'):return
        for key,combo in self.rule_combos.items():
            selected_profile=combo.currentData() if combo.count() else self.store.data['automation'][key]
            combo.clear();combo.addItem('Kapalı',None)
            for name in self.store.profiles:combo.addItem(name,name)
            combo.setCurrentIndex(max(0,combo.findData(selected_profile)))
        if selected is None:selected=self.app_list.currentRow()
        self.app_list.clear()
        for rule in self.app_rules:self.app_list.addItem(Path(rule['executable']).name+' → '+rule['profile']+'\n'+rule['executable'])
        if self.app_rules:self.app_list.setCurrentRow(min(max(0,selected),len(self.app_rules)-1))

    def add_app_rule(self):
        path,_=QFileDialog.getOpenFileName(self,'Çalışan programın gerçek yürütülebilir dosyası')
        if not path:return
        profile,ok=QInputDialog.getItem(self,'Uygulama profili','Kayıtlı profil:',list(self.store.profiles),0,False)
        if ok:
            executable=str(Path(path).resolve())
            self.guarded(lambda:self.upsert_rule(executable,profile))

    def rule_draft(self):
        return dict(self.store.data['automation'],apps=self.app_rules)

    def upsert_rule(self, executable, profile):
        updated = upsert_app_rule(self.rule_draft(),self.store.profiles,executable,profile)
        self.app_rules = updated['apps']
        selected = next(i for i,rule in enumerate(self.app_rules) if rule['executable']==executable)
        self.refresh_rules(selected)

    def edit_selected_app_rule(self):
        index = self.app_list.currentRow()
        if not 0<=index<len(self.app_rules):return
        rule = self.app_rules[index]
        executable,ok = QInputDialog.getText(self,'Uygulama kuralı','Gerçek yürütülebilir dosyanın tam yolu:',text=rule['executable'])
        if not ok:return
        names = list(self.store.profiles)
        profile,ok = QInputDialog.getItem(self,'Uygulama profili','Kayıtlı profil:',names,names.index(rule['profile']) if rule['profile'] in names else 0,False)
        if not ok:return
        def edit():
            updated = edit_app_rule(self.rule_draft(),self.store.profiles,index,executable,profile)
            self.app_rules = updated['apps'];self.refresh_rules(index)
        self.guarded(edit)

    def move_selected_app_rule(self, step):
        index = self.app_list.currentRow();destination = index+step
        if not 0<=index<len(self.app_rules) or not 0<=destination<len(self.app_rules):return
        def move():
            updated = move_app_rule(self.rule_draft(),self.store.profiles,index,destination)
            self.app_rules = updated['apps'];self.refresh_rules(destination)
        self.guarded(move)

    def remove_app_rule(self):
        index=self.app_list.currentRow()
        if index>=0:self.app_rules.pop(index);self.refresh_rules()

    def add_running_rule(self):
        import os
        from .automation import executables
        paths=sorted(executables(os.getuid()))
        executable,ok=QInputDialog.getItem(self,'Çalışan program','Gerçek yürütülebilir dosya:',paths,0,False)
        if not ok:return
        profile,ok=QInputDialog.getItem(self,'Uygulama profili','Kayıtlı profil:',list(self.store.profiles),0,False)
        if ok:
            self.guarded(lambda:self.upsert_rule(executable,profile))

    def save_automation(self):
        def save():
            rules={key:combo.currentData() for key,combo in self.rule_combos.items()}
            rules.update(apps=self.app_rules,low_hz_on_battery=self.low_hz.isChecked(),lighting_timeout=self.light_timeout.value())
            previous=self.store.data['automation'];self.store.data['automation']=rules
            try:self.store.save()
            except Exception:self.store.data['automation']=previous;raise
            self.command('configure',json.dumps(self.store.data))
        self.guarded(save)

    def build_devices(self):
        page=self.tab('Cihazlar')
        box=self.card(page,'Bağlantı ve giriş aygıtları')
        self.device_text=self.label('KDE aygıtları bekleniyor…');box.addWidget(self.device_text)
        self.camera_check=QCheckBox('Dahili FHD / IR kamera açık');box.addWidget(self.camera_check)
        self.camera_actual=self.label('Kamera durumu okunuyor…');box.addWidget(self.camera_actual)
        self.button(box,'Kamera seçimini uygula',lambda:self.command('camera','1' if self.camera_check.isChecked() else '0'),'camera')
        self.camera_initialized=False
        self.device_checks={};self.desktop_buttons=[]
        for key,title in [('wifi','Wi-Fi'),('bluetooth','Bluetooth')]:
            check=QCheckBox(title+' açık');self.device_checks[key]=check;box.addWidget(check)
            button=QPushButton(title+' seçimini uygula');button.clicked.connect(lambda checked=False,k=key:self.desktop_command({'op':k,'enabled':self.device_checks[k].isChecked()}));box.addWidget(button);self.desktop_buttons.append((button,key))
        self.touchpad_combo=QComboBox();box.addWidget(self.touchpad_combo)
        self.touchpad_check=QCheckBox('Touchpad açık');box.addWidget(self.touchpad_check)
        self.touchpad_button=QPushButton('Touchpad seçimini uygula');self.touchpad_button.clicked.connect(lambda:self.desktop_command({'op':'touchpad','id':self.touchpad_combo.currentData(),'enabled':self.touchpad_check.isChecked()}));box.addWidget(self.touchpad_button)
        row=QHBoxLayout();self.button(row,'Uçak modunu aç',lambda:self.desktop_command({'op':'airplane','enabled':True}));self.button(row,'Radyoları aç',lambda:self.desktop_command({'op':'airplane','enabled':False}));box.addLayout(row)
        box=self.card(page,'OEM donanım desteği')
        self.oem_text=self.label('Destek bilgisi bekleniyor…');box.addWidget(self.oem_text)
        page.addStretch()

    def build_diagnostics(self):
        page = self.tab('Tanılama')
        self.diagnostics = QTextEdit()
        self.diagnostics.setReadOnly(True)
        page.addWidget(self.diagnostics)
        self.button(page, 'İşlem geçmişini yenile', lambda: self.command('events', action=False))
        self.button(page, 'Tanılama raporunu dışa aktar', self.export_diagnostics)
        page.addWidget(self.label('Raporda donanım modeli, BIOS, çekirdek, destek durumu ve işlem sonuçları bulunur. Seri numarası ve kullanıcı dosyaları eklenmez. İşlem kayıtları servis günlüğünde saklanır.'))

    def feedback(self, message, error=False, pending=False):
        self.operation_status.setText(message)
        self.operation_status.setStyleSheet('color:'+('#f28f97' if error else '#dfb477' if pending else '#83cfc6'))

    def update_buttons(self):
        capabilities = self.state.get('capabilities', {})
        for button, capability in self.action_buttons:
            if capability == 'thermal':
                enabled = all(capabilities.get(key) for key in ('power','boost','cpu_limit','gpu_limit'))
            elif capability == 'profile':
                fields = self.store.profiles.get(self.profile_combo.currentText(), {})
                mapping = {'power_profile':'power','boost_enabled':'boost','rgb':'rgb','rgb_map':'rgb_map','brightness':'rgb','cpu_epp':'cpu_epp','cpu_max_mhz':'cpu_limit','gpu_max_mhz':'gpu_limit','fan':'manual_fan'}
                enabled = bool(fields) and all(capabilities.get(mapping[key],False) for key in fields)
            else:
                enabled = capabilities.get(capability,False)
            button.setEnabled(bool(enabled) and not self.action_busy)

    def command(self, op, *arguments, action=True):
        if action:
            if self.action_busy:
                return
            self.fan_feedback_pending = False
            self.action_busy = True
            self.feedback('İşlem sürüyor… Sistemden geri okunarak doğrulanıyor.', pending=True)
            self.update_buttons()
        if self.process:
            if action:
                self.queue.append((op, arguments, action))
            return
        process = QProcess(self)
        self.process = process
        process.setProgram(sys.executable)
        process.setArguments([str(CLIENT), op, *arguments])
        process.finished.connect(lambda code, _: self.complete(process, op, action, code))
        process.errorOccurred.connect(lambda _: self.complete(process, op, action, -1) if process.state() == QProcess.ProcessState.NotRunning else None)
        process.start()
        QTimer.singleShot(20000, lambda: process.kill() if self.process is process else None)

    def complete(self, process, op, action, code):
        if self.process is not process or self.closing:
            return
        self.process = None
        output = bytes(process.readAllStandardOutput()).decode(errors='replace')
        error = bytes(process.readAllStandardError()).decode(errors='replace')
        process.deleteLater()
        try:
            if code:
                raise RuntimeError(error.strip() or 'Servis bağlantısı kurulamadı veya yanıt süresi aşıldı.')
            result = json.loads(output)
            if op == 'status':
                self.update_state(result)
            elif op == 'events':
                self.events = result['events']
                self.render_diagnostics()
            elif action:
                if 'state' in result:
                    self.update_state(result['state'])
                if op == 'thermal_preset' and 'state' in result:
                    current = result['state']
                    self.power_combo.setCurrentIndex(self.power_combo.findData(current['power_profile']))
                    self.boost_check.setChecked(current['boost_enabled'] is True)
                    self.cpu_max.setValue(current['cooling']['cpu']['max_mhz'])
                    self.gpu_max.setValue(current['cooling']['gpu']['requested_max_mhz'] or 0)
                self.feedback(result['message'], pending=result.get('verified') is not True)
                self.fan_feedback_pending = (op in ('set','curve','fan_boost','fan_preset') or (op=='profile' and 'fan' in json.loads(process.arguments()[2]))) and result.get('verified') is not True
                if self.tray.isVisible():self.tray.showMessage('Slayer R9T',result['message'],QSystemTrayIcon.MessageIcon.Information,3000)
        except (RuntimeError, ValueError, KeyError, TypeError) as exc:
            if action:
                self.feedback('Başarısız: '+str(exc), error=True)
            else:
                self.connection_status.setText('Servis verisi alınamadı: '+str(exc))
                self.state = {}
                self.state_received=0
        if action:
            self.action_busy = False
        self.update_buttons()
        if self.queue:
            queued_op, args, queued_action = self.queue.pop(0)
            self.action_busy = False
            self.command(queued_op, *args, action=queued_action)
        elif action:
            self.command('events', action=False)

    def update_state(self, state):
        self.state = state
        self.state_received=time.monotonic()
        epp=state.get('cooling',{}).get('epp',{})
        self.epp_state.setText('Sistemden okunan enerji tercihi: '+EPP_NAMES.get(epp.get('value'),'Karışık / okunamadı')+f" · {epp.get('policy_count',0)} politika"+(' · '+epp.get('reason','') if not epp.get('available') else ''))
        choices=epp.get('choices',[])
        if [self.epp_combo.itemData(i) for i in range(self.epp_combo.count())]!=choices:
            self.epp_combo.clear()
            for value in choices:self.epp_combo.addItem(EPP_NAMES[value],value)
            self.epp_initialized=False
        if choices and not self.epp_initialized:
            self.epp_combo.setCurrentIndex(max(0,self.epp_combo.findData(epp.get('value'))));self.epp_initialized=True
        extra=state.get('features',{});battery=extra.get('battery',{})
        camera=extra.get('camera',{})
        self.camera_actual.setText(('Kamera açık' if camera.get('value') else 'Kamera kapalı')+' · '+camera.get('reason',''))
        if camera.get('available') and not self.camera_initialized:self.camera_check.setChecked(camera['value']);self.camera_initialized=True
        fn=extra.get('fn_lock',{});self.fn_actual.setText('Sistemden okunan Fn Lock: '+('Açık' if fn.get('value')==1 else 'Kapalı' if fn.get('value')==0 else 'Destek yok'))
        self.battery_text.setText(f"Seviye: %{battery.get('capacity','—')} · Sağlık: %{battery.get('health','—')} · Döngü: {battery.get('cycles','—')}\nDurum: {battery.get('status','—')} · Voltaj: {battery.get('voltage_v','—')} V · Akım: {battery.get('current_a','—')} A · Güç: {battery.get('power_w','—')} W")
        self.charge_reason.setText(extra.get('charge_limit',{}).get('reason') or 'Şarj sınırı arayüzü mevcut.')
        self.oem_text.setText('\n\n'.join(title+': '+extra.get(key,{}).get('reason','Bekleniyor…') for key,title in [('oem_mode','OEM performans modları'),('cpu_watts','CPU watt sınırları'),('gpu_watts','GPU TGP / Dynamic Boost'),('mux','GPU geçişi'),('rgb_effects','RGB efektleri'),('camera','Kamera'),('usb_charge','USB şarjı')]))
        auto=state.get('automation',{})
        rules=auto.get('document',{}).get('automation',{})
        enabled=any(rules.values())
        self.automation_text.setText(('Otomasyon kapalı' if not enabled else 'Duraklatıldı' if auto.get('paused') else 'Kurallar izleniyor' if auto.get('session_ready') else 'Kullanıcı oturumu bekleniyor')+' · '+str(auto.get('reason',''))+'\n'+str(auto.get('error','')))
        ec = state.get('ec', {})
        self.connection_status.setText('Servis bağlı · Son okuma '+datetime.now().strftime('%H:%M:%S')+(' · EC: '+ec['error'] if ec.get('error') else ''))
        power = state.get('power_profile')
        boost = state.get('boost_enabled')
        rgb = state.get('rgb_state') or {}
        self.power_state.setText('Sistemden okunan etkin profil: '+POWER_NAMES.get(power, 'Okunamadı'))
        self.boost_state.setText('Sistemden okunan boost: '+('Açık' if boost is True else 'Kapalı' if boost is False else 'Okunamadı'))
        values = rgb.get('rgb')
        color = QColor(*values).name() if values else 'Karışık veya okunamadı'
        self.rgb_state.setText(f'Sürücüden okunan renk: {color} · Parlaklık: {rgb.get("brightness", "—")}%')
        if not self.initialized:
            if power in POWER_NAMES:
                self.power_combo.setCurrentIndex(self.power_combo.findData(power))
            self.boost_check.setChecked(boost is True)
            self.fn_check.setChecked(extra.get('fn_lock',{}).get('value')==1)
            if values:
                self.color = QColor(*values)
                self.color_button.setText('Seçilen renk: '+self.color.name())
            if rgb.get('brightness') is not None:
                self.brightness.setValue(rgb['brightness'])
            self.initialized = True
        fans=state.get('fans',{})
        measured=fans.get('measured') or {}
        for key,card in self.fan_metrics.items():card.set_value(measured.get(key))
        names={'auto':'EC otomatik','manual':'Elle hız','curve':'Fan eğrisi','boost':'Maksimum soğutma'}
        fan_text=names.get(fans.get('mode'),'Sürücü hazır değil')
        if fans.get('mode')!='auto':fan_text+=' · '+('Hedef / RPM doğrulandı' if fans.get('verified') else 'Doğrulama sürüyor')
        if fans.get('error'):fan_text+=' · '+fans['error']
        if not fans.get('available'):fan_text='R9T fan sürücüsü hazır değil; kontrol uygulanamaz.'
        self.fan_validation.setText(fan_text)
        if self.fan_feedback_pending and (fans.get('verified') or fans.get('error')):
            self.feedback('Fan kontrolü: '+fan_text,error=bool(fans.get('error')))
            self.fan_feedback_pending = False
        targets=fans.get('targets')
        target_text=(f"CPU %{targets[0]} / GPU %{targets[1]}" if targets else 'EC otomatik belirler')
        self.fan_targets.setText(f"Etkin hedef: {target_text} · CPU {measured.get('cpu_temp','—')}°C / GPU {measured.get('gpu_temp','—')}°C")
        self.fan_validation.setStyleSheet('color:'+('#f28f97' if fans.get('error') else '#83cfc6' if fans.get('verified') else '#dfb477'))
        self.fan_state.setText(fan_text+' · CPU '+str(measured.get('cpu_rpm','—'))+' RPM · GPU '+str(measured.get('gpu_rpm','—'))+' RPM')
        if not self.fan_preset_combo.count():
            for key,preset in fans.get('presets',{}).items():self.fan_preset_combo.addItem(preset['name'],key)
        limits = state.get('cooling', {})
        target = limits.get('temperature_target', {})
        text = 'Etkin · CPU / GPU hedefleri: '+str(target.get('targets'))+' °C · Frekans sınırları: '+str(target.get('limits_mhz'))+' MHz' if target.get('active') else 'Sıcaklık hedefi kapalı.'
        if target.get('error'): text += ' · '+target['error']
        self.target_state.setText(text)
        cpu_limits = limits.get('cpu') or {}
        gpu_limits = limits.get('gpu') or {}
        self.cpu_limit_state.setText('Sistemden okunan CPU üst sınırı: '+str(cpu_limits.get('max_mhz','—'))+' MHz')
        target = gpu_limits.get('requested_max_mhz')
        self.gpu_limit_state.setText('Bu servisin son GPU isteği: '+(str(target)+' MHz' if target else 'Sınır isteği yok')+
            ' · Ölçülen frekans: '+str(gpu_limits.get('current_mhz','—'))+' MHz'+
            ' · Etkin güç tavanı: '+str(gpu_limits.get('enforced_power_w','—'))+' W')
        if not self.cooling_initialized:
            self.thermal_combo.clear()
            for key, preset in limits.get('presets',{}).items():
                self.thermal_combo.addItem(preset['name'],key)
            if cpu_limits.get('allowed_max_mhz'):self.cpu_max.setMaximum(cpu_limits['allowed_max_mhz'])
            if cpu_limits.get('max_mhz') is not None:self.cpu_max.setValue(cpu_limits['max_mhz'])
            if 'max_mhz' in gpu_limits:self.gpu_max.setMaximum(gpu_limits['max_mhz'])
            self.gpu_max.setValue(target or 0)
            self.cooling_initialized = bool(limits)
        self.preview_thermal()
        self.update_buttons()
        self.render_diagnostics()

    def desktop_command(self,payload,action=True):
        if self.desktop_process:
            if action:self.feedback('Masaüstü işlemi sürüyor; bitmesini bekleyin.',pending=True)
            return
        process=QProcess(self);self.desktop_process=process;self.desktop_busy=action
        process.setProgram(sys.executable);process.setArguments([str(DESKTOP_CLIENT),json.dumps(payload)])
        process.finished.connect(lambda code,_:self.desktop_complete(process,action,code))
        process.errorOccurred.connect(lambda _:self.desktop_complete(process,action,-1) if process.state()==QProcess.ProcessState.NotRunning else None)
        process.start();QTimer.singleShot(35000,lambda:process.kill() if self.desktop_process is process else None)
        if action:self.feedback('Masaüstü ayarı uygulanıyor…',pending=True)

    def desktop_complete(self,process,action,code):
        if self.desktop_process is not process or self.closing:return
        self.desktop_process=None;self.desktop_busy=False
        output=bytes(process.readAllStandardOutput()).decode();error=bytes(process.readAllStandardError()).decode();process.deleteLater()
        try:
            if code:raise RuntimeError(error.strip() or 'KDE oturum servisi hazır değil.')
            result=json.loads(output)
            self.desktop_state=result.get('state',result) if action else result
            if action:self.feedback(result['message'],pending=not result.get('verified'))
            screens=self.desktop_state.get('screens',[])
            known=[self.display_combo.itemData(i) for i in range(self.display_combo.count())]
            if known!=[s['id'] for s in screens]:
                self.display_combo.clear()
                for screen in screens:self.display_combo.addItem(screen['name'],screen['id'])
                self.refresh_modes()
            self.display_text.setText(' · '.join(s['name']+' / '+next((m['name'] for m in s['modes'] if m['id']==s['currentModeId']),'—') for s in screens) or self.desktop_state.get('errors',{}).get('screens','Ekran bilgisi yok.'))
            self.night_actual.setText('Sistemden okunan gece rengi: '+('Açık' if self.desktop_state.get('nightlight') is True else 'Kapalı' if self.desktop_state.get('nightlight') is False else 'Destek yok'))
            self.display_apply.setEnabled(bool(screens));self.display_brightness_button.setEnabled(bool(screens));self.night_button.setEnabled(self.desktop_state.get('nightlight') is not None)
            pads=self.desktop_state.get('touchpads',[])
            if [self.touchpad_combo.itemData(i) for i in range(self.touchpad_combo.count())]!=[p['id'] for p in pads]:
                self.touchpad_combo.clear()
                for pad in pads:self.touchpad_combo.addItem(pad['name'],pad['id'])
            self.touchpad_button.setEnabled(bool(pads))
            for button,key in self.desktop_buttons:button.setEnabled(self.desktop_state.get(key) is not None)
            self.device_text.setText(' · '.join(key+': '+('Açık' if self.desktop_state.get(key) is True else 'Kapalı' if self.desktop_state.get(key) is False else 'Destek yok') for key in ('wifi','bluetooth'))+'\n'+' · '.join(p['name']+': '+('Açık' if p['enabled'] else 'Kapalı') for p in pads)+'\n'+self.desktop_state.get('super_lock',{}).get('reason',''))
            if not self.desktop_initialized:
                for key,check in self.device_checks.items():check.setChecked(self.desktop_state.get(key) is True)
                if pads:self.touchpad_check.setChecked(pads[0]['enabled'])
                self.night_check.setChecked(self.desktop_state.get('nightlight') is True)
                if screens:self.display_brightness.setValue(round(screens[0].get('brightness',1)*100))
                self.desktop_initialized=True
        except (OSError,RuntimeError,ValueError,KeyError) as exc:
            self.display_text.setText(str(exc));self.device_text.setText(str(exc))
            self.display_apply.setEnabled(False);self.display_brightness_button.setEnabled(False);self.night_button.setEnabled(False);self.touchpad_button.setEnabled(False)
            for button,_ in self.desktop_buttons:button.setEnabled(False)
            if action:self.feedback('Başarısız: '+str(exc),error=True)

    def metric_value(self, key, value, unit):
        self.metrics[key].set_value(value)

    def poll(self):
        if self.tabs.currentIndex()==self.gpu_diagnostic_index:
            if self.gpu_process: self.gpu_process.kill()
            self.gpu_values=[None]*3; self.gpu_received=0
            self.connection_status.setText('GPU tanılaması · düzenli NVIDIA ve servis durum sorguları duraklatıldı.')
            self.poll_gpu_diagnostics()
            return
        cpu_temp=telemetry.temperature('k10temp');cpu_usage=self.cpu_usage.read()
        self.metric_value('cpu',cpu_temp,'°C')
        self.metric_value('cpu_util',cpu_usage,'%')
        battery = telemetry.battery()
        self.metric_value('battery', battery['capacity'] if battery else None, '%')
        memory = telemetry.memory()
        parts = ['Sensör okuması '+datetime.now().strftime('%H:%M:%S')]
        if memory:
            parts.append(f'RAM {memory["used_gib"]:.1f}/{memory["total_gib"]:.1f} GiB')
        ssd = telemetry.temperature('nvme')
        if ssd is not None:
            parts.append(f'SSD {ssd:.0f}°C')
        if battery:
            status = {'Charging':'Şarj oluyor', 'Discharging':'Bataryadan çalışıyor', 'Full':'Dolu', 'Not charging':'Şarj edilmiyor'}
            parts.append(status.get(battery['status'], battery['status']))
            if 'health' in battery:
                parts.append(f'Pil kapasite sağlığı %{battery["health"]}')
            if 'cycles' in battery:
                parts.append(f'{battery["cycles"]} pil döngüsü')
        self.sensor_summary.setText('  ·  '.join(parts))
        self.render_sensors()
        self.record_history(cpu_temp,cpu_usage,battery)
        self.poll_gpu()
        self.command('status', action=False)
        self.desktop_command({'op':'status'},action=False)
        if self.tabs.currentIndex()==self.dynamic_boost_index:self.poll_dynamic_boost()

    def poll_gpu(self):
        if self.gpu_process:
            return
        process = QProcess(self)
        self.gpu_process = process
        process.setProgram('/usr/bin/nvidia-smi')
        process.setArguments(['--query-gpu=temperature.gpu,utilization.gpu,power.draw', '--format=csv,noheader,nounits'])
        process.finished.connect(lambda code, _: self.gpu_done(process, code))
        process.errorOccurred.connect(lambda _: self.gpu_done(process, -1) if process.state() == QProcess.ProcessState.NotRunning else None)
        process.start()
        QTimer.singleShot(2000, lambda: process.kill() if self.gpu_process is process else None)

    def gpu_done(self, process, code):
        if self.gpu_process is not process:
            return
        self.gpu_process = None
        try:
            if code:
                raise ValueError()
            import math
            fields=bytes(process.readAllStandardOutput()).decode().splitlines()[0].split(',')
            if len(fields)!=3:raise ValueError()
            values=[]
            for field,upper in zip(fields,(115,100,500)):
                try:value=float(field)
                except ValueError:value=None
                values.append(value if value is not None and math.isfinite(value) and 0<=value<=upper else None)
        except (ValueError, IndexError):
            values = [None]*3
        self.gpu_received=time.monotonic();self.gpu_values=values
        for key, value, unit in zip(('gpu', 'gpu_util', 'gpu_power'), values, ('°C', '%', ' W')):
            self.metric_value(key, value, unit)
        process.deleteLater()

    def pick_color(self):
        color = QColorDialog.getColor(self.color, self, 'Klavye rengi')
        if color.isValid():
            self.color = color
            self.color_button.setText('Seçilen renk: '+color.name())

    def lighting_settings(self):
        key=self.light_pattern.currentData()
        if key=='custom':return {'brightness':self.brightness.value(),'rgb_map':copy.deepcopy(validate_map(self.rgb_map_draft))}
        return dict(brightness=self.brightness.value(),**({'rgb':self.rgb_values()} if key=='uniform' else {'rgb_map':pattern(key,self.rgb_values())}))

    def edit_rgb_map(self):
        current = self.lighting_settings()
        colors = current.get('rgb_map') or [self.rgb_values() for _ in range(ROWS*COLUMNS)]
        dialog = LightingEditor(colors,self)
        try:
            if dialog.exec()!=QDialog.DialogCode.Accepted:return
            self.rgb_map_draft = dialog.colors()
            if self.light_pattern.findData('custom')<0:self.light_pattern.addItem('Özel renk düzeni','custom')
            self.light_pattern.setCurrentIndex(self.light_pattern.findData('custom'))
            self.feedback('Renk düzeni taslakta hazır. Klavyeye göndermek için renk düzenini uygula düğmesini kullanın.')
        finally:dialog.deleteLater()

    def rgb_values(self):
        return [self.color.red(), self.color.green(), self.color.blue()]

    def reload_profiles(self, name=None):
        self.profile_combo.clear()
        self.profile_combo.addItems(sorted(self.store.profiles))
        if name:
            self.profile_combo.setCurrentText(name)
        self.preview_profile()
        self.refresh_rules();self.update_tray_profiles()

    def preview_profile(self, *_):
        settings = self.store.profiles.get(self.profile_combo.currentText())
        if not settings:
            self.profile_preview.setText('Henüz profil yok.')
            return
        parts = []
        if 'power_profile' in settings: parts.append(POWER_NAMES[settings['power_profile']])
        if 'cpu_epp' in settings:parts.append('CPU enerji: '+EPP_NAMES[settings['cpu_epp']])
        if 'boost_enabled' in settings: parts.append('Boost '+('açık' if settings['boost_enabled'] else 'kapalı'))
        if 'rgb' in settings: parts.append('Klavye '+QColor(*settings['rgb']).name()+f' · %{settings["brightness"]} parlaklık')
        if 'rgb_map' in settings:parts.append(f'Klavye: 126 kanallı renk düzeni · %{settings["brightness"]} parlaklık')
        for key, label in [('cpu_max_mhz','CPU üst sınırı'),('gpu_max_mhz','GPU üst sınırı')]:
            if key in settings: parts.append(label+': '+(str(settings[key])+' MHz' if settings[key] else 'Otomatik'))
        if 'fan' in settings:
            fan=settings['fan'];parts.append('Fan: '+{'auto':'EC otomatik','boost':'Maksimum','manual':f"CPU %{fan.get('cpu')} / GPU %{fan.get('gpu')}",'curve':'Özel eğri','preset':fan.get('preset','')}[fan['mode']])
        self.profile_preview.setText('  /  '.join(parts))
        self.update_buttons()

    def guarded(self, callback):
        try:
            callback()
        except (OSError, ValueError, RuntimeError) as exc:
            self.feedback(str(exc), error=True)

    def save_profile(self):
        if not self.initialized:
            self.feedback('Önce servis bağlantısının kurulmasını bekleyin.', error=True)
            return
        name, ok = QInputDialog.getText(self, 'Yeni profil', 'Profil adı (1–80 karakter):')
        if not ok or not name.strip():
            return
        def save():
            name_key = name.strip()
            if name_key in self.store.profiles:
                raise ValueError('Bu ad zaten kayıtlı; yeni bir profil adı kullanın.')
            settings = {'power_profile': self.power_combo.currentData(), 'boost_enabled': self.boost_check.isChecked(),
                        **self.lighting_settings(),
                        'cpu_epp':self.epp_combo.currentData(),'cpu_max_mhz':self.cpu_max.value(),'gpu_max_mhz':self.gpu_max.value()}
            capabilities = self.state.get('capabilities',{})
            if not capabilities.get('cpu_epp'):settings.pop('cpu_epp')
            if not capabilities.get('cpu_limit'):settings.pop('cpu_max_mhz')
            if not capabilities.get('gpu_limit'):settings.pop('gpu_max_mhz')
            mode=self.profile_fan.currentData()
            if mode:
                fan={'mode':mode}
                if mode=='manual':fan.update(cpu=self.fan_cpu.value(),gpu=self.fan_gpu.value())
                elif mode=='curve':fan['points']=[[t.value(),p.value()] for t,p in self.fan_points]
                elif mode=='preset':fan['preset']=self.fan_preset_combo.currentData()
                settings['fan']=fan
            previous = dict(self.store.profiles)
            self.store.profiles[name_key] = settings
            try:
                self.store.save()
            except Exception:
                self.store.data['profiles'] = previous
                raise
            self.reload_profiles(name_key)
            self.command('configure',json.dumps(self.store.data))
        self.guarded(save)

    def edit_profile(self):
        self.open_profile_editor(copying=False)

    def copy_profile(self):
        self.open_profile_editor(copying=True)

    def open_profile_editor(self, copying=False):
        source = self.profile_combo.currentText()
        settings = self.store.profiles.get(source)
        if not settings:
            self.feedback('Önce bir profil seçin.',error=True);return
        def edit():
            dialog = ProfileEditor(source,settings,self,copying)
            try:
                if dialog.exec()!=QDialog.DialogCode.Accepted:return
                name = dialog.name.text().strip() if copying else source
                if copying and name in self.store.profiles:raise ValueError('Bu ad zaten kayıtlı; yeni bir profil adı kullanın.')
                updated = dialog.profile_settings()
                previous = copy.deepcopy(self.store.data)
                self.store.profiles[name] = updated
                try:self.store.save()
                except Exception:self.store.data = previous;raise
                self.reload_profiles(name)
                self.command('configure',json.dumps(self.store.data))
            finally:dialog.deleteLater()
        self.guarded(edit)

    def apply_profile(self):
        settings = self.store.profiles.get(self.profile_combo.currentText())
        if settings:
            self.command('profile', json.dumps(settings))
        else:
            self.feedback('Önce bir profil kaydedin veya seçin.', error=True)

    def delete_profile(self):
        name = self.profile_combo.currentText()
        if name not in self.store.profiles:
            return
        def remove():
            import copy
            previous_document=copy.deepcopy(self.store.data)
            previous = self.store.profiles.pop(name)
            rules=self.store.data['automation']
            for key in ('startup','ac','battery'):
                if rules[key]==name:rules[key]=None
            rules['apps']=[r for r in rules['apps'] if r['profile']!=name]
            self.app_rules=[r for r in self.app_rules if r['profile']!=name]
            try:
                self.store.save()
            except Exception:
                self.store.data=previous_document
                raise
            self.reload_profiles()
            self.command('configure',json.dumps(self.store.data))
        self.guarded(remove)

    def import_profiles(self):
        path, _ = QFileDialog.getOpenFileName(self, 'Profil dosyası', '', 'JSON (*.json)')
        if path:
            def load():
                self.store.import_file(path)
                self.reload_profiles()
                self.command('configure',json.dumps(self.store.data))
            self.guarded(load)

    def export_profiles(self):
        path, _ = QFileDialog.getSaveFileName(self, 'Profilleri kaydet', 'slayer-profilleri.json', 'JSON (*.json)')
        if path:
            self.guarded(lambda: self.store.export_file(path))

    def report(self):
        return {'application_version': __version__, 'kernel': platform.release(), 'state': self.state,'desktop':self.desktop_state, 'events': self.events}

    def render_diagnostics(self):
        self.diagnostics.setPlainText(json.dumps(self.report(), ensure_ascii=False, indent=2))

    def export_diagnostics(self):
        path, _ = QFileDialog.getSaveFileName(self, 'Tanılama raporunu kaydet', 'slayer-tanilama.json', 'JSON (*.json)')
        if path:
            self.guarded(lambda: atomic_json(path, self.report()))

    def closeEvent(self, event):
        self.closing = True
        self.queue.clear()
        self.timer.stop()
        for process in (self.process, self.gpu_process,self.desktop_process,self.gpu_diagnostic_process,self.dynamic_boost_process):
            if process:
                process.kill()
                process.waitForFinished(1000)
        super().closeEvent(event)


def main():
    app = QApplication(sys.argv)
    app.setApplicationName('Slayer R9T Kontrol Merkezi')
    app.setFont(QFont('Noto Sans', 10))
    window = ControlCenter()
    if '--fan' in sys.argv:
        window.tabs.setCurrentIndex(2)
    window.show()
    return app.exec()
