#!/usr/bin/env python3
"""Faz 0.6: 126 RGB kanalını tek tek yakıp hangi tuşa denk geldiğini kaydeder.

    sudo python3 -I r9t-studio/tools/keymap-calibrate.py [--start 0]

Başlangıçta 126 LED'in multi_intensity ve global brightness değeri kaydedilir; her çıkışta
(Ctrl-C dahil) geri yüklenir ve buffer_input=0 yazılır. Her adımda yalnız bir kanal beyaz
yanar, diğerleri söner. Yanıt: tuş etiketi (ör. "Ş", "F5", "Sol Shift"), boş = LED yok,
"g" = geri, "q" = kaydet ve çık. İlerleme her yanıtta data/keymap-tr-q.json'a yazılır,
--start ile kalınan yerden devam edilir.
"""
import argparse
import glob
import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'data/keymap-tr-q.json'
ROWS, COLS = 6, 21


def leds():
    found = [p for p in Path('/sys/class/leds').glob('rgb:kbd_backlight*') if (p / 'multi_intensity').is_file()]
    found.sort(key=lambda p: 0 if p.name == 'rgb:kbd_backlight' else int(p.name.rsplit('_', 1)[1]))
    if len(found) != ROWS * COLS:
        raise SystemExit(f'{len(found)} LED bulundu, 126 bekleniyor')
    return found


def frame(entries, ctl, colors):
    ctl.write_text('1')
    try:
        for entry, color in zip(entries, colors):
            (entry / 'multi_intensity').write_text(color)
    finally:
        ctl.write_text('0')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--start', type=int, default=None)
    args = parser.parse_args()
    if os.geteuid() != 0:
        raise SystemExit('root gerekli')
    entries = leds()
    controls = glob.glob('/sys/bus/hid/devices/0003:048D:600B.*/controls/buffer_input')
    if len(controls) != 1:
        raise SystemExit('buffer_input bulunamadı')
    ctl = Path(controls[0])
    saved = [(e / 'multi_intensity').read_text().strip() for e in entries]
    brightness_file = entries[0] / 'brightness'
    saved_brightness = brightness_file.read_text().strip()
    doc = json.loads(OUT.read_text()) if OUT.exists() else None
    keys = doc['keys'] if doc else [{'index': i, 'row': i // COLS, 'col': i % COLS, 'label': None, 'led': None}
                                     for i in range(ROWS * COLS)]
    start = args.start if args.start is not None else next((k['index'] for k in keys if k['led'] is None), ROWS * COLS)
    off = ['0 0 0'] * len(entries)
    try:
        brightness_file.write_text('50')
        i = start
        while i < ROWS * COLS:
            colors = list(off)
            colors[i] = '255 255 255'
            frame(entries, ctl, colors)
            answer = input(f'[{i:3d}] satır {i // COLS} sütun {i % COLS} — yanan tuş (boş=LED yok, g=geri, q=çık): ').strip()
            if answer == 'q':
                break
            if answer == 'g':
                i = max(0, i - 1)
                continue
            keys[i]['label'] = answer or None
            keys[i]['led'] = bool(answer)
            save(keys)
            i += 1
    finally:
        frame(entries, ctl, saved)
        brightness_file.write_text(saved_brightness)
        save(keys)
        print('Renkler ve parlaklık geri yüklendi; kayıt:', OUT)


def save(keys):
    done = sum(k['led'] is not None for k in keys)
    doc = {'version': 1, 'layout': 'tr-q', 'grid': {'rows': ROWS, 'cols': COLS, 'channel': 'row*21+col'},
           'verified': done == ROWS * COLS, 'calibrated_count': done, 'keys': keys}
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(doc, ensure_ascii=False, indent=1) + '\n')
    uid = int(os.environ.get('SUDO_UID', '0'))
    if uid:
        os.chown(OUT, uid, int(os.environ.get('SUDO_GID', uid)))


if __name__ == '__main__':
    try:
        main()
    except KeyboardInterrupt:
        sys.exit(130)
