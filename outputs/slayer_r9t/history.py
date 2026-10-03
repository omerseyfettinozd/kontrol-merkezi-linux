"""Bounded session telemetry with real timestamps and missing-value gaps."""
import csv
import math
import time
from collections import deque
from datetime import datetime,timezone

METRICS={
    'cpu_temp':('CPU sıcaklığı','°C','#50c9ba',110),
    'gpu_temp':('GPU sıcaklığı','°C','#a596dd',110),
    'cpu_rpm':('CPU fanı','RPM','#50c9ba',6000),
    'gpu_rpm':('GPU fanı','RPM','#a596dd',6000),
    'cpu_usage':('CPU kullanımı','%','#70a9ec',100),
    'gpu_usage':('GPU kullanımı','%','#a596dd',100),
    'cpu_power':('CPU paket gücü','W','#50c9ba',100),
    'gpu_power':('GPU tüketimi','W','#dfb477',150),
    'cpu_mhz':('CPU frekansı','MHz','#70a9ec',6000),
    'gpu_mhz':('GPU frekansı','MHz','#a596dd',4000),
    'battery_power':('Pil şarj/deşarj gücü','W','#dfb477',100),
}

class History:
    def __init__(self):self.rows=deque(maxlen=1200)

    def add(self,values,now=None,utc=None):
        now=time.monotonic() if now is None else now
        clean={k:(float(v) if type(v) in (float,int) and math.isfinite(v) else None) for k,v in values.items() if k in METRICS}
        self.rows.append({'monotonic':now,'utc':utc or datetime.now(timezone.utc).isoformat(timespec='milliseconds'),**{k:clean.get(k) for k in METRICS}})
        while self.rows and self.rows[0]['monotonic']<now-1800:self.rows.popleft()

    def window(self,seconds,now=None):
        now=time.monotonic() if now is None else now
        return [r for r in self.rows if now-seconds<=r['monotonic']<=now]

    def export(self,path,seconds,now=None):
        rows=self.window(seconds,now)
        with open(path,'w',newline='',encoding='utf-8') as stream:
            writer=csv.DictWriter(stream,fieldnames=['utc',*METRICS])
            writer.writeheader()
            writer.writerows({k:r.get(k) for k in ['utc',*METRICS]} for r in rows)
        return len(rows)
