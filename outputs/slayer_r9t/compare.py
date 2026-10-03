"""Pure logic for A/B profile comparison over History samples (read-only, no hardware access)."""
import csv
import math

COMPARE_METRICS=[
    ('cpu_temp','CPU sıcaklığı','°C'),
    ('gpu_temp','GPU sıcaklığı','°C'),
    ('cpu_rpm','CPU fanı','RPM'),
    ('gpu_rpm','GPU fanı','RPM'),
    ('cpu_power','CPU paket gücü','W'),
    ('gpu_power','NVIDIA gücü','W'),
]
PERMANENT_WARNING='Uyarı: İş yükü kontrol edilmez ve FPS ölçülmez; sonuçlar yalnız bilgi amaçlıdır, profil performansını kanıtlamaz.'
DURATION_RATIO=1.5
COUNT_RATIO=1.5

def _valid(v):
    return type(v) in (int,float) and math.isfinite(v)

def window_rows(rows,start,end):
    """Samples whose monotonic stamp lies in [start,end]."""
    return [r for r in rows if start<=r.get('monotonic',float('nan'))<=end]

def summarize(rows,start,end):
    """Summary of one window: duration, sample count and per-metric avg/peak/count (None when missing)."""
    sel=window_rows(rows,start,end)
    out={'duration':max(0.0,end-start),'samples':len(sel),'metrics':{}}
    for key,_,_ in COMPARE_METRICS:
        vals=[r[key] for r in sel if _valid(r.get(key))]
        out['metrics'][key]={'avg':sum(vals)/len(vals) if vals else None,'peak':max(vals) if vals else None,'count':len(vals)}
    return out

def _diff(a,b):
    return None if a is None or b is None else b-a

def compare(a,b):
    """Side-by-side rows with B-A difference; missing values stay None."""
    rows=[]
    for key,title,unit in COMPARE_METRICS:
        ma,mb=a['metrics'][key],b['metrics'][key]
        rows.append({'key':key,'title':title,'unit':unit,'a_avg':ma['avg'],'b_avg':mb['avg'],'avg_diff':_diff(ma['avg'],mb['avg']),
                     'a_peak':ma['peak'],'b_peak':mb['peak'],'peak_diff':_diff(ma['peak'],mb['peak'])})
    return rows

def warnings(a,b):
    """Always includes the permanent warning; adds notes for empty or very unequal windows."""
    out=[PERMANENT_WARNING]
    if a['samples']==0 or b['samples']==0:
        out.append('Uyarı: Pencerelerden en az birinde örnek yok; karşılaştırma yapılamaz.')
        return out
    da,db=a['duration'],b['duration']
    if min(da,db)<=0 or max(da,db)/min(da,db)>DURATION_RATIO:
        out.append(f'Uyarı: A ({da:.0f} sn) ve B ({db:.0f} sn) süreleri çok farklı.')
    if max(a['samples'],b['samples'])/min(a['samples'],b['samples'])>COUNT_RATIO:
        out.append(f"Uyarı: A ({a['samples']}) ve B ({b['samples']}) örnek sayıları çok farklı.")
    return out

def fmt(v,digits=1):
    return '' if v is None else f'{v:.{digits}f}'

def export_csv(path,a,b):
    """Write comparison table; empty cell for missing values. Returns written metric row count."""
    rows=compare(a,b)
    with open(path,'w',newline='',encoding='utf-8') as s:
        w=csv.writer(s)
        w.writerow(['metric','unit','a_avg','b_avg','avg_diff','a_peak','b_peak','peak_diff'])
        for r in rows:
            w.writerow([r['title'],r['unit'],*[fmt(r[k],2) for k in ('a_avg','b_avg','avg_diff','a_peak','b_peak','peak_diff')]])
        w.writerow([]);w.writerow(['window','duration_s','samples'])
        w.writerow(['A',fmt(a['duration'],1),a['samples']]);w.writerow(['B',fmt(b['duration'],1),b['samples']])
        w.writerow([]);w.writerow(['note',PERMANENT_WARNING])
    return len(rows)
