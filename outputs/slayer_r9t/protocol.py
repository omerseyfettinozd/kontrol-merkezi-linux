"""Bounded, versioned local RPC protocol."""
import json
import socket
import uuid

SOCKET = '/run/slayer-r9t/control.sock'
VERSION = 1
REQUEST_LIMIT = 131072
RESPONSE_LIMIT = 131072
FIELDS = {
    'status': set(), 'events': set(), 'auto': set(), 'fan_boost':set(), 'fan_preset':{'preset'},
    'power': {'profile'}, 'boost': {'enabled'},
    'thermal_preset': {'preset'},
    'rgb': {'rgb', 'brightness'}, 'profile': {'settings'},
    'manual': {'cpu', 'gpu'}, 'curve': {'points'},
    'configure':{'document'},'automation_resume':set(),'automation_pause':set(),'session_ready':set(),
    'fn_lock':{'enabled'},'camera':{'enabled'},'charge_limit':{'end','start'},
    'temperature_target':{'cpu','gpu'}, 'temperature_target_stop':set(),
    'lighting_idle':{'idle'},
}


def receive(connection, limit):
    buffer = bytearray()
    while b'\n' not in buffer:
        chunk = connection.recv(4096)
        if not chunk:
            raise ValueError('Eksik servis iletisi.')
        buffer.extend(chunk)
        if len(buffer) > limit:
            raise ValueError('Servis iletisi boyut sınırını aştı.')
    line, tail = bytes(buffer).split(b'\n', 1)
    if tail:
        raise ValueError('Bağlantı başına tek istek kabul edilir.')
    value = json.loads(line)
    if not isinstance(value, dict):
        raise ValueError('İleti bir nesne olmalı.')
    return value


def validate(request):
    if type(request.get('version')) is not int or request['version'] != VERSION:
        raise ValueError('Servis protokolü sürümü uyumsuz; programı güncelleyin.')
    identifier = request.get('id')
    if not isinstance(identifier, str) or not 1 <= len(identifier) <= 64:
        raise ValueError('İstek kimliği geçersiz.')
    operation = request.get('op')
    if not isinstance(operation, str) or operation not in FIELDS:
        raise ValueError('Bilinmeyen işlem.')
    if set(request) != {'version', 'id', 'op'} | FIELDS[operation]:
        raise ValueError('İstek alanları geçersiz veya eksik.')
    return request


def request(data):
    payload = dict(data, version=VERSION, id=uuid.uuid4().hex)
    validate(payload)
    wire = (json.dumps(payload, ensure_ascii=False)+'\n').encode()
    if len(wire) > REQUEST_LIMIT:
        raise ValueError('İstek çok büyük.')
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
        connection.settimeout(18)
        connection.connect(SOCKET)
        connection.sendall(wire)
        reply = receive(connection, RESPONSE_LIMIT)
    if reply.get('version') != VERSION or reply.get('id') != payload['id']:
        raise RuntimeError('Servis yanıtı istekle eşleşmedi.')
    if reply.get('ok') is not True:
        raise RuntimeError(reply.get('error', 'Servis işlemi başarısız.'))
    return reply['data']
