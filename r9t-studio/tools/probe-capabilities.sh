#!/usr/bin/env bash
# Faz 0.5: yetenek yoklaması. Varsayılan çalışma SALT OKUNURDUR.
#
#   sudo r9t-studio/tools/probe-capabilities.sh                # yalnız okuma
#   sudo r9t-studio/tools/probe-capabilities.sh --rgb-timing   # + RGB kare süresi ölçümü
#
# --rgb-timing: 126 LED'in MEVCUT multi_intensity değerlerini aynen yeniden yazar
# (renk değişmez), arabelleksiz ve buffer_input ile süreyi ölçer, her çıkışta
# buffer_input=0 yazar ve sonunda değerlerin değişmediğini geri okur.
# brightness'a, fana, cTGP'ye ve EC'ye hiçbir koşulda yazılmaz.
#
# Çıktı: r9t-studio/docs/06_yetenekler.json (sahibi sudo'yu çağıran kullanıcı).
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    echo "Bu betik root ister: sudo $0 [--rgb-timing]" >&2
    exit 1
fi
if [[ -z "${SUDO_UID:-}" || "${SUDO_UID}" == 0 ]]; then
    echo "sudo ile normal kullanıcıdan çalıştırın (SUDO_UID gerekli)." >&2
    exit 1
fi
rgb_timing=0
for arg in "$@"; do
    case "$arg" in
        --rgb-timing) rgb_timing=1 ;;
        *) echo "Bilinmeyen seçenek: $arg" >&2; exit 2 ;;
    esac
done

here="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
out="$here/../docs/06_yetenekler.json"

/usr/bin/python3 -I - "$out" "$rgb_timing" <<'PY'
import ctypes, fcntl, glob, json, os, re, shutil, statistics, struct, subprocess, sys, time
from pathlib import Path

