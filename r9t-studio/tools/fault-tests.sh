#!/bin/bash
# runs as root
trap 'rm -rf /run/systemd/system/r9td.service.d; systemctl daemon-reload' EXIT
st() { cat /sys/module/r9t_fan/parameters/status; }
f() { sed -n "s/.*\b$1=\([0-9-]*\).*/\1/p" <<<"$2"; }
maxf() { cat /sys/devices/system/cpu/cpufreq/policy0/scaling_max_freq; }
wait_for() { # cond-desc, timeout_s, test-cmd ; prints elapsed
  local t0=$(date +%s.%N) lim=$2; shift 2
  while :; do if eval "$@"; then awk -v a=$t0 -v b=$(date +%s.%N) 'BEGIN{printf "%.1f", b-a}'; return 0; fi
    awk -v a=$t0 -v b=$(date +%s.%N) -v l=$lim 'BEGIN{exit !(b-a>l)}' && { echo "ZAMAN AŞIMI"; return 1; }; sleep 0.2; done; }
is_auto() { s=$(st); [[ $(f mode "$s") == 0 && $(( $(f mode_byte "$s") & 64 )) == 0 ]]; }
is_m60() { s=$(st); [[ $(f mode "$s") == 1 && $(f cpu_target "$s") == 60 ]]; }
is_quiet() { [[ $(maxf) == 1800000 ]]; }
call() { /usr/local/bin/r9tctl call "$@" >/dev/null; }
setup() { call thermal_preset '{"preset":"quiet"}'; call manual '{"cpu":60,"gpu":60}'; sleep 4; }
echo "baseline max=$(maxf)"
r9t-switch --trial 30 >/dev/null; sleep 3
echo "== T1 kill -9 (ExecStopPost + Restart)"
setup; echo "  önce: $(st | cut -c1-60) max=$(maxf)"
kill -9 $(systemctl show -p MainPID --value r9td.service)
echo "  auto'ya dönüş: $(wait_for auto 10 is_auto) sn; max=$(maxf)"
echo "  yeniden başlayıp quiet+manual60: $(wait_for re 20 'is_m60 && is_quiet') sn"
systemctl reset-failed r9td.service
echo "== T2 kill -STOP (watchdog 10 s)"
pid=$(systemctl show -p MainPID --value r9td.service); [[ $pid -gt 1 ]] || { echo "r9td çalışmıyor, test durdu"; exit 1; }; kill -STOP $pid; t0=$(date +%s)
echo "  auto'ya dönüş: $(wait_for auto 30 is_auto) sn (STOP'tan itibaren)"
echo "  yeniden başlayıp son politika: $(wait_for re 30 'is_m60 && is_quiet') sn sonra"; echo "  NRestarts=$(systemctl show -p NRestarts --value r9td.service) sonuç=$(systemctl show -p Result --value r9td.service)"
systemctl reset-failed r9td.service
echo "== T3 lease (Restart=no, ExecStopPost boş)"
mkdir -p /run/systemd/system/r9td.service.d
printf '[Unit]\nOnFailure=\n[Service]\nRestart=no\nExecStopPost=\n' > /run/systemd/system/r9td.service.d/zz-lease-test.conf
systemctl daemon-reload; systemctl restart r9td.service; sleep 3
call manual '{"cpu":60,"gpu":60}'; sleep 3; echo "  önce: $(st | cut -c1-80)"
kill -9 $(systemctl show -p MainPID --value r9td.service)
echo "  lease ile auto: $(wait_for lease 25 is_auto) sn; $(st | grep -o 'error=[-0-9]*')"
rm -f /run/systemd/system/r9td.service.d/zz-lease-test.conf; systemctl daemon-reload; systemctl reset-failed r9td.service
systemctl start r9td.service
echo "  normal unit geri: $(wait_for re 20 'is_m60 && is_quiet') sn içinde son politika"
systemctl reset-failed r9td.service
echo "== T4 systemctl restart"
t0=$(date +%s.%N); systemctl restart r9td.service
echo "  son politika: $(wait_for re 20 'is_m60 && is_quiet') sn"
echo "== son: auto + stock"; call auto; call thermal_preset '{"preset":"stock"}'; sleep 1; echo "  $(st | cut -c1-40) max=$(maxf)"
journalctl -u r9td.service --since -6min --no-pager -o cat | grep -E "emergency|Watchdog|watchdog|SIGABRT|core-dump|son istenen" | tail -8
