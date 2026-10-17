#!/usr/bin/env bash
# r9t_fan'ı DKMS 0.1.0'a geri döndürür.  sudo r9t-studio/tools/rollback-fan-0.1.0.sh [--no-service-start]
# 0.1.0 kaynağı /usr/src/slayer-r9t-fan-0.1.0'da durur; kayıt yoksa yeniden eklenir.
set -euo pipefail

PKG=slayer-r9t-fan
NEW=0.2.0
OLD=0.1.0
start_service=1
[[ "${1:-}" == "--no-service-start" ]] && start_service=0

[[ $EUID -eq 0 ]] || { echo "root gerekli: sudo $0" >&2; exit 1; }
[[ -f "/usr/src/$PKG-$OLD/dkms.conf" ]] || { echo "/usr/src/$PKG-$OLD yok; geri alınamaz." >&2; exit 1; }

# pipefail altında `dkms status | grep -q` SIGPIPE ile yanlış sonuç verir; çıktıyı önce al.
dkms_state() { local out; out=$(dkms status "$PKG/$1" -k "$2" 2>/dev/null || true); sed -n 's/.*: //p' <<<"$out" | head -n1; }
dkms_known() { local out; out=$(dkms status "$PKG/$1" 2>/dev/null || true); [[ -n "$out" ]]; }
kernels=()
for dir in /lib/modules/*; do
    k=$(basename "$dir")
    if [[ -f "$dir/build/Makefile" ]] && pacman -Qqo "$dir/vmlinuz" >/dev/null 2>&1; then kernels+=("$k"); fi
done

was_active=0
systemctl is-active --quiet slayer-r9t.service && was_active=1
systemctl stop slayer-r9t.service || true

dkms_known "$OLD" || dkms add -m "$PKG" -v "$OLD"
for k in "${kernels[@]}"; do
    if [[ "$(dkms_state "$NEW" "$k")" == installed* ]]; then
        dkms uninstall -m "$PKG" -v "$NEW" -k "$k" || true
    fi
    dkms install -m "$PKG" -v "$OLD" -k "$k" --force
done
dkms remove -m "$PKG" -v "$NEW" --all 2>/dev/null || true

if [[ -d /sys/module/r9t_fan ]]; then rmmod r9t_fan; fi
modprobe r9t_fan
sleep 1
echo "srcversion: $(cat /sys/module/r9t_fan/srcversion)  status: $(cat /sys/module/r9t_fan/parameters/status)"
if [[ $start_service == 1 && $was_active == 1 ]]; then systemctl start slayer-r9t.service; fi
dkms status -m "$PKG"
echo "Geri alma tamam: $OLD"