out = Path(sys.argv[1])
RGB_TIMING = sys.argv[2] == '1'
R = {'generated_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()), 'rgb_timing_requested': RGB_TIMING}

def rd(path, limit=4096):
    try:
        return Path(path).read_text()[:limit].strip()
    except OSError as exc:
        return f'<hata: {type(exc).__name__}: {exc.strerror or exc}>'

def section(name):
    def wrap(fn):
        try:
            R[name] = fn()
        except Exception as exc:  # her bölüm bağımsız; biri düşerse diğerleri yazılır
            R[name] = {'error': f'{type(exc).__name__}: {exc}'}
        return fn
    return wrap

@section('system')
def _():
    return {'kernel': os.uname().release,
            'dmi': {k: rd(f'/sys/class/dmi/id/{k}') for k in ('sys_vendor', 'product_name', 'bios_version')},
            'platform_profile_exists': Path('/sys/firmware/acpi/platform_profile').exists()}

@section('ryzenadj')
def _():
    exe = shutil.which('ryzenadj')
    if not exe:
        return {'available': False, 'reason': 'ryzenadj bulunamadı'}
    p = subprocess.run([exe, '-i'], capture_output=True, text=True, timeout=20)
    parsed = {}
    for line in p.stdout.splitlines():
        m = re.match(r'\|\s*([^|]+?)\s*\|\s*([-0-9.]+|nan)\s*\|\s*([^|]*?)\s*\|', line)
        if m:
            parsed[m.group(1)] = {'value': m.group(2), 'param': m.group(3)}
    return {'available': True, 'returncode': p.returncode, 'parsed': parsed,
            'stdout': p.stdout[:6000], 'stderr': p.stderr[:2000]}

@section('rapl')
def _():
    zones = []
    for entry in sorted(Path('/sys/class/powercap').glob('*')):
        z = {'path': entry.name, 'name': rd(entry/'name'), 'enabled': rd(entry/'enabled')}
        st = (entry/'energy_uj')
        if st.exists():
            z['energy_uj_mode'] = oct(st.stat().st_mode & 0o777)
            z['max_energy_range_uj'] = rd(entry/'max_energy_range_uj')
        constraints = {}
        for f in sorted(entry.glob('constraint_*')):
            constraints[f.name] = rd(f)
        z['constraints'] = constraints
        zones.append(z)
    return zones

@section('cpufreq')
def _():
    policies = sorted(Path('/sys/devices/system/cpu/cpufreq').glob('policy*'), key=lambda p: int(p.name[6:]))
    def uniq(name):
        return sorted({rd(p/name) for p in policies})
    return {'policy_count': len(policies),
            **{n: uniq(n) for n in ('scaling_driver', 'scaling_governor', 'energy_performance_preference',
                                     'energy_performance_available_preferences', 'boost', 'cpuinfo_max_freq',
                                     'amd_pstate_max_freq', 'scaling_max_freq', 'scaling_min_freq',
                                     'amd_pstate_highest_perf', 'amd_pstate_prefcore_ranking')}}

@section('r9t_fan')
def _():
    base = Path('/sys/module/r9t_fan')
    res = {'loaded': base.exists(), 'srcversion': rd(base/'srcversion'), 'version': rd(base/'version')}
    times, values = [], None
    for _ in range(2):
        t0 = time.perf_counter()
        text = (base/'parameters/status').read_text().strip()
        times.append(round((time.perf_counter()-t0)*1000, 1))
        values = dict(kv.split('=', 1) for kv in text.split())
        time.sleep(1)
    res.update(status=values, status_read_ms=times)
    try:
        res['dkms'] = subprocess.run(['dkms', 'status'], capture_output=True, text=True, timeout=20).stdout.strip()[:2000]
    except (OSError, subprocess.SubprocessError) as exc:
        res['dkms'] = {'error': str(exc)}
    return res

# --- tuxedo_io: ÖNCE hwcheck (değişmez 9) ---
PTR = struct.calcsize('P')
def ior(group, number):
    return (2 << 30) | (PTR << 16) | (group << 8) | number

def ioctl_int(fd, group, number):
    buf = bytearray(4)
    t0 = time.perf_counter()
    fcntl.ioctl(fd, ior(group, number), buf, True)
    return struct.unpack('<i', buf)[0], (time.perf_counter()-t0)*1000

TUX = {'fd': None}

@section('tuxedo_io')
def _():
    res = {}
    try:
        fd = os.open('/dev/tuxedo_io', os.O_RDONLY | os.O_CLOEXEC); res['open_mode'] = 'O_RDONLY'
    except OSError:
        fd = os.open('/dev/tuxedo_io', os.O_RDWR | os.O_CLOEXEC); res['open_mode'] = 'O_RDWR'
    hw, ms = ioctl_int(fd, 0xEC, 0x06)
    res['hwcheck_uw'] = hw
    res['hwcheck_ms'] = round(ms, 1)
    if hw != 1:
        os.close(fd)
        res['stopped'] = 'hwcheck != 1; başka ioctl çağrılmadı'
        return res
    res['model_id'] = hex(ioctl_int(fd, 0xEF, 0x01)[0])
    iface = bytearray(32)
    fcntl.ioctl(fd, ior(0xEF, 0x00), iface, True)
    res['interface'] = iface.split(b'\0', 1)[0].decode(errors='replace')
    reads = {}
    for name, num in (('cpu_duty', 0x10), ('gpu_duty', 0x11), ('cpu_temp', 0x12), ('gpu_temp', 0x13), ('mode', 0x14)):
        value, ms = ioctl_int(fd, 0xEF, num)
        reads[name] = {'value': value, 'ms': round(ms, 1)}
    res['reads'] = reads
    if res['model_id'] == hex(0x1A) and 'uniwill_wmi' in res['interface']:
        TUX['fd'] = fd
    else:
        os.close(fd)
        res['stopped'] = 'model/arayüz beklenen değil; örnekleme yapılmayacak'
    return res

# --- dGPU güç durumu ve NVML (yalnız runtime_status == active iken) ---
GPU = Path('/sys/bus/pci/devices/0000:01:00.0')

@section('dgpu_power')
def _():
    p = GPU/'power'
    return {k: rd(p/k) for k in ('runtime_status', 'control', 'runtime_active_time', 'runtime_suspended_time')} | \
           {'d3cold_allowed': rd(GPU/'d3cold_allowed'), 'boot_vga': rd(GPU/'boot_vga')}

class Nvml:
    def __init__(self):
        self.lib = ctypes.CDLL('libnvidia-ml.so.1')
        self.h = ctypes.c_void_p()
        if self.lib.nvmlInit_v2():
            raise RuntimeError('nvmlInit başarısız')
        if self.lib.nvmlDeviceGetHandleByPciBusId_v2(b'00000000:01:00.0', ctypes.byref(self.h)):
            self.lib.nvmlShutdown(); raise RuntimeError('NVML aygıt tutamacı alınamadı')
    def temp(self):
        v = ctypes.c_uint()
        return v.value if self.lib.nvmlDeviceGetTemperature(self.h, 0, ctypes.byref(v)) == 0 else None
    def close(self):
        self.lib.nvmlShutdown()

def k10temp_tctl():
    for hw in Path('/sys/class/hwmon').glob('hwmon*'):
        if rd(hw/'name') == 'k10temp':
            for t in hw.glob('temp*_label'):
                if rd(t) == 'Tctl':
                    return int(rd(t.with_name(t.name.replace('_label', '_input')))) / 1000
    return None

@section('ec_vs_host_temperature')
def _():
    if TUX['fd'] is None:
        return {'skipped': 'tuxedo_io doğrulanmadı'}
    nv, nvml_error = None, None
    if rd(GPU/'power/runtime_status') == 'active':
        try:
            nv = Nvml()
        except Exception as exc:
            nvml_error = str(exc)
    samples = []
    try:
        for _ in range(10):
            cpu_ec, _ms = ioctl_int(TUX['fd'], 0xEF, 0x12)
            gpu_ec, _ms = ioctl_int(TUX['fd'], 0xEF, 0x13)
            samples.append({'ec_cpu': cpu_ec, 'tctl': k10temp_tctl(), 'ec_gpu': gpu_ec,
                            'nvml_gpu': nv.temp() if nv else None})
            time.sleep(1)
    finally:
        if nv:
            nv.close()
        os.close(TUX['fd']); TUX['fd'] = None
    def diff(a, b):
        d = [s[a]-s[b] for s in samples if s[a] is not None and s[b] is not None and 1 <= s[a] <= 115]
        return {'mean': round(statistics.mean(d), 2), 'min': round(min(d), 2), 'max': round(max(d), 2), 'n': len(d)} if d else None
    return {'samples': samples, 'ec_cpu_minus_tctl': diff('ec_cpu', 'tctl'), 'ec_gpu_minus_nvml': diff('ec_gpu', 'nvml_gpu'),
            'nvml_used': nv is not None, 'nvml_error': nvml_error}

# --- RGB: yollar ve (isteğe bağlı) kare süresi ---
def leds():
    found = [p for p in Path('/sys/class/leds').glob('rgb:kbd_backlight*') if (p/'multi_intensity').is_file()]
    return sorted(found, key=lambda p: 0 if p.name == 'rgb:kbd_backlight' else int(p.name.rsplit('_', 1)[1]))

def buffer_control():
    paths = glob.glob('/sys/bus/hid/devices/0003:048D:600B.*/controls/buffer_input')
    return Path(paths[0]) if len(paths) == 1 else None

@section('rgb')
def _():
    entries = leds()
    ctl = buffer_control()
    return {'led_count': len(entries),
            'driver': os.path.basename(os.path.realpath(entries[0]/'device/driver')) if entries else None,
            'multi_max_intensity': sorted({rd(e/'multi_max_intensity') for e in entries}),
            'max_brightness': sorted({rd(e/'max_brightness') for e in entries}),
            'brightness': sorted({rd(e/'brightness') for e in entries}),
            'buffer_input_path': str(ctl) if ctl else None,
            'buffer_input_value': rd(ctl) if ctl else None}

@section('rgb_timing')
def _():
    if not RGB_TIMING:
        return {'skipped': '--rgb-timing verilmedi'}
    entries = leds()
    ctl = buffer_control()
    if len(entries) != 126 or ctl is None:
        return {'skipped': '126 LED ya da buffer_input bulunamadı'}
    saved = [(e/'multi_intensity').read_text().strip() for e in entries]
    def frame():
        t0 = time.perf_counter()
        for e, v in zip(entries, saved):
            with open(e/'multi_intensity', 'w') as f:
                f.write(v)
        return (time.perf_counter()-t0)*1000
    unbuffered, buffered = [], []
    try:
        for _ in range(3):
            unbuffered.append(frame())
            time.sleep(0.2)
        for _ in range(3):
            t0 = time.perf_counter()
            ctl.write_text('1')
            frame()
            ctl.write_text('0')
            buffered.append((time.perf_counter()-t0)*1000)
            time.sleep(0.2)
    finally:
        try:
            ctl.write_text('0')
        except OSError:
            pass
    after = [(e/'multi_intensity').read_text().strip() for e in entries]
    unchanged = after == saved
    return {'unbuffered_ms': [round(x, 1) for x in unbuffered], 'buffered_ms': [round(x, 1) for x in buffered],
            'values_unchanged': unchanged, 'buffer_input_final': rd(ctl)}

@section('features')
def _():
    bat = Path('/sys/class/power_supply/BAT0')
    return {'fn_lock': rd('/sys/devices/platform/tuxedo_keyboard/fn_lock'),
            'ctgp_offset': rd('/sys/devices/platform/tuxedo_nvidia_power_ctrl/ctgp_offset'),
            'bat0_files': sorted(p.name for p in bat.iterdir()) if bat.is_dir() else None,
            'charge_threshold_present': (bat/'charge_control_end_threshold').exists()}

out.write_text(json.dumps(R, ensure_ascii=False, indent=2) + '\n')
os.chown(out, int(os.environ['SUDO_UID']), int(os.environ.get('SUDO_GID', os.environ['SUDO_UID'])))
os.chmod(out, 0o644)
errors = [k for k, v in R.items() if isinstance(v, dict) and 'error' in v]
print(f'Yazıldı: {out}' + (f' (hatalı bölümler: {", ".join(errors)})' if errors else ''))
PY
