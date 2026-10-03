"""Bounded, unprivileged GPU evidence without NVML or opening device nodes."""
from datetime import datetime, timezone
import os
from pathlib import Path
import re
import time

DRM = Path('/sys/class/drm')
PROC = Path('/proc')
PCI = Path('/sys/bus/pci/devices')
NVIDIA = PROC/'driver/nvidia/gpus'


def read(path):
    try:
        with path.open() as stream:
            return stream.read(8192).strip()
    except (OSError, UnicodeError):
        return None


def power(device):
    result = {key: read(device/'power'/key) for key in ('runtime_status', 'control', 'runtime_active_time', 'runtime_suspended_time')}
    for key in ('runtime_active_time', 'runtime_suspended_time'):
        value = result[key]
        result[key] = int(value) if value is not None and value.isdigit() else None
    return result


def process_start(entry):
    text = read(entry/'stat')
    try:
        return text.rsplit(')', 1)[1].split()[19] if text is not None else None
    except IndexError:
        return None


def device_users(nodes, proc=PROC, budget=1.5):
    result = []
    coverage = {'permission_denied': 0, 'vanished': 0, 'fd_truncated': 0, 'process_truncated': False, 'timed_out': False}
    deadline = time.monotonic()+budget
    try:
        entries = sorted((p for p in proc.iterdir() if p.name.isdigit()), key=lambda p: int(p.name))
    except OSError:
        coverage['process_truncated'] = True
        return result, coverage
    coverage['process_truncated'] = len(entries)>4096
    for entry in entries[:4096]:
        if time.monotonic()>deadline:
            coverage['timed_out'] = True
            break
        start = process_start(entry)
        found = set()
        try:
            with os.scandir(entry/'fd') as descriptors:
                for index, fd in enumerate(descriptors):
                    if index>=512:
                        coverage['fd_truncated'] += 1
                        break
                    if time.monotonic()>deadline:
                        coverage['timed_out'] = True
                        break
                    try:
                        target = os.readlink(fd.path)
                        if target in nodes:
                            found.add(target)
                    except PermissionError:
                        coverage['permission_denied'] += 1
                        break
                    except OSError:
                        continue
        except PermissionError:
            coverage['permission_denied'] += 1
        except OSError:
            coverage['vanished'] += 1
        if found:
            name = read(entry/'comm') or 'Bilinmiyor'
            if start is None or start != process_start(entry):
                coverage['vanished'] += 1
                continue
            name = ''.join(c for c in name if c.isprintable())[:120]
            result.append({'pid': int(entry.name), 'name': name, 'nodes': sorted(found)})
            if len(result)>=256:
                coverage['process_truncated'] = True
                break
    return result, coverage


def snapshot():
    cards = []
    for entry in sorted(DRM.glob('card*')):
        if not re.fullmatch(r'card\d+', entry.name):
            continue
        device = entry/'device'
        vendor = read(device/'vendor')
        if vendor not in ('0x10de', '0x1002', '0x8086'):
            continue
        before = power(device)
        address = device.resolve().name
        nodes = {f'/dev/dri/{entry.name}'}
        for render in DRM.glob('renderD*'):
            if re.fullmatch(r'renderD\d+', render.name) and (render/'device').resolve() == device.resolve():
                nodes.add(f'/dev/dri/{render.name}')
        connectors = []
        for connector in sorted(DRM.glob(entry.name+'-*')):
            connectors.append({'name': connector.name[len(entry.name)+1:],
                               'status': read(connector/'status'), 'enabled': read(connector/'enabled')})
        fields = {}
        if vendor=='0x10de' and re.fullmatch(r'[0-9a-fA-F]{4}:[0-9a-fA-F]{2}:[0-9a-fA-F]{2}\.[0-7]', address):
            information = read(NVIDIA/address/'information') or ''
            for line in information.splitlines():
                key, sep, value = line.partition(':')
                if sep and key.strip()=='Device Minor' and value.strip().isdigit():
                    nodes.add('/dev/nvidia'+value.strip())
            text = read(NVIDIA/address/'power')
            if text:
                # Keep the flat feature labels; use section-qualified S0ix status.
                section = ''
                for line in text.splitlines():
                    if line.strip()=='S0ix Power Management:':
                        section = 'S0ix '
                        continue
                    key, sep, value = line.partition(':')
                    if sep and value.strip():
                        fields[(section+key.strip()) if key.strip() in ('Status','Platform Support') else key.strip()] = value.strip()
        siblings = []
        if re.fullmatch(r'[0-9a-fA-F]{4}:[0-9a-fA-F]{2}:[0-9a-fA-F]{2}\.[0-7]', address):
            for sibling in sorted(PCI.glob(address.rsplit('.',1)[0]+'.*')):
                if sibling.name != address:
                    siblings.append({'address': sibling.name, 'class': read(sibling/'class'), **power(sibling)})
        cards.append({'card':entry.name, 'pci':address, 'vendor':vendor, 'nodes':sorted(nodes),
                      'connectors':connectors, 'power_before':before, 'driver_power':fields,
                      'siblings':siblings})
    cards.sort(key=lambda card: (card['vendor']!='0x10de', card['card']))
    shared = {'/dev/nvidiactl', '/dev/nvidia-uvm', '/dev/nvidia-uvm-tools'} if any(c['vendor']=='0x10de' for c in cards) else set()
    nodes = shared | {node for card in cards for node in card['nodes']}
    users, coverage = device_users(nodes, proc=PROC) if nodes else ([], {})
    for card in cards:
        card['users'] = [dict(user, nodes=[node for node in user['nodes'] if node in card['nodes']])
                         for user in users if set(user['nodes']) & set(card['nodes'])]
        card['power_after'] = power(DRM/card['card']/'device')
        status = card['power_after']['runtime_status']
        active_outputs = [c['name'] for c in card['connectors'] if c['status']=='connected' and c['enabled']=='enabled']
        card['evidence'] = []
        if active_outputs:
            card['evidence'].append('Etkin ekran bağlantısı: '+', '.join(active_outputs)+'.')
        if card['power_after']['control']=='on':
            card['evidence'].append('Runtime güç politikası on: otomatik askıya alma kapalı.')
        if card['users']:
            card['evidence'].append('Bu GPU aygıtını açık tutan süreçler var; açık dosya tek başına GPU iş yükü kanıtı değildir.')
        if not card['evidence']:
            card['evidence'].append('Görünen dosyalardan kesin neden belirlenemedi; süreç görünürlüğü ve diğer PCI işlevleri de kontrol edilmeli.')
        before = card['power_before']['runtime_status']
        card['observation'] = ('İki okumada da askıda; aradaki geçişler ölçülmedi.' if before==status=='suspended' else
                               'Örnekleme sırasında etkinleşti; uyandıran işlem belirlenemedi.' if before=='suspended' and status=='active' else
                               'GPU zaten etkin veya durum belirsiz; uyandırma etkisi bu örnekten kanıtlanamaz.')
    return {'time':datetime.now(timezone.utc).isoformat(timespec='seconds'), 'cards':cards,
            'shared_users':[dict(user,nodes=[n for n in user['nodes'] if n in shared]) for user in users if set(user['nodes']) & shared],
            'coverage':coverage, 'method':'sysfs/procfs; GPU device nodes are not opened; no NVML or nvidia-smi',
            'wake_effect_verified':False}


if __name__=='__main__':
    import json
    print(json.dumps(snapshot(), ensure_ascii=False))
