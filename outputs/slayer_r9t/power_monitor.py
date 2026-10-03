"""Read-only CPU package energy deltas; never sum core subzones twice."""
import math
from pathlib import Path
import re
import time

ROOT = Path('/sys/class/powercap')


class PackagePower:
    def __init__(self, root=ROOT):
        self.root=root
        self.previous={}
        self.last_tick=None
        self.sample_time=None
        self.watts=None
        self.interval=None
        self.zones=[]
        self.error='İlk iki enerji örneği bekleniyor.'

    def tick(self, now=None):
        now=time.monotonic() if now is None else now
        if self.last_tick is not None and now-self.last_tick<1:
            return
        self.last_tick=now
        current={}
        values=[]
        intervals=[]
        errors=[]
        zones=[]
        for entry in sorted(self.root.glob('*')):
            try:
                name=(entry/'name').read_text().strip()
            except OSError:
                continue
            if not re.fullmatch(r'package-\d+',name):
                continue
            zones.append({'name':name,'path':entry.name})
            try:
                energy=int((entry/'energy_uj').read_text())
                ceiling=int((entry/'max_energy_range_uj').read_text())
                if not 0<=energy<ceiling:
                    raise ValueError('Enerji sayacı aralık dışında.')
                key=str(entry.resolve())
                # Symlink aliases of the same package must not be double counted.
                if key in current:
                    zones.pop()
                    continue
                current[key]=(energy,ceiling,now)
                previous=self.previous.get(key)
                if previous is None or previous[1]!=ceiling:
                    errors.append('İlk iki enerji örneği bekleniyor.')
                    continue
                elapsed=now-previous[2]
                if not 1<=elapsed<=10:
                    errors.append('Enerji örnekleme aralığı geçersiz veya eski.')
                    continue
                delta=(energy-previous[0]) % ceiling
                watts=delta/1000000/elapsed
                if not math.isfinite(watts) or not 0<=watts<=500:
                    raise ValueError('Paket güç örneği geçersiz; sayaç sıfırlanmış olabilir.')
                values.append(watts);intervals.append(elapsed)
            except (OSError,ValueError) as exc:
                errors.append(str(exc))
        self.previous=current
        self.zones=zones
        self.watts=sum(values) if values and not errors and len(values)==len(current) else None
        self.interval=max(intervals) if self.watts is not None else None
        self.sample_time=now
        self.error='; '.join(dict.fromkeys(errors)) if errors else ('' if zones else 'CPU paket enerji sayacı bulunamadı.')

    def status(self, now=None):
        now=time.monotonic() if now is None else now
        age=now-self.sample_time if self.sample_time is not None else None
        fresh=age is not None and 0<=age<=5
        return {'watts':self.watts if fresh else None,'interval_seconds':self.interval if fresh else None,
                'sample_monotonic':self.sample_time,'age_seconds':age,'zones':self.zones,
                'source':'CPU package energy_uj delta (RAPL)','error':self.error if fresh else 'CPU güç örneği eski veya henüz yok.'}
