"""Read-only Linux telemetry. Unsupported sensors are explicitly absent."""
from pathlib import Path
HWMON=Path('/sys/class/hwmon')

def storage_memory_sensors():
    result=[]
    for base in sorted(HWMON.glob('hwmon*'),key=lambda p:str(p.resolve())):
        try:driver=(base/'name').read_text().strip()
        except OSError:continue
        if driver not in ('nvme','spd5118'):continue
        device=str((base/'device').resolve())
        name=Path(device).name
        for sensor in sorted(base.glob('temp*_input')):
            try:
                value=int(sensor.read_text())/1000
                label=base/(sensor.stem.replace('_input','')+'_label')
                title=label.read_text().strip() if label.exists() else sensor.stem.replace('_input','')
                result.append({'id':device+':'+sensor.stem,'name':('SSD' if driver=='nvme' else 'RAM')+' '+name+' · '+title,'driver':driver,'celsius':value})
            except (OSError,ValueError):continue
    return result

def cpu_frequency():
    values=[]
    for p in Path('/sys/devices/system/cpu/cpufreq').glob('policy*/scaling_cur_freq'):
        try:values.append(int(p.read_text())/1000)
        except (OSError,ValueError):pass
    return sum(values)/len(values) if values else None


def temperature(driver):
    for entry in Path('/sys/class/hwmon').glob('hwmon*/name'):
        try:
            if entry.read_text().strip() == driver:
                return int((entry.parent/'temp1_input').read_text())/1000
        except (OSError, ValueError):
            pass
    return None


def memory():
    try:
        values = {line.split(':')[0]: int(line.split()[1]) for line in Path('/proc/meminfo').read_text().splitlines()}
        return {'used_gib': (values['MemTotal']-values['MemAvailable'])/1048576,
                'total_gib': values['MemTotal']/1048576}
    except (OSError, ValueError, KeyError):
        return None


def battery():
    for base in Path('/sys/class/power_supply').glob('BAT*'):
        try:
            result = {'capacity': int((base/'capacity').read_text()), 'status': (base/'status').read_text().strip()}
            for prefix in ('energy', 'charge'):
                full, design = base/(prefix+'_full'), base/(prefix+'_full_design')
                if full.exists() and design.exists():
                    result['health'] = round(int(full.read_text())*100/int(design.read_text()), 1)
                    break
            cycles = base/'cycle_count'
            if cycles.exists():
                result['cycles'] = int(cycles.read_text())
            voltage=base/'voltage_now';current=base/'current_now';power=base/'power_now'
            if voltage.exists():result['voltage_v']=int(voltage.read_text())/1000000
            if current.exists():result['current_a']=int(current.read_text())/1000000
            if power.exists():result['power_w']=int(power.read_text())/1000000
            elif 'current_a' in result and 'voltage_v' in result:result['power_w']=round(result['current_a']*result['voltage_v'],2)
            return result
        except (OSError, ValueError, ZeroDivisionError):
            pass
    return None


class CpuUsage:
    def __init__(self):
        self.previous = None

    def read(self):
        try:
            fields = list(map(int, Path('/proc/stat').read_text().splitlines()[0].split()[1:]))
            idle, total = fields[3]+fields[4], sum(fields[:8])
            previous, self.previous = self.previous, (idle, total)
            if previous is None or total == previous[1]:
                return None
            return max(0, min(100, 100*(1-(idle-previous[0])/(total-previous[1]))))
        except (OSError, ValueError, IndexError):
            return None
