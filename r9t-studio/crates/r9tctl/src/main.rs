//! `r9tctl` — command-line client for R9T Studio.

mod client;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use client::Client;
use r9t_hal::Sysfs;
use r9t_protocol::rpc::{Call, Empty, EventsParams, HistoryParams, SubscribeParams, Topic};

const USAGE: &str = "\
Kullanım: r9tctl <komut> [seçenekler]

Komutlar:
  probe [--json] [--no-nvml]   Donanım yeteneklerini salt okunur yoklar (r9td gerekmez)
  status                       r9td anlık görüntüsü (JSON)
  history [saniye]             Son en çok 300 sn geçmiş
  capabilities                 Yetenekler ve sayaçlar
  events [n]                   Denetim günlüğünün son olayları
  watch [n]                    Anlık görüntü akışına abone olur (n bildirim)
  call <yöntem> [params-json]  Ham JSON-RPC çağrısı
  bench [n]                    status gecikmesi (p50/p99)
  help                         Bu metni gösterir

Soket: $R9TD_SOCKET ya da /run/r9td/r9td.sock
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, rest) = match args.split_first() {
        Some((c, r)) => (c.as_str(), r),
        None => {
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match cmd {
        "probe" => probe(rest),
        "status" => simple(Call::Status(Empty {})),
        "capabilities" => simple(Call::Capabilities(Empty {})),
        "history" => simple(Call::History(HistoryParams { seconds: rest.first().and_then(|v| v.parse().ok()).unwrap_or(300) })),
        "events" => simple(Call::Events(EventsParams { limit: rest.first().and_then(|v| v.parse().ok()).unwrap_or(20) })),
        "call" => raw(rest),
        "watch" => watch(rest.first().and_then(|v| v.parse().ok()).unwrap_or(u64::MAX)),
        "bench" => bench(rest.first().and_then(|v| v.parse().ok()).unwrap_or(1000)),
        "help" | "-h" | "--help" => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("Bilinmeyen komut: {other}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn probe(args: &[String]) -> ExitCode {
    let mut json = false;
    let mut nvml = true;
    for a in args {
        match a.as_str() {
            "--json" => json = true,
            "--no-nvml" => nvml = false,
            other => {
                eprintln!("probe: bilinmeyen seçenek {other}");
                return ExitCode::from(2);
            }
        }
    }
    let report = r9t_hal::probe::probe(&Sysfs::from_env(), nvml);
    if json {
        match serde_json::to_string_pretty(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("JSON üretilemedi: {e}");
                return ExitCode::FAILURE;
            }
        }
        return ExitCode::SUCCESS;
    }
    let d = &report.device;
    println!("{} {} · BIOS {} · kernel {}", show(&d["vendor"]), show(&d["product"]), show(&d["bios"]), show(&d["kernel"]));
    println!("\nYetenekler:");
    for c in &report.capabilities {
        let mark = if c.available { "✓" } else { "✗" };
        let name = serde_json::to_value(c.id).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default();
        if c.reason.is_empty() {
            println!("  {mark} {name}");
        } else {
            println!("  {mark} {name:<18} {}", c.reason);
        }
    }
    let r = &report.readings;
    println!("\nOkumalar:");
    println!("  fan      {}", compact(&r["fan"]));
    println!("  ec       {}", compact(&r["ec"]));
    println!("  cpu      {}", compact(&r["cpu"]));
    println!("  paket W  {}", compact(&r["package_w"]));
    println!("  APU PPT  {}", compact(&r["apu_ppt_w"]));
    println!("  gpu      {}", compact(&r["gpu"]));
    println!("  güç      {}", compact(&r["power"]));
    println!("  rgb      {}", compact(&r["rgb"]));
    println!("  fn_lock  {}", compact(&r["fn_lock"]));
    println!("  sensörler:");
    if let Some(list) = r["sensors"].as_array() {
        for s in list {
            println!("    {:<40} {}", show(&s["id"]), show(&s["c"]));
        }
    }
    ExitCode::SUCCESS
}

fn show(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => "—".into(),
        other => other.to_string(),
    }
}

fn compact(v: &serde_json::Value) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

fn socket() -> PathBuf {
    std::env::var_os("R9TD_SOCKET").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r9t_protocol::DAEMON_SOCKET))
}

fn connect() -> Result<Client, ExitCode> {
    Client::connect(&socket(), Some(Duration::from_secs(10))).map_err(|e| {
        eprintln!("r9td'ye bağlanılamadı ({}): {e}", socket().display());
        ExitCode::from(3)
    })
}

fn simple(call: Call) -> ExitCode {
    let mut c = match connect() {
        Ok(c) => c,
        Err(code) => return code,
    };
    match c.call(call) {
        Ok(v) => {
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("hata: {e}");
            ExitCode::FAILURE
        }
    }
}

fn raw(args: &[String]) -> ExitCode {
    let Some(method) = args.first() else {
        eprintln!("call: yöntem gerekli");
        return ExitCode::from(2);
    };
    let params = args.get(1).map(String::as_str).unwrap_or("{}");
    let line = format!(r#"{{"jsonrpc":"2.0","id":1,"method":{},"params":{params}}}"#, serde_json::Value::from(method.as_str()));
    let call = match r9t_protocol::rpc::decode_request(line.as_bytes()) {
        Ok(r) => r.call,
        Err((_, e)) => {
            eprintln!("geçersiz istek: {e}");
            return ExitCode::from(2);
        }
    };
    simple(call)
}

fn watch(n: u64) -> ExitCode {
    let mut c = match connect() {
        Ok(c) => c,
        Err(code) => return code,
    };
    if let Err(e) = c.call(Call::Subscribe(SubscribeParams { topics: vec![Topic::Snapshot] })) {
        eprintln!("abonelik: {e}");
        return ExitCode::FAILURE;
    }
    for _ in 0..n {
        match c.notification() {
            Ok(note) => println!("{}", serde_json::to_string(&note.params).unwrap_or_default()),
            Err(e) => {
                eprintln!("akış bitti: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

fn bench(n: usize) -> ExitCode {
    let mut c = match connect() {
        Ok(c) => c,
        Err(code) => return code,
    };
    let mut times = Vec::with_capacity(n);
    for _ in 0..n {
        let t = Instant::now();
        if let Err(e) = c.call(Call::Status(Empty {})) {
            eprintln!("hata: {e}");
            return ExitCode::FAILURE;
        }
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let q = |p: f64| times[((times.len() as f64 * p) as usize).min(times.len() - 1)];
    println!("{{\"n\": {n}, \"p50_ms\": {:.3}, \"p99_ms\": {:.3}, \"max_ms\": {:.3}}}", q(0.5), q(0.99), times[times.len() - 1]);
    ExitCode::SUCCESS
}
