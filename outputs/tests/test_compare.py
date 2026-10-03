import csv,os,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
os.environ.setdefault('QT_QPA_PLATFORM','offscreen')
from slayer_r9t.compare import summarize,compare,warnings,export_csv,PERMANENT_WARNING
from slayer_r9t.history import History

def hist(vals,t0=0,step=1):
    h=History()
    for i,v in enumerate(vals):h.add(v,now=t0+i*step,utc='x')
    return h

class CompareTests(unittest.TestCase):
    def test_avg_peak_and_missing(self):
        h=hist([{'cpu_temp':50,'cpu_power':10},{'cpu_temp':70,'cpu_power':None},{'cpu_temp':60}])
        s=summarize(h.rows,0,2)
        self.assertEqual(s['samples'],3)
        self.assertEqual(s['metrics']['cpu_temp']['avg'],60);self.assertEqual(s['metrics']['cpu_temp']['peak'],70)
        self.assertEqual(s['metrics']['cpu_power']['avg'],10)
        self.assertIsNone(s['metrics']['gpu_power']['avg']);self.assertIsNone(s['metrics']['gpu_temp']['peak'])
    def test_diff_none_when_missing(self):
        a=summarize(hist([{'cpu_temp':50}]).rows,0,0);b=summarize(hist([{'cpu_temp':55,'gpu_temp':40}]).rows,0,0)
        r={x['key']:x for x in compare(a,b)}
        self.assertEqual(r['cpu_temp']['avg_diff'],5);self.assertIsNone(r['gpu_temp']['avg_diff'])
    def test_window_excludes_outside(self):
        h=hist([{'cpu_temp':t} for t in (10,20,30,40)])
        self.assertEqual(summarize(h.rows,1,2)['metrics']['cpu_temp']['avg'],25)
    def test_warnings(self):
        a=summarize(hist([{'cpu_temp':1}]*10).rows,0,9);b=summarize(hist([{'cpu_temp':1}]*3).rows,0,2)
        w=warnings(a,b);self.assertEqual(w[0],PERMANENT_WARNING);self.assertEqual(len(w),3)
        self.assertEqual(len(warnings(a,a)),1)
        self.assertEqual(len(warnings(summarize([],0,5),a)),2)
    def test_csv_blank(self):
        a=summarize(hist([{'cpu_temp':50}]).rows,0,0)
        with tempfile.TemporaryDirectory() as d:
            p=os.path.join(d,'c.csv');export_csv(p,a,a)
            with open(p,encoding='utf-8') as f:rows=list(csv.reader(f))
        gpu=[r for r in rows if r and r[0]=='NVIDIA gücü'][0]
        self.assertEqual(gpu[2:],['']*6)
    def test_widget(self):
        from PySide6.QtWidgets import QApplication
        QApplication.instance() or QApplication([])
        from slayer_r9t.compare_widget import ComparePanelWidget
        h=History();t=[0.0]
        w=ComparePanelWidget(h,clock=lambda:t[0])
        w.toggle('A');h.add({'cpu_temp':50},now=0.5,utc='x');t[0]=2;w.toggle('A')
        w.toggle('B');t[0]=3;h.add({'cpu_temp':60},now=3,utc='x');t[0]=4;w.toggle('B')
        self.assertEqual(w.table.item(0,3).text(),'10.0')
        self.assertEqual(w.table.item(1,1).text(),'')
        self.assertIn(PERMANENT_WARNING,w.warning.text());self.assertIn('1 örnek',w.info.text())
        w.timer.stop()

    def test_completed_window_survives_history_retention_and_csv_export(self):
        from PySide6.QtWidgets import QApplication
        from slayer_r9t.compare_widget import ComparePanelWidget
        app=QApplication.instance() or QApplication([])
        for retention in ('time', 'count'):
            with self.subTest(retention=retention):
                h=History();t=[0.0];w=ComparePanelWidget(h,clock=lambda:t[0])
                try:
                    w.toggle('A');h.add({'cpu_temp':50},now=0.5,utc='x')
                    t[0]=1;w.toggle('A');a=w.summary('A')
                    w.toggle('B')
                    if retention=='time':
                        t[0]=1900;h.add({'cpu_temp':60},now=t[0],utc='x')
                    else:
                        for i in range(1201):h.add({'cpu_temp':60},now=2+i/10,utc='x')
                        t[0]=123
                    self.assertFalse(any(row['monotonic']==0.5 for row in h.rows))
                    w.refresh();self.assertEqual(w.summary('A'),a)
                    self.assertEqual(w.table.item(0,1).text(),'50.0')
                    w.toggle('B');b=w.summary('B')
                    t[0]=4000;h.add({'cpu_temp':99},now=t[0],utc='x')
                    w.refresh();self.assertEqual(w.summary('B'),b)
                    with tempfile.TemporaryDirectory() as d:
                        p=os.path.join(d,'comparison.csv');w.export_to(p)
                        with open(p,encoding='utf-8') as stream:rows=list(csv.reader(stream))
                    cpu=next(row for row in rows if row and row[0]=='CPU sıcaklığı')
                    self.assertEqual(cpu[2:5],['50.00','60.00','10.00'])
                finally:w.timer.stop();w.close()

    def test_restarting_window_discards_previous_completed_summary(self):
        from PySide6.QtWidgets import QApplication
        from slayer_r9t.compare_widget import ComparePanelWidget
        app=QApplication.instance() or QApplication([])
        h=History();t=[0.0];w=ComparePanelWidget(h,clock=lambda:t[0])
        try:
            w.toggle('A');h.add({'cpu_temp':50},now=0.5,utc='x');t[0]=1;w.toggle('A')
            t[0]=2;w.toggle('A');h.add({'cpu_temp':70},now=2.5,utc='x')
            t[0]=3;w.toggle('A')
            self.assertEqual(w.summary('A')['samples'],1)
            self.assertEqual(w.summary('A')['metrics']['cpu_temp']['avg'],70)
        finally:w.timer.stop();w.close()

    def test_running_window_collects_all_samples_beyond_both_retention_limits(self):
        from PySide6.QtWidgets import QApplication
        from slayer_r9t.compare_widget import ComparePanelWidget
        app=QApplication.instance() or QApplication([])
        h=History();t=[0.0];w=ComparePanelWidget(h,clock=lambda:t[0])
        try:
            w.toggle('A')
            h.add({'cpu_temp':90},now=0.5,utc='x')
            for i in range(1500):h.add({'cpu_temp':50},now=2+i*2,utc='x')
            t[0]=3002
            live=w.summary('A')
            self.assertLess(len(h.rows),1501)
            self.assertEqual(live['samples'],1501)
            self.assertEqual(live['metrics']['cpu_temp']['peak'],90)
            self.assertAlmostEqual(live['metrics']['cpu_temp']['avg'],(90+1500*50)/1501)
            self.assertIsNone(live['metrics']['gpu_temp']['avg'])
            w.toggle('A')
            self.assertEqual(w.summary('A'),live)
            self.assertEqual(len(h.observers),0)
        finally:w.timer.stop();w.close()

    def test_overlapping_windows_and_close_release_collectors(self):
        from PySide6.QtWidgets import QApplication
        from slayer_r9t.compare_widget import ComparePanelWidget
        app=QApplication.instance() or QApplication([])
        h=History();t=[0.0];w=ComparePanelWidget(h,clock=lambda:t[0])
        try:
            w.toggle('A');h.add({'cpu_temp':50},now=0.5,utc='x')
            t[0]=1;w.toggle('B');h.add({'cpu_temp':60},now=1.5,utc='x')
            t[0]=2;w.toggle('A')
            self.assertEqual(w.summary('A')['samples'],2)
            self.assertEqual(w.summary('B')['samples'],1)
            self.assertEqual(len(h.observers),1)
            t[0]=3;w.close()
            self.assertEqual(len(h.observers),0)
            self.assertFalse(w.timer.isActive())
            h.add({'cpu_temp':99},now=4,utc='x')
            self.assertEqual(w.summary('B')['samples'],1)
        finally:w.timer.stop();w.close()

    def test_csv_io_failures_preserve_destination_and_remove_temporary_file(self):
        a=summarize(hist([{'cpu_temp':50}]).rows,0,0)
        for step in ('fsync','replace'):
            for existing in (False,True):
                with self.subTest(step=step,existing=existing),tempfile.TemporaryDirectory() as d:
                    p=Path(d)/'comparison.csv'
                    if existing:p.write_bytes(b'previous report')
                    with patch('slayer_r9t.compare.os.'+step,side_effect=OSError('disk full')):
                        with self.assertRaisesRegex(OSError,'disk full'):export_csv(p,a,a)
                    self.assertEqual(list(Path(d).iterdir()),[p] if existing else [])
                    if existing:self.assertEqual(p.read_bytes(),b'previous report')

    def test_export_dialog_reports_failure_without_losing_measurement(self):
        from PySide6.QtWidgets import QApplication
        from slayer_r9t.compare_widget import ComparePanelWidget
        app=QApplication.instance() or QApplication([])
        h=History();t=[0.0];w=ComparePanelWidget(h,clock=lambda:t[0])
        try:
            for name in 'AB':
                w.toggle(name);h.add({'cpu_temp':50},now=t[0]+0.5,utc='x')
                t[0]+=1;w.toggle(name)
            before=(w.summary('A'),w.summary('B'))
            with patch('slayer_r9t.compare_widget.QFileDialog.getSaveFileName',return_value=('test.csv','CSV')):
                with patch('slayer_r9t.compare_widget.export_csv',side_effect=PermissionError('permission denied')):
                    w.export_dialog()
                self.assertIn('CSV kaydedilemedi',w.export_status.text())
                self.assertIn('permission denied',w.export_status.text())
                self.assertEqual((w.summary('A'),w.summary('B')),before)
                with patch('slayer_r9t.compare_widget.export_csv',return_value=6):w.export_dialog()
                self.assertIn('CSV kaydedildi',w.export_status.text())
        finally:w.timer.stop();w.close()
if __name__=='__main__':unittest.main()
