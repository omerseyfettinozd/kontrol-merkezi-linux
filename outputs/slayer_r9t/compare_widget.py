"""Panel for user-started A/B measurement windows. Read-only: never changes hardware settings."""
import time
from PySide6.QtCore import QTimer
from PySide6.QtWidgets import QFileDialog,QHBoxLayout,QLabel,QPushButton,QTableWidget,QTableWidgetItem,QVBoxLayout,QWidget
from .compare import COMPARE_METRICS,compare,export_csv,fmt,summarize,warnings

class ComparePanelWidget(QWidget):
    def __init__(self,history,parent=None,clock=time.monotonic):
        super().__init__(parent)
        self.history=history;self.clock=clock
        self.windows={'A':None,'B':None}  # name -> [start,end or None]
        lay=QVBoxLayout(self)
        lay.addWidget(QLabel('Profil karşılaştırma: A ölçümünü başlatıp durdurun, profili değiştirin, sonra B ölçümünü yapın.'))
        row=QHBoxLayout();self.buttons={}
        for n in ('A','B'):
            b=QPushButton(f'Başlat {n}');b.clicked.connect(lambda _=False,n=n:self.toggle(n));self.buttons[n]=b;row.addWidget(b)
        self.export_button=QPushButton('CSV dışa aktar');self.export_button.clicked.connect(self.export_dialog);row.addWidget(self.export_button)
        lay.addLayout(row)
        self.table=QTableWidget(len(COMPARE_METRICS),7)
        self.table.setHorizontalHeaderLabels(['Ölçüm','A ort.','B ort.','Fark ort.','A tepe','B tepe','Fark tepe'])
        lay.addWidget(self.table)
        self.info=QLabel('');lay.addWidget(self.info)
        self.warning=QLabel('');self.warning.setWordWrap(True);lay.addWidget(self.warning)
        self.timer=QTimer(self);self.timer.setInterval(1000);self.timer.timeout.connect(self.refresh)
        self.refresh()

    def running(self,n):
        w=self.windows[n];return bool(w) and w[1] is None

    def toggle(self,n):
        now=self.clock()
        if self.running(n):self.windows[n][1]=now
        else:self.windows[n]=[now,None]
        if any(self.running(x) for x in 'AB'):self.timer.start()
        else:self.timer.stop()
        self.refresh()

    def summary(self,n):
        w=self.windows[n]
        if not w:return None
        return summarize(self.history.rows,w[0],self.clock() if w[1] is None else w[1])

    def refresh(self):
        for n in 'AB':self.buttons[n].setText(('Durdur ' if self.running(n) else 'Başlat ')+n)
        a,b=self.summary('A'),self.summary('B')
        empty=lambda:summarize([],0,0)
        ra=compare(a or empty(),b or empty())
        for i,r in enumerate(ra):
            vals=[f"{r['title']} ({r['unit']})",*[fmt(r[k]) for k in ('a_avg','b_avg','avg_diff','a_peak','b_peak','peak_diff')]]
            for j,v in enumerate(vals):self.table.setItem(i,j,QTableWidgetItem(v))
        d=lambda s:'—' if s is None else f"{s['duration']:.0f} sn · {s['samples']} örnek"
        self.info.setText(f'A: {d(a)}    B: {d(b)}')
        self.warning.setText('\n'.join(warnings(a or empty(),b or empty())))

    def export_dialog(self):
        path,_=QFileDialog.getSaveFileName(self,'Karşılaştırmayı kaydet','karsilastirma.csv','CSV (*.csv)')
        if path:self.export_to(path)

    def export_to(self,path):
        a,b=self.summary('A'),self.summary('B')
        if not a or not b:return 0
        return export_csv(path,a,b)
