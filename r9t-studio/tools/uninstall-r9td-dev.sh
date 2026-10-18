#!/usr/bin/env bash
# install-r9td-dev.sh'nin tersi. r9t grubu ve /var/lib/r9t korunur (--purge ile silinir).
set -euo pipefail
[[ $EUID -eq 0 ]] || { echo "root gerekli" >&2; exit 1; }
systemctl stop r9td.service 2>/dev/null || true
systemctl start slayer-r9t.service 2>/dev/null || true
systemctl disable --now r9td-shadow.service 2>/dev/null || true
rm -f /etc/systemd/user/r9t-session.service /etc/systemd/system/r9td-shadow.service /etc/systemd/system/r9td.service /etc/systemd/system/r9t-fallback.service /usr/local/bin/r9t-switch
rm -rf /etc/systemd/system/slayer-r9t.service.d/r9t-conflicts.conf /usr/share/polkit-1/actions/org.r9t.studio.policy /usr/local/bin/r9tctl
rm -rf /usr/local/lib/r9t-studio /usr/local/share/doc/r9t-studio
systemctl daemon-reload
if [[ "${1:-}" == "--purge" ]]; then
    rm -rf /var/lib/r9t /etc/sysusers.d/r9t.conf
    groupdel r9t 2>/dev/null || true
fi
echo "r9td gölge kurulumu kaldırıldı."
