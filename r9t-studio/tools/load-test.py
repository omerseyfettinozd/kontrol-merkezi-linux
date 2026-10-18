import json, multiprocessing as mp, subprocess, sys, time
def burn(stop):
    x = 0
    while time.time() < stop: x = (x * 1103515245 + 12345) & 0xffffffff
def r9(method, params):
    return json.loads(subprocess.run(['/usr/local/bin/r9tctl','call',method,json.dumps(params)],capture_output=True,text=True).stdout or '{}')
def snap():
    s = json.loads(subprocess.run(['/usr/local/bin/r9tctl','status'],capture_output=True,text=True).stdout)
    def v(*p):
        d = s
        for k in p:
            d = d[k]
        return d['value']
    return dict(cpu=v('cpu','ec_temp_c'), gpu=v('gpu','ec_temp_c'), duty=v('fans','cpu_duty_pct'), rpm=v('fans','cpu_rpm'),
                maxf=int(open('/sys/devices/system/cpu/cpufreq/policy0/scaling_max_freq').read())//1000,
                freq=v('cpu','freq_mhz'), pkg=v('cpu','package_w'), mode=v('fans','mode'), guard=s['authority']['guard_active'])
mp.set_start_method("fork")
mode = sys.argv[1]; secs = int(sys.argv[2])
r9('thermal_preset', {'preset': 'stock'})
if mode == 'target': print(r9('temperature_target', {'cpu': 75, 'gpu': 72}).get('message'))
else: print(r9('fan_preset', {'preset': 'cool'}).get('message'))
time.sleep(3)
stop = time.time() + secs
ps = [mp.Process(target=burn, args=(stop,)) for _ in range(16)]
[p.start() for p in ps]
rows = []
t0 = time.time()
while time.time() < stop:
    r = snap(); r['t'] = round(time.time() - t0); rows.append(r)
    if r['t'] % 10 == 0 or r['guard']: print(r, flush=True)
    time.sleep(2)
[p.join() for p in ps]
time.sleep(4); print('yük sonrası', snap())
if mode == 'target': print(r9('temperature_target_stop', {}).get('message'))
r9('auto', {})
json.dump(rows, open(sys.argv[3], 'w'))
