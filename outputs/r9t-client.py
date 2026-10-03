#!/usr/bin/env python3
"""Unprivileged client for the restricted local hardware service."""
import json
import sys
from slayer_r9t.protocol import request


def main(args):
    op = args[0]
    if op in ('status', 'auto', 'events', 'fan_boost','automation_resume','session_ready','temperature_target_stop'): data = {'op': op}
    elif op=='temperature_target':data={'op':op,'cpu':int(args[1]),'gpu':int(args[2])}
    elif op=='configure':data={'op':op,'document':json.loads(args[1])}
    elif op in ('fn_lock','camera'):
        if args[1] not in ('0','1'):raise ValueError('Fn Lock 0/1 olmalı.')
        data={'op':op,'enabled':args[1]=='1'}
    elif op=='charge_limit':data={'op':op,'end':int(args[1]),'start':int(args[2]) if len(args)>2 else None}
    elif op == 'set': data = {'op': 'manual', 'cpu': int(args[1]), 'gpu': int(args[2] if len(args)>2 else args[1])}
    elif op == 'curve': data = {'op': 'curve', 'points': json.loads(args[1])}
    elif op == 'fan_preset': data={'op':'fan_preset','preset':args[1]}
    elif op == 'thermal_preset': data={'op':'thermal_preset','preset':args[1]}
    elif op == 'profile': data={'op':'profile','settings':json.loads(args[1])}
    elif op == 'power': data={'op':'power','profile':args[1]}
    elif op == 'boost':
        if args[1] not in ('0','1'):raise ValueError('Boost 0/1 olmalı.')
        data={'op':'boost','enabled':args[1]=='1'}
    elif op == 'rgb': data = {'op': 'rgb', 'rgb': json.loads(args[1]), 'brightness': int(args[2])}
    else: raise ValueError('Bilinmeyen komut.')
    print(json.dumps(request(data), ensure_ascii=False))

if __name__ == '__main__':
    try: main(sys.argv[1:])
    except (OSError, RuntimeError, ValueError, IndexError) as exc:
        print('Servise erişilemedi: '+str(exc), file=sys.stderr); sys.exit(1)
