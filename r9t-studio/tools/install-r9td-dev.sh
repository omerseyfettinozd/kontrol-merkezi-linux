#!/usr/bin/env bash
# Faz 2 geliştirme kurulumu: r9td gölge modu.  Önce kullanıcı olarak:
#   cargo build --release -p r9td -p r9tctl
# sonra:  sudo r9t-studio/tools/install-r9td-dev.sh
# Geri alma: sudo r9t-studio/tools/uninstall-r9td-dev.sh
set -euo pipefail
[[ $EUID -eq 0 ]] || { echo "root gerekli" >&2; exit 1; }
user="${SUDO_USER:?sudo ile çalıştırın}"
here="$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)"
for b in r9td r9tctl r9t-session; do [[ -x "$here/target/release/$b" ]] || { echo "Önce derleyin: target/release/$b yok" >&2; exit 1; }; done

install -d -m 0755 /usr/local/lib/r9t-studio /usr/local/share/doc/r9t-studio
install -m 0755 "$here/target/release/r9td" "$here/target/release/r9tctl" "$here/target/release/r9t-session" /usr/local/lib/r9t-studio/
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
# Faz 3: control unit (not enabled; used by r9t-switch --trial), fallback, legacy drop-in.
install -m 0644 "$here/packaging/systemd/r9td.service" "$here/packaging/systemd/r9t-fallback.service" /etc/systemd/system/
install -D -m 0644 "$here/packaging/systemd/slayer-r9t.service.d/r9t-conflicts.conf" /etc/systemd/system/slayer-r9t.service.d/r9t-conflicts.conf
install -m 0755 "$here/tools/r9t-switch" /usr/local/lib/r9t-studio/r9t-switch
ln -sf /usr/local/lib/r9t-studio/r9t-switch /usr/local/bin/r9t-switch
install -m 0755 "$here/tools/r9t-switch-user" /usr/local/lib/r9t-studio/r9t-switch-user
ln -sf /usr/local/lib/r9t-studio/r9t-switch-user /usr/local/bin/r9t-switch-user
# Faz 4: the UI (built with `cd ui && npx tauri build --no-bundle`), menu entry and icon.
ui="$here/ui/src-tauri/target/release/r9t-studio"
if [[ -x "$ui" ]]; then
    install -m 0755 "$ui" /usr/local/lib/r9t-studio/r9t-studio
    install -D -m 0644 "$here/ui/src-tauri/icons/icon.png" /usr/local/share/icons/hicolor/64x64/apps/r9t-studio.png
    install -D -m 0644 "$here/packaging/applications/r9t-studio.desktop" /usr/local/share/applications/r9t-studio.desktop
    gtk-update-icon-cache -q /usr/local/share/icons/hicolor 2>/dev/null || true
else
    echo "Arayüz derlenmemiş ($ui yok); yalnız servisler kuruldu."
fi
# User agent unit (not enabled: the legacy session agent stays until Faz 4).
install -D -m 0644 "$here/packaging/systemd-user/r9t-session.service" /etc/systemd/user/r9t-session.service
systemctl daemon-reload
systemctl enable --now r9td-shadow.service
sleep 3
systemctl --no-pager --lines=5 status r9td-shadow.service || true
ls -la /run/r9td/
