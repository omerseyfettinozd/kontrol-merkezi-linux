"""Shared visual system and telemetry cards."""
import math
from PySide6.QtCore import Qt, QRectF, QPointF
from PySide6.QtGui import QColor, QPainter, QPainterPath, QPen, QLinearGradient
from PySide6.QtWidgets import QFrame, QWidget, QLabel, QVBoxLayout, QHBoxLayout

STYLE = '''
QMainWindow,QWidget#root,QWidget#page,QScrollArea{background:#0b1220;color:#edf3fc}
QScrollArea{border:0}
QFrame#card{background:#121d2d;border:1px solid #243348;border-radius:16px}
QLabel{color:#e8eff9;background:transparent}
QLabel#eyebrow{color:#8193ad;font-size:10px;font-weight:600;letter-spacing:1px}
QLabel#title{color:#f2f6fd;font-size:27px;font-weight:700}
QLabel#subtitle,QLabel#hint,QLabel#status{color:#94a5bc;font-size:12px}
QLabel#section{color:#e8eff9;font-size:15px;font-weight:600}
QLabel#metricValue{color:#f1f6fd;font-size:38px;font-weight:600}
QLabel#metricUnit{color:#879bb6;font-size:15px;font-weight:500;padding-bottom:6px}
QLabel#metricTitle{color:#c0cce0;font-size:12px;font-weight:600}
QLabel#metricDetail{color:#8295af;font-size:10px}
QCheckBox{color:#e8eff9;spacing:8px}
QCheckBox::indicator{width:16px;height:16px;border:1px solid #42546d;border-radius:4px;background:#182538}
QCheckBox::indicator:checked{background:#50c9ba;border-color:#50c9ba}
QPushButton,QComboBox,QSpinBox{background:#19273b;border:1px solid #304158;border-radius:9px;padding:9px 12px;color:#dce7f6}
QPushButton:disabled{color:#657892;background:#121d2d;border-color:#243348}
QPushButton:hover{background:#22354b;border-color:#4b7c8f}
QPushButton:pressed{background:#183a41;border-color:#50c9ba}
QPushButton#primary{background:#50c9ba;color:#092a2c;border:0;font-weight:600}
QPushButton#selected{background:#17383c;color:#8fe5d9;border:1px solid #347d79}
QComboBox QAbstractItemView{background:#19273b;color:#e8eff9;selection-background-color:#245157;border:1px solid #304158}
QTabWidget::pane{border:0}
QTabBar QToolButton{background:#19273b;color:#dce7f6;border:1px solid #304158;border-radius:5px}
QTabBar::tab{background:#121d2d;color:#91a2bb;padding:12px 18px;margin:0 5px 12px 0;border-radius:9px}
QTabBar::tab:selected{background:#17383c;color:#8fe5d9}
QTabBar::tab:hover{background:#1b2c40;color:#dce7f6}
QTextEdit{background:#121d2d;color:#c8d5e9;border:1px solid #243348;border-radius:10px;padding:12px;selection-background-color:#245157}
QScrollBar:vertical{background:#0b1220;width:8px;margin:0}
QScrollBar::handle:vertical{background:#304158;border-radius:4px;min-height:30px}
QScrollBar::add-line:vertical,QScrollBar::sub-line:vertical{height:0}
QScrollBar::add-page:vertical,QScrollBar::sub-page:vertical{background:transparent}
'''


class Card(QFrame):
    def __init__(self):
        super().__init__()
        self.setObjectName('card')


