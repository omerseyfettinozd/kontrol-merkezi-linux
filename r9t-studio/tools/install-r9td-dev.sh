#!/usr/bin/env bash
# Faz 2 geliştirme kurulumu: r9td gölge modu.  Önce kullanıcı olarak:
#   cargo build --release -p r9td -p r9tctl
# sonra:  sudo r9t-studio/tools/install-r9td-dev.sh
# Geri alma: sudo r9t-studio/tools/uninstall-r9td-dev.sh
set -euo pipefail
[[ $EUID -eq 0 ]] || { echo "root gerekli" >&2; exit 1; }
user="${SUDO_USER:?sudo ile çalıştırın}"
here="$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)"
for b in r9td r9tctl; do [[ -x "$here/target/release/$b" ]] || { echo "Önce derleyin: target/release/$b yok" >&2; exit 1; }; done

install -d -m 0755 /usr/local/lib/r9t-studio /usr/local/share/doc/r9t-studio
install -m 0755 "$here/target/release/r9td" "$here/target/release/r9tctl" /usr/local/lib/r9t-studio/
ln -sf /usr/local/lib/r9t-studio/r9tctl /usr/local/bin/r9tctl
install -m 0644 "$here/docs/04_ana_plan.md" /usr/local/share/doc/r9t-studio/
install -D -m 0644 "$here/packaging/sysusers.d/r9t.conf" /etc/sysusers.d/r9t.conf
systemd-sysusers /etc/sysusers.d/r9t.conf
if ! id -nG "$user" | tr ' ' '\n' | grep -qx r9t; then
    usermod -aG r9t "$user"
    echo "$user r9t grubuna eklendi; grubun oturumda görünmesi için yeniden giriş gerekir (ya da: sg r9t -c 'r9tctl status')."
fi
install -m 0644 "$here/packaging/polkit/org.r9t.studio.policy" /usr/share/polkit-1/actions/org.r9t.studio.policy
install -m 0644 "$here/packaging/systemd/r9td-shadow.service" /etc/systemd/system/r9td-shadow.service
systemctl daemon-reload
systemctl enable --now r9td-shadow.service
sleep 3
systemctl --no-pager --lines=5 status r9td-shadow.service || true
ls -la /run/r9td/
