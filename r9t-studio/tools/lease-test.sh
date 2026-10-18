#!/bin/bash
trap 'rm -rf /run/systemd/system/r9td.service.d; mv /etc/systemd/system/r9t-fallback.service.off /etc/systemd/system/r9t-fallback.service 2>/dev/null; systemctl daemon-reload' EXIT
mv /etc/systemd/system/r9t-fallback.service /etc/systemd/system/r9t-fallback.service.off
mkdir -p /run/systemd/system/r9td.service.d
printf '[Unit]\nOnFailure=\n[Service]\nRestart=no\nExecStopPost=\n' > /run/systemd/system/r9td.service.d/zz-lease-test.conf
systemctl daemon-reload; systemctl show -p Restart,ExecStopPost,OnFailure r9td.service | cut -c1-80
systemctl restart r9td.service; sleep 3
/usr/local/bin/r9tctl call manual '{"cpu":60,"gpu":60}' >/dev/null; sleep 4
T=$(date "+%H:%M:%S"); pid=$(systemctl show -p MainPID --value r9td.service); echo "kill $pid at $T"; kill -9 $pid; t0=$(date +%s.%N)
for i in $(seq 1 40); do s=$(cat /sys/module/r9t_fan/parameters/status); printf '%5.1f %s\n' $(awk -v a=$t0 -v b=$(date +%s.%N) 'BEGIN{print b-a}') "$(echo $s | grep -oE 'mode=[0-9]|lease_ms=[0-9]+|error=[-0-9]+' | tr '\n' ' ')"; [[ $s == mode=0* ]] && break; sleep 0.5; done
journalctl --since "$T" --no-pager -o short-iso | grep -vE "sudo|pam_unix|warp-svc|NVRM" | tail -12
