//! `r9tctl` — command-line client for R9T Studio.

use std::process::ExitCode;

use r9t_hal::Sysfs;

const USAGE: &str = "\
Kullanım: r9tctl <komut> [seçenekler]

Komutlar:
  probe [--json] [--no-nvml]   Donanım yeteneklerini salt okunur yoklar
  help                         Bu metni gösterir
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