class MetricIcon(QWidget):
    """Small vector symbols; independent of desktop fonts and emoji rendering."""
    def __init__(self, kind, color):
        super().__init__()
        self.kind, self.color = kind, QColor(color)
        self.setFixedSize(32, 32)

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        background = QColor(self.color)
        background.setAlpha(22)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(background)
        painter.drawRoundedRect(QRectF(0, 0, 32, 32), 9, 9)
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.setPen(QPen(self.color, 1.5, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap, Qt.PenJoinStyle.RoundJoin))
        if self.kind == 'temperature':
            path = QPainterPath()
            path.moveTo(13, 19)
            path.lineTo(13, 10)
            path.cubicTo(13, 6, 19, 6, 19, 10)
            path.lineTo(19, 19)
            path.cubicTo(24, 25, 8, 28, 12, 20)
            painter.drawPath(path)
            painter.drawLine(QPointF(16, 12), QPointF(16, 22))
        elif self.kind == 'power':
            path = QPainterPath()
            path.moveTo(18, 7)
            for x, y in [(11,17), (16,17), (14,25), (22,14), (17,14), (18,7)]:
                path.lineTo(x, y)
            painter.drawPath(path)
        elif self.kind == 'battery':
            painter.drawRoundedRect(QRectF(7, 11, 17, 11), 2, 2)
            painter.drawLine(QPointF(26,14), QPointF(26,19))
            painter.drawLine(QPointF(11,14), QPointF(11,19))
            painter.drawLine(QPointF(15,14), QPointF(15,19))
        else:
            painter.drawRoundedRect(QRectF(10, 10, 12, 12), 2, 2)
            painter.drawRect(QRectF(14,14,4,4))
            for x in (13,19):
                painter.drawLine(QPointF(x,7),QPointF(x,10))
                painter.drawLine(QPointF(x,22),QPointF(x,25))
                painter.drawLine(QPointF(7,x),QPointF(10,x))
                painter.drawLine(QPointF(22,x),QPointF(25,x))


class Sparkline(QWidget):
    def __init__(self, color, upper=100):
        super().__init__()
        self.color = QColor(color)
        self.upper = upper
        self.values = []
        self.setFixedHeight(52)

    def add(self, value):
        self.values.append(value)
        self.values = self.values[-60:]
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        rect = QRectF(1, 3, self.width()-3, self.height()-7)
        painter.setPen(QPen(QColor('#243247'), 1, Qt.PenStyle.DotLine))
        for fraction in (0, .5, 1):
            y = rect.top()+rect.height()*fraction
            painter.drawLine(QPointF(rect.left(), y), QPointF(rect.right(), y))
        if not self.values:
            return
        upper = max(self.upper, max((v for v in self.values if v is not None), default=0)*1.1)
        points = []
        for index, value in enumerate(self.values):
            if value is None:
                if points:
                    self.draw_segment(painter, points, rect)
                    points = []
                continue
            x = rect.right()-(len(self.values)-1-index)*rect.width()/59
            y = rect.bottom()-max(0,min(value/upper,1))*rect.height()
            points.append(QPointF(x,y))
        if points:
            self.draw_segment(painter, points, rect, last=self.values[-1] is not None)

    def draw_segment(self, painter, points, rect, last=False):
        path = QPainterPath(points[0])
        for point in points[1:]:
            path.lineTo(point)
        fill = QPainterPath(path)
        fill.lineTo(points[-1].x(), rect.bottom())
        fill.lineTo(points[0].x(), rect.bottom())
        fill.closeSubpath()
        gradient = QLinearGradient(0, rect.top(), 0, rect.bottom())
        top, bottom = QColor(self.color), QColor(self.color)
        top.setAlpha(48)
        bottom.setAlpha(3)
        gradient.setColorAt(0,top)
        gradient.setColorAt(1,bottom)
        painter.fillPath(fill,gradient)
        painter.setPen(QPen(self.color, 1.8, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap, Qt.PenJoinStyle.RoundJoin))
        painter.drawPath(path)
        if last:
            glow = QColor(self.color)
            glow.setAlpha(35)
            painter.setPen(Qt.PenStyle.NoPen)
            painter.setBrush(glow)
            painter.drawEllipse(points[-1],5,5)
            painter.setBrush(self.color)
            painter.drawEllipse(points[-1],2.5,2.5)


