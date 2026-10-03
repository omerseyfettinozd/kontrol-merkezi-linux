import csv,os,tempfile,unittest
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
if __name__=='__main__':unittest.main()
