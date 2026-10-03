#!/usr/bin/env python3
"""Small, guarded fan controller for the GameGaraj Slayer R9T."""

import fcntl
import os
from pathlib import Path
import struct
import sys

DEVICE = "/dev/tuxedo_io"
EXPECTED_MODEL_ID = 0x1A
READ_UW = 0xEF
WRITE_UW = 0xF0
IOCTL_POINTER_SIZE = struct.calcsize("P")


def ior(number):
    return (2 << 30) | (IOCTL_POINTER_SIZE << 16) | (READ_UW << 8) | number


def iow(number):
    return (1 << 30) | (IOCTL_POINTER_SIZE << 16) | (WRITE_UW << 8) | number


def read_int(fd, number):
    data = bytearray(4)
    fcntl.ioctl(fd, ior(number), data, True)
    return struct.unpack("<i", data)[0]


def write_int(fd, number, value):
    data = bytearray(struct.pack("<i", value))
    fcntl.ioctl(fd, iow(number), data, True)


def verify_device(fd):
    vendor = Path("/sys/class/dmi/id/sys_vendor").read_text().strip()
    product = Path("/sys/class/dmi/id/product_name").read_text().strip()
    if vendor != "GAME GARAJ" or product != "SLAYER R9T":
        raise RuntimeError(f"Bu araç Slayer R9T içindir; bulunan cihaz: {vendor} / {product}")

    # Re-identify before accessing cached Uniwill features. tuxedo_io may have
    # loaded before uniwill_wmi during boot; MODEL_ID alone then dereferences
    # an uninitialized feature pointer in upstream 4.24.0.
    detected = bytearray(4)
    hwcheck = (2 << 30) | (IOCTL_POINTER_SIZE << 16) | (0xEC << 8) | 0x06
    fcntl.ioctl(fd, hwcheck, detected, True)
    if struct.unpack('<i', detected)[0] != 1:
        raise RuntimeError('Uniwill arayüzü henüz hazır değil; model/fan erişimi yapılmadı.')
    model_id = read_int(fd, 0x01)
    if model_id != EXPECTED_MODEL_ID:
        raise RuntimeError(f"Uniwill EC kimliği beklenenden farklı: 0x{model_id:02x}")

    interface = bytearray(32)
    fcntl.ioctl(fd, ior(0x00), interface, True)
    if b"uniwill_wmi" not in interface:
        raise RuntimeError("Aktif TUXEDO EC arayüzü uniwill_wmi değil; fan komutu gönderilmedi.")

    return model_id


def open_device():
    if os.geteuid() != 0:
        raise RuntimeError("Yetki gerekli. Komutu sudo ile çalıştır.")
    fd = os.open(DEVICE, os.O_RDWR | os.O_CLOEXEC)
    try:
        verify_device(fd)
    except Exception:
        os.close(fd)
        raise
    return fd


def show_status(fd):
    model_id = read_int(fd, 0x01)
    cpu_temp = read_int(fd, 0x12)
    gpu_temp = read_int(fd, 0x13)
    cpu_pwm = read_int(fd, 0x10)
    gpu_pwm = read_int(fd, 0x11)
    mode = read_int(fd, 0x14)
    print(f"Model kimliği: 0x{model_id:02x}")
    print(f"CPU / GPU sıcaklığı: {cpu_temp}°C / {gpu_temp}°C")
    print(f"CPU / GPU fan EC değeri: {cpu_pwm} / {gpu_pwm} (0–200 ölçeği)")
    print(f"EC profil baytı 0x0751: 0x{mode:02x}")


def usage():
    print("Kullanım: sudo r9t-control.py status | auto")
    print("Elle fan kontrolü bu R9T firmware üzerinde doğrulanmadığı için kapalı.")


def main(argv):
    if len(argv) != 2 and not (len(argv) == 3 and argv[1] == "set"):
        usage()
        return 2

    if argv[1] == "set":
        raise RuntimeError("Bu R9T firmware sürümünde fan hedefleri korunmadığı için elle kontrol kapalı.")
    fd = open_device()
    try:
        if argv[1] == "status" and len(argv) == 2:
            show_status(fd)
            return 0
        if argv[1] == "auto" and len(argv) == 2:
            # W_UW_FANAUTO returns the EC to its built-in automatic fan tables.
            fcntl.ioctl(fd, (WRITE_UW << 8) | 0x14)
            print("Fanlar EC otomatik kontrolüne döndürüldü.")
            return 0
        usage()
        return 2
    finally:
        os.close(fd)


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv))
    except (OSError, RuntimeError) as error:
        print(f"Hata: {error}", file=sys.stderr)
        raise SystemExit(1)