class MetricCard(QFrame):
    def __init__(self, title, subtitle, color, unit, kind, upper):
        super().__init__()
        self.color, self.unit = QColor(color), unit
        self.setMinimumHeight(192)
        box = QVBoxLayout(self)
        box.setContentsMargins(18,16,18,14)
        box.setSpacing(7)
        heading = QHBoxLayout()
        heading.setSpacing(10)
        heading.addWidget(MetricIcon(kind,color))
        names = QVBoxLayout()
        names.setSpacing(2)
        names.addWidget(self.label(title,'metricTitle'))
        names.addWidget(self.label(subtitle,'metricDetail'))
        heading.addLayout(names)
        heading.addStretch()
        self.indicator = QLabel('BEKLENİYOR')
        self.indicator.setStyleSheet('color:#8295af;font-size:8px;font-weight:600;')
        heading.addWidget(self.indicator)
        box.addLayout(heading)
        value_row = QHBoxLayout()
        value_row.setSpacing(7)
        self.value = self.label('—','metricValue')
        self.unit_label = self.label(unit,'metricUnit')
        value_row.addWidget(self.value)
        value_row.addWidget(self.unit_label,0,Qt.AlignmentFlag.AlignBottom)
        value_row.addStretch()
        box.addLayout(value_row)
        self.plot = Sparkline(color, upper)
        box.addWidget(self.plot)
        footer = QHBoxLayout()
        self.range = self.label('Min —   /   Maks —','metricDetail')
        footer.addWidget(self.range)
        footer.addStretch()
        footer.addWidget(self.label('3 dk pencere','metricDetail'))
        box.addLayout(footer)

    @staticmethod
    def label(text, role):
        label = QLabel(text)
        label.setObjectName(role)
        return label

    def set_value(self, value):
        if value is not None and not math.isfinite(value):
            value = None
        self.value.setText(f'{value:.0f}' if value is not None else '—')
        self.unit_label.setVisible(value is not None)
        self.indicator.setText('● CANLI' if value is not None else 'VERİ YOK')
        self.indicator.setStyleSheet('color:'+('#83cfc6' if value is not None else '#8295af')+';font-size:8px;font-weight:600;')
        self.plot.add(value)
        values = [v for v in self.plot.values if v is not None]
        self.range.setText(f'Min {min(values):.0f}{self.unit}   /   Maks {max(values):.0f}{self.unit}' if values else 'Min —   /   Maks —')

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        rect = QRectF(.5,.5,self.width()-1,self.height()-1)
        path = QPainterPath()
        path.addRoundedRect(rect,16,16)
        gradient = QLinearGradient(rect.topLeft(),rect.bottomRight())
        gradient.setColorAt(0,QColor('#142234'))
        gradient.setColorAt(1,QColor('#101a29'))
        painter.fillPath(path,gradient)
        painter.setPen(QPen(QColor('#29394f'),1))
        painter.drawPath(path)


class HistoryPlot(QWidget):
    """A timestamped plot; unavailable samples and sampling pauses remain gaps."""
    def __init__(self):
        super().__init__()
        self.setMinimumHeight(270)
        self.rows=[];self.key='cpu_temp';self.seconds=300;self.now=0

    def set_series(self,rows,key,seconds,now):
        self.rows=rows;self.key=key;self.seconds=seconds;self.now=now;self.update()

    def paintEvent(self,event):
        from .history import METRICS
        painter=QPainter(self);painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        title,unit,color,upper=METRICS[self.key]
        values=[r[self.key] for r in self.rows if r.get(self.key) is not None]
        upper=max(upper,max(values,default=0)*1.1)
        lower=min(0,min(values,default=0)*1.1)
        span=upper-lower
        rect=QRectF(65,20,max(1,self.width()-85),max(1,self.height()-58))
        for fraction in (0,.25,.5,.75,1):
            y=rect.bottom()-rect.height()*fraction
            painter.setPen(QPen(QColor('#243247'),1,Qt.PenStyle.DotLine))
            painter.drawLine(QPointF(rect.left(),y),QPointF(rect.right(),y))
            painter.setPen(QColor('#94a5bc'));painter.drawText(QRectF(0,y-9,57,20),Qt.AlignmentFlag.AlignRight,f'{lower+span*fraction:.0f}')
        painter.drawText(QRectF(0,0,60,20),Qt.AlignmentFlag.AlignRight,unit)
        for fraction in (0,.5,1):
            x=rect.left()+rect.width()*fraction
            painter.drawText(QRectF(x-35,rect.bottom()+8,70,22),Qt.AlignmentFlag.AlignCenter,'Şimdi' if fraction==1 else f'-{self.seconds*(1-fraction)/60:g} dk')
        path=QPainterPath();last=None;count=0
        for row in self.rows:
            value=row.get(self.key);stamp=row['monotonic']
            if value is None:last=None;continue
            x=rect.right()-(self.now-stamp)/self.seconds*rect.width()
            y=rect.bottom()-max(0,min((value-lower)/span,1))*rect.height()
            if last is None or stamp-last>9:path.moveTo(x,y)
            else:path.lineTo(x,y)
            last=stamp;count+=1
            painter.setPen(QPen(QColor(color),2));painter.drawEllipse(QPointF(x,y),1.5,1.5)
        painter.setPen(QPen(QColor(color),2));painter.drawPath(path)
        if not count:
            painter.setPen(QColor('#94a5bc'));painter.drawText(rect,Qt.AlignmentFlag.AlignCenter,'Bu aralıkta ölçüm yok')
