#!/usr/bin/env bash
# Faz 0.8: r9t_fan DKMS 0.2.0 kurulumu.  sudo r9t-studio/tools/install-fan-0.2.0.sh
#
# Sıra: kaynak kopyala → iki paketli kernelde DERLE (hata: hiçbir şey değişmez) →
# eski servisi durdur (ExecStopPost fanı auto'ya alır) → auto doğrula → 0.1.0'ı bu
# kernellerden kaldır (kayıt ve /usr/src kaynağı geri alma için kalır) → 0.2.0 kur →
# modülü yeniden yükle → sürüm ve auto doğrula → servisi başlat.
# Kurulum aşamasında hata olursa rollback-fan-0.1.0.sh otomatik çalışır.
set -euo pipefail

PKG=slayer-r9t-fan
NEW=0.2.0
OLD=0.1.0
here="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
src="$here/../driver/r9t_fan"
params=/sys/module/r9t_fan/parameters

[[ $EUID -eq 0 ]] || { echo "root gerekli: sudo $0" >&2; exit 1; }
[[ "$(cat /sys/class/dmi/id/sys_vendor)" == "GAME GARAJ" && "$(cat /sys/class/dmi/id/product_name)" == "SLAYER R9T" ]] \
    || { echo "Bu cihaz Slayer R9T değil." >&2; exit 1; }

# Paketli ve başlıkları kurulu kerneller (sahipsiz modül dizinleri atlanır).
# pipefail altında `dkms status | grep -q` SIGPIPE ile yanlış sonuç verir; çıktıyı önce al.
dkms_state() { local out; out=$(dkms status "$PKG/$1" -k "$2" 2>/dev/null || true); sed -n 's/.*: //p' <<<"$out" | head -n1; }
dkms_known() { local out; out=$(dkms status "$PKG/$1" 2>/dev/null || true); [[ -n "$out" ]]; }
kernels=()
for dir in /lib/modules/*; do
    k=$(basename "$dir")
    if [[ -f "$dir/build/Makefile" ]] && pacman -Qqo "$dir/vmlinuz" >/dev/null 2>&1; then kernels+=("$k"); fi
done
running=$(uname -r)
printf 'Hedef kerneller: %s (çalışan: %s)\n' "${kernels[*]}" "$running"
[[ " ${kernels[*]} " == *" $running "* ]] || { echo "Çalışan kernel hedefler arasında değil." >&2; exit 1; }

auto_ok() {  # mode=0 ve mode_byte bit 6 temiz
    local s mode byte
    s=$(cat "$params/status") || return 1
    mode=$(sed -n 's/.*\bmode=\([0-9-]*\).*/\1/p' <<<"$s")
    byte=$(sed -n 's/.*mode_byte=\([0-9]*\).*/\1/p' <<<"$s")
    [[ "$mode" == 0 && $(( byte & 0x40 )) -eq 0 ]]
}

echo "== 1. Kaynak /usr/src/$PKG-$NEW"
rm -rf "/usr/src/$PKG-$NEW"
install -d "/usr/src/$PKG-$NEW"
install -m 0644 "$src"/{r9t_fan.c,Makefile,build.sh,dkms.conf} "/usr/src/$PKG-$NEW/"
dkms_known "$NEW" || dkms add -m "$PKG" -v "$NEW"

echo "== 2. Derleme (sistem henüz değişmedi)"
for k in "${kernels[@]}"; do
    if ! dkms build -m "$PKG" -v "$NEW" -k "$k"; then
        echo "Derleme başarısız ($k); $NEW kaydı kaldırılıyor, sistem değişmedi." >&2
        dkms remove -m "$PKG" -v "$NEW" --all || true
        rm -rf "/usr/src/$PKG-$NEW"
        exit 1
    fi
done

echo "== 3. Eski servis durduruluyor"
was_active=0
systemctl is-active --quiet slayer-r9t.service && was_active=1
systemctl stop slayer-r9t.service
sleep 1
auto_ok || { echo "UYARI: servis durunca fan auto doğrulanamadı; auto yazılıyor."; echo auto > "$params/control"; sleep 1; auto_ok; }

rollback() {
    echo "!! Hata: geri alma çalıştırılıyor" >&2
    "$here/rollback-fan-0.1.0.sh" --no-service-start || true
    [[ $was_active == 1 ]] && systemctl start slayer-r9t.service || true
    exit 1
}
trap rollback ERR

echo "== 4. $OLD bu kernellerden kaldırılıyor, $NEW kuruluyor"
for k in "${kernels[@]}"; do
    if [[ "$(dkms_state "$OLD" "$k")" == installed* ]]; then
        dkms uninstall -m "$PKG" -v "$OLD" -k "$k"
    fi
    dkms install -m "$PKG" -v "$NEW" -k "$k" --force
done

echo "== 5. Modül yeniden yükleniyor"
rmmod r9t_fan  # modprobe -r soft-dep'leri (tuxedo_keyboard) de boşaltmaya çalışır
modprobe r9t_fan
sleep 1
version=$(cat /sys/module/r9t_fan/version)
[[ "$version" == "$NEW" ]] || { echo "Yüklü sürüm $version" >&2; false; }
auto_ok
echo "srcversion: $(cat /sys/module/r9t_fan/srcversion)"
echo "status: $(cat "$params/status")"
trap - ERR

echo "== 6. Servis"
[[ $was_active == 1 ]] && systemctl start slayer-r9t.service && sleep 3 && systemctl is-active slayer-r9t.service
dkms status -m "$PKG"
echo "Tamam: r9t_fan $NEW yüklü."
