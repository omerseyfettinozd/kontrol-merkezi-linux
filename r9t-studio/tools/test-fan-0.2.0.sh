#!/usr/bin/env bash
# Faz 0.8 kabul testleri (r9t_fan 0.2.0).  sudo r9t-studio/tools/test-fan-0.2.0.sh
#
# Eski servis test süresince durdurulur ve sonunda eski durumuna döner. Fana yalnız
# %60 (taban %50'nin üstü) ve auto yazılır. Her çıkışta auto yazılıp doğrulanır.
#   1. auto → mode=0, mode_byte & 0x40 == 0
#   2. manual 60 60 → mode=1, PWM ≈ 120, lease ≤ 15000
#   3. lease yenilenmezse ≤ 16 sn'de auto ve error=-110
#   4. geçersiz komutlar -EINVAL (manual 49, manual 101, bozuk)
#   Uyku testi burada YOK: 2026-10-18 gözetimsiz rtcwake makineyi uyandırmadı (docs/09). Uyku elle, kullanıcı başındayken test edilir.
# Sonuç: r9t-studio/docs/09_fan_0.2.0_kabul.json
set -uo pipefail

[[ $EUID -eq 0 ]] || { echo "root gerekli" >&2; exit 1; }
here="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
out="$here/../docs/09_fan_0.2.0_kabul.json"
P=/sys/module/r9t_fan/parameters
do_sleep=0

field() { sed -n "s/.*\b$1=\([0-9-]*\).*/\1/p" <<<"$2"; }
results=()
record() { results+=("{\"test\": \"$1\", \"ok\": $2, \"detail\": \"$3\"}"); echo "[$([[ $2 == true ]] && echo GEÇTİ || echo KALDI)] $1 — $3"; }

was_active=0
systemctl is-active --quiet slayer-r9t.service && was_active=1
cleanup() {
    echo auto > "$P/control" 2>/dev/null
    sleep 1
    local s; s=$(cat "$P/status")
    echo "Son durum: $s"
    [[ $was_active == 1 ]] && systemctl start slayer-r9t.service
}
trap cleanup EXIT
systemctl stop slayer-r9t.service
sleep 1

# Ön koşul: sıcaklıklar makul
s=$(cat "$P/status")
cpu=$(field cpu_temp "$s"); gpu=$(field gpu_temp "$s")
if (( cpu >= 85 || gpu >= 80 )); then echo "Sıcaklık yüksek ($cpu/$gpu), test yapılmadı." >&2; exit 1; fi

# 1
echo auto > "$P/control"; sleep 1; s=$(cat "$P/status")
m=$(field mode "$s"); b=$(field mode_byte "$s")
[[ $m == 0 && $(( b & 64 )) == 0 ]] && record auto true "mode=$m mode_byte=$b" || record auto false "$s"

# 2
t0=$(date +%s.%N)
echo "manual 60 60" > "$P/control"; sleep 3; s=$(cat "$P/status")
m=$(field mode "$s"); pw=$(field cpu_pwm "$s"); pw2=$(field gpu_pwm "$s"); lease=$(field lease_ms "$s"); b=$(field mode_byte "$s")
[[ $m == 1 && $pw -ge 114 && $pw -le 126 && $pw2 -ge 114 && $pw2 -le 126 && $lease -gt 0 && $lease -le 15000 && $(( b & 64 )) == 64 ]] \
    && record manual true "mode=$m pwm=$pw/$pw2 lease_ms=$lease rpm=$(field cpu_rpm "$s")/$(field gpu_rpm "$s")" || record manual false "$s"

# 3: lease dolumu
end=$(( $(date +%s) + 20 )); expired=""
while (( $(date +%s) < end )); do
    s=$(cat "$P/status")
    if [[ $(field mode "$s") == 0 ]]; then expired=$(awk -v a="$t0" -v b="$(date +%s.%N)" 'BEGIN{printf "%.1f", b-a}'); break; fi
    sleep 0.5
done
err=$(field error "$s"); b=$(field mode_byte "$s")
[[ -n $expired && $err == -110 && $(( b & 64 )) == 0 ]] && awk -v e="$expired" 'BEGIN{exit !(e<=16.5)}' \
    && record lease_expiry true "auto ${expired}s sonra, error=$err" || record lease_expiry false "expired=${expired:-yok} $s"

# 4
bad=0
for cmd in "manual 49 60" "manual 60 101" "manual 60" "fast" "manual 60 60 x"; do
    if echo "$cmd" > "$P/control" 2>/dev/null; then bad=1; fi
done
[[ $bad == 0 ]] && record invalid_rejected true "5 geçersiz komut reddedildi" || record invalid_rejected false "geçersiz bir komut kabul edildi"
echo auto > "$P/control"

# 5
if [[ $do_sleep == 1 ]]; then
    echo "manual 60 60" > "$P/control"; sleep 2
    before=$(cat "$P/status")
    rtcwake -m freeze -s 20 >/dev/null 2>&1; rc=$?
    sleep 3
    s=$(cat "$P/status"); m=$(field mode "$s"); b=$(field mode_byte "$s")
    j=$(journalctl -k --since "-2min" --no-pager 2>/dev/null | grep -c "r9t_fan" || true)
    [[ $rc == 0 && $m == 0 && $(( b & 64 )) == 0 ]] && record sleep_resume true "rtcwake rc=$rc; uyanınca mode=$m mode_byte=$b; çekirdek r9t_fan log satırı=$j" \
        || record sleep_resume false "rc=$rc before=[$before] after=[$s]"
fi

{
    printf '{\n  "generated_utc": "%s",\n  "kernel": "%s",\n  "module_version": "%s",\n  "srcversion": "%s",\n  "tests": [\n' \
        "$(date -u +%FT%TZ)" "$(uname -r)" "$(cat /sys/module/r9t_fan/version 2>/dev/null)" "$(cat /sys/module/r9t_fan/srcversion)"
    for i in "${!results[@]}"; do printf '    %s%s\n' "${results[$i]}" "$([[ $i -lt $(( ${#results[@]} - 1 )) ]] && echo ,)"; done
    printf '  ]\n}\n'
} > "$out"
chown "${SUDO_UID:-0}:${SUDO_GID:-0}" "$out"
echo "Yazıldı: $out"
