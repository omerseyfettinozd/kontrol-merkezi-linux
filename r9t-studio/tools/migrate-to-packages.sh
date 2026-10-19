#!/usr/bin/env bash
# migrate-to-packages.sh: dev install (/usr/local, /etc/systemd) → Arch packages
# (packaging/arch, built into packaging/arch/out). One command:
#   sudo r9t-studio/tools/migrate-to-packages.sh            (--dry-run: print only)
# Order: patched tuxedo-drivers → r9t_fan DKMS → r9t-studio → permanent switch again.
# Loaded kernel modules keep running throughout; if anything fails after the dev
# install was removed, it is reinstalled and the switch re-applied.
set -euo pipefail
DRY=0
[[ "${1:-}" == "--dry-run" ]] && DRY=1
((DRY)) || [[ $EUID -eq 0 ]] || { echo "root gerekli (sudo)" >&2; exit 1; }
user="${SUDO_USER:-$(id -un)}"
here="$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)"
out="$here/packaging/arch/out"
log() { printf '\n==> %s\n' "$*"; }
run() { if ((DRY)); then echo "  [dry-run] $*"; else "$@"; fi; }
die() { echo "HATA: $*" >&2; exit 1; }

newest() {
    local f
    f="$(ls "$out"/"$1"-[0-9]*.pkg.tar.zst 2>/dev/null | sort -V | tail -n1)"
    [[ -n "$f" ]] || die "$1 paketi yok: $out (önce makepkg, packaging/arch/README.md)"
    echo "$f"
}
TUX="$(newest tuxedo-drivers-r9t-dkms)"
FAN="$(newest slayer-r9t-fan-dkms)"
APP="$(newest r9t-studio)"
kernel="$(uname -r)"
committed=0
# Marker sits in a root-only directory; r9td enabled at boot means the same.
[[ -e /var/lib/r9t/committed || "$(systemctl is-enabled r9td.service 2>/dev/null)" == enabled ]] && committed=1

log "Ön kontrol"
for p in "$TUX" "$FAN" "$APP"; do
    pacman -Qip "$p" >/dev/null || die "okunamayan paket: $p"
    echo "  $(basename "$p")"
done
[[ -d "/usr/lib/modules/$kernel/build/include" ]] || die "çalışan kernel ($kernel) için başlıklar yok"
[[ -x /usr/local/lib/r9t-studio/r9td || -x /usr/lib/r9t-studio/r9td ]] || die "r9t-studio kurulu değil"
echo "  kernel $kernel, kalıcı geçiş: $([[ $committed == 1 ]] && echo var || echo yok), kullanıcı: $user"

# Captured, not piped into grep -q: with pipefail an early grep exit (SIGPIPE) fails the test.
dkms_ok() {  # module/version installed for the running kernel
    local s
    s="$(dkms status "$1" -k "$kernel" 2>/dev/null || true)"
    [[ "$s" == *": installed"* ]]
}

log "1/5 Yedek: /usr/src altındaki sürücü kaynakları"
backup="/var/lib/r9t/migrate-backup-$(date +%Y%m%d-%H%M%S).tar"
run install -d -m 0750 /var/lib/r9t
srcs=()
for d in /usr/src/tuxedo-drivers-4.24.0 /usr/src/slayer-r9t-fan-0.2.0; do [[ -d "$d" ]] && srcs+=("$d"); done
run tar -cf "$backup" "${srcs[@]}"
echo "  $backup"

log "2/5 tuxedo-drivers → tuxedo-drivers-r9t-dkms (aynı kaynak + kalıcı R9T yaması)"
if pacman -Q tuxedo-drivers-r9t-dkms >/dev/null 2>&1; then
    echo "  zaten kurulu"
else
    # -dd: tuxedo-control-center-bin depends on tuxedo-drivers-dkms, which the new
    # package provides; it is installed in the very next transaction.
    ((DRY)) || trap 'echo "HATA: tuxedo paketi kurulamadı. Yüklü modüller çalışmaya devam ediyor; yeniden başlatmadan önce: pacman -U $TUX (kaynak yedeği: $backup)" >&2' ERR
    pacman -Q tuxedo-drivers-dkms >/dev/null 2>&1 && run pacman -Rdd --noconfirm tuxedo-drivers-dkms
    run pacman -U --noconfirm "$TUX"
    trap - ERR
fi
((DRY)) || dkms_ok tuxedo-drivers/4.24.0 || die "tuxedo-drivers $kernel için kurulmadı (yedek: $backup)"
((DRY)) || grep -q R9T_RGB_INTENSITY_FIX /usr/src/tuxedo-drivers-4.24.0/ite_8291/ite_8291.c || die "R9T yaması kaynakta yok"

log "3/5 r9t_fan → slayer-r9t-fan-dkms"
if pacman -Q slayer-r9t-fan-dkms >/dev/null 2>&1; then
    echo "  zaten kurulu"
else
    # The hand-made DKMS copy owns the same path; the loaded module keeps running.
    [[ -n "$(dkms status slayer-r9t-fan/0.2.0 2>/dev/null || true)" ]] && run dkms remove slayer-r9t-fan/0.2.0 --all
    run rm -rf /usr/src/slayer-r9t-fan-0.2.0
    run pacman -U --noconfirm "$FAN"
fi
((DRY)) || dkms_ok slayer-r9t-fan/0.2.0 || die "slayer-r9t-fan $kernel için kurulmadı (yedek: $backup)"

restore_dev() {
    echo "HATA: paket kurulumu yarıda kaldı; geliştirme kurulumu geri yükleniyor" >&2
    pacman -Q r9t-studio >/dev/null 2>&1 && return
    SUDO_USER="$user" "$here/tools/install-r9td-dev.sh" || true
    ((committed)) && "$here/tools/r9t-switch" --commit || true
}

log "4/5 Geliştirme kurulumu → r9t-studio paketi (arada eski servis kontrolde)"
if pacman -Q r9t-studio >/dev/null 2>&1; then
    run pacman -U --noconfirm "$APP"
else
    ((DRY)) || trap restore_dev ERR
    run "$here/tools/uninstall-r9td-dev.sh"
    run pacman -U --noconfirm "$APP"
    trap - ERR
fi
run systemctl daemon-reload

log "5/5 Kontrol"
if ((committed)); then
    run /usr/bin/r9t-switch --commit
else
    run systemctl enable --now r9td-shadow.service
fi
# User side: the session agent's unit now lives in /usr/lib/systemd/user.
run systemctl --user -M "$user@" daemon-reload
run systemctl --user -M "$user@" reenable r9t-session.service
run systemctl --user -M "$user@" restart r9t-session.service

((DRY)) && exit 0
log "Sonuç"
for u in r9td r9td-shadow slayer-r9t; do
    printf '  %-12s %-9s %s  %s\n' "$u" "$(systemctl is-active $u)" "$(systemctl is-enabled $u 2>/dev/null)" "$(systemctl show -P FragmentPath $u)"
done
printf '  %-12s %s\n' r9t-session "$(systemctl --user -M "$user@" is-active r9t-session.service)"
dkms status | grep -E 'tuxedo-drivers|slayer-r9t-fan' | sed 's/^/  /'
/usr/bin/r9tctl status | /usr/bin/python3 -I -c 'import json,sys; d=json.load(sys.stdin); print("  authority:", d["authority"]["state"], d["authority"]["session"])'
echo "  Pakete geçiş tamam. Yedek: $backup"
