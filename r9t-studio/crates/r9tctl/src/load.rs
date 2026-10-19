//! `r9tctl load-test <target|fan> <saniye> <çıktı.json>` (Faz 3 load tests): full CPU
//! load on every core while r9td holds either a temperature target (75/72 °C) or the
//! `cool` fan preset; samples every 2 s and writes the rows as JSON. Needs control mode.
//! Afterwards the target is stopped and the fans go back to EC auto.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

fn burn(until: Instant) {
    let mut x = 1u32;
    while Instant::now() < until {
        for _ in 0..10_000 {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        }
        std::hint::black_box(x);
    }
}

fn sample(c: &mut crate::client::Client) -> Value {
    let s = crate::rpc(c, "status", json!({})).unwrap_or(Value::Null);
    let v = |p: &[&str]| p.iter().fold(&s, |d, k| &d[*k])["value"].clone();
    let max_freq = std::fs::read_to_string("/sys/devices/system/cpu/cpufreq/policy0/scaling_max_freq")
        .ok()
        .and_then(|t| t.trim().parse::<u64>().ok())
        .map(|k| k / 1000);
    json!({
        "cpu": v(&["cpu", "ec_temp_c"]), "gpu": v(&["gpu", "ec_temp_c"]),
        "duty": v(&["fans", "cpu_duty_pct"]), "rpm": v(&["fans", "cpu_rpm"]),
        "maxf": max_freq, "freq": v(&["cpu", "freq_mhz"]), "pkg": v(&["cpu", "package_w"]),
        "mode": v(&["fans", "mode"]), "guard": s["authority"]["guard_active"],
    })
}

pub fn run(args: &[String]) -> ExitCode {
    let (Some(mode), Some(secs), Some(out)) = (args.first(), args.get(1).and_then(|s| s.parse::<u64>().ok()), args.get(2)) else {
        eprintln!("Kullanım: r9tctl load-test <target|fan> <saniye> <çıktı.json>");
        return ExitCode::from(2);
    };
    let target = match mode.as_str() {
        "target" => true,
        "fan" => false,
        _ => {
            eprintln!("mod target ya da fan olmalı");
            return ExitCode::from(2);
        }
    };
    let mut c = match crate::connect() {
        Ok(c) => c,
        Err(code) => return code,
    };
    let say = |r: Result<Value, String>| println!("{}", r.map(|v| v["message"].as_str().unwrap_or("").to_owned()).unwrap_or_else(|e| format!("hata: {e}")));
    say(crate::rpc(&mut c, "thermal_preset", json!({"preset": "stock"})));
    if target {
        say(crate::rpc(&mut c, "temperature_target", json!({"cpu": 75, "gpu": 72})));
    } else {
        say(crate::rpc(&mut c, "fan_preset", json!({"preset": "cool"})));
    }
    std::thread::sleep(Duration::from_secs(3));
    let t0 = Instant::now();
    let stop = t0 + Duration::from_secs(secs);
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let workers: Vec<_> = (0..cores).map(|_| std::thread::spawn(move || burn(stop))).collect();
    let mut rows = Vec::new();
    while Instant::now() < stop {
        let mut r = sample(&mut c);
        let t = t0.elapsed().as_secs();
        r["t"] = json!(t);
        if t % 10 == 0 || r["guard"] == true {
            println!("{r}");
        }
        rows.push(r);
        std::thread::sleep(Duration::from_secs(2));
    }
    for w in workers {
        let _ = w.join();
    }
    std::thread::sleep(Duration::from_secs(4));
    println!("yük sonrası {}", sample(&mut c));
    if target {
        say(crate::rpc(&mut c, "temperature_target_stop", json!({})));
    }
    say(crate::rpc(&mut c, "auto", json!({})));
    match std::fs::write(out, serde_json::to_string(&rows).unwrap_or_default()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{out}: {e}");
            ExitCode::FAILURE
        }
    }
}
