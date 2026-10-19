//! `r9tctl switch` (also installed as `r9t-switch`): moves hardware control between
//! the legacy service and r9td. Root only.
//!
//! - `--trial <dk>`: r9td controls for a limited time, then the legacy service returns
//! - `--commit`: r9td is the permanent controller (enabled at boot)
//! - `--rollback`: legacy service back in control, r9td shadow resumes; after
//!   `--commit` this is permanent too
//! - `--fallback`: `r9t-fallback.service` (r9td start limit hit): legacy now, boot
//!   configuration untouched. Once the legacy system is removed (`r9tctl
//!   legacy-remove`), the fans stay in EC auto instead.
//! - `--trial`/`--rollback` refuse after that removal, naming the backup to restore.
//! - `--status`
//!
//! The desktop side (session agent, Hyprland line, menu entry) is `r9tctl desktop`.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

const LEGACY_SOCK: &str = "/run/slayer-r9t/control.sock";
const COMMITTED: &str = "/var/lib/r9t/committed";
/// Files the legacy service must not find from an older r9td run.
const STALE: [&str; 3] = ["/var/lib/r9t/baseline.json", "/var/lib/r9t/last.json", "/var/lib/r9t/automation-snapshot.json"];

/// Directory of this binary: /usr/lib/r9t-studio (package) or /usr/local/lib/r9t-studio.
pub fn lib_dir() -> PathBuf {
    std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("/usr/lib/r9t-studio"))
}

/// Effective uid via the owner of /proc/self (no libc call needed).
pub fn euid() -> u32 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(u32::MAX)
}

struct Fail(String);

type Step = Result<(), Fail>;

fn systemctl(args: &[&str]) -> Step {
    let st = Command::new("systemctl").args(args).status().map_err(|e| Fail(format!("systemctl: {e}")))?;
    if st.success() { Ok(()) } else { Err(Fail(format!("systemctl {} başarısız", args.join(" ")))) }
}

/// Best effort (`|| true` in the shell version).
fn systemctl_quiet(args: &[&str]) {
    let _ = Command::new("systemctl").args(args).stderr(std::process::Stdio::null()).status();
}

fn unit_state(verb: &str, unit: &str) -> String {
    Command::new("systemctl")
        .args([verb, unit])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

/// One request on the legacy socket (NDJSON, version 1); failures are only reported.
fn legacy_rpc(op: &str) {
    let r = (|| -> std::io::Result<String> {
        let mut s = UnixStream::connect(LEGACY_SOCK)?;
        s.set_read_timeout(Some(Duration::from_secs(20)))?;
        let line = serde_json::json!({"version": 1, "id": "r9t-switch", "op": op}).to_string() + "\n";
        s.write_all(line.as_bytes())?;
        let mut buf = vec![0u8; 65536];
        let n = s.read(&mut buf)?;
        Ok(String::from_utf8_lossy(&buf[..n]).trim().chars().take(300).collect())
    })();
    match r {
        Ok(v) => println!("{v}"),
        Err(e) => println!("eski servis: {e}"),
    }
}

fn print_authority() {
    let Ok(mut c) = crate::client::Client::connect(&crate::socket(), Some(Duration::from_secs(10))) else { return };
    if let Ok(v) = c.call(r9t_protocol::rpc::Call::Status(r9t_protocol::rpc::Empty {})) {
        println!("authority: {}", v["authority"]);
    }
}

/// After `legacy-remove` there is nothing to hand back to.
fn legacy_present() -> Step {
    match std::fs::read_to_string(crate::legacy::REMOVED) {
        Ok(t) => Err(Fail(format!("eski sistem kaldırıldı ({}). Geri yükleme: tar -xpf <yedek> -C / && rm {} && systemctl daemon-reload", t.trim(), crate::legacy::REMOVED))),
        Err(_) => Ok(()),
    }
}

/// r9td gave up (start limit). Its ExecStopPost already wrote auto; repeat it once.
fn fallback_without_legacy() -> Step {
    let st = Command::new(lib_dir().join("r9td")).arg("--emergency-auto").status();
    println!("Eski sistem yok: fanlar EC otomatikte bırakıldı ({}). r9td: journalctl -u r9td", if st.is_ok_and(|s| s.success()) { "doğrulandı" } else { "ExecStopPost'a güvenildi" });
    Ok(())
}

fn rollback() -> Step {
    legacy_present()?;
    systemctl_quiet(&["stop", "r9t-switch-return.timer"]);
    systemctl_quiet(&["stop", "r9td.service"]);
    // r9td's ExecStopPost already returned the fans to auto.
    systemctl(&["start", "slayer-r9t.service"])?;
    systemctl_quiet(&["start", "r9td-shadow.service"]);
    println!("Kontrol eski serviste: {}", unit_state("is-active", "slayer-r9t.service"));
    Ok(())
}

/// Undo `--commit` at boot level as well (explicit `--rollback` only, never the fallback).
fn uncommit() -> Step {
    if !Path::new(COMMITTED).exists() {
        return Ok(());
    }
    systemctl(&["disable", "r9td.service"])?;
    systemctl(&["enable", "slayer-r9t.service", "r9td-shadow.service"])?;
    std::fs::remove_file(COMMITTED).map_err(|e| Fail(format!("{COMMITTED}: {e}")))?;
    println!("Açılışta yine eski servis başlar (kalıcı geçiş geri alındı). Masaüstü için: r9tctl desktop --rollback");
    Ok(())
}

/// Stop the legacy service and start r9td in control mode; on failure hand back.
fn take_control() -> Step {
    println!("1. Eski serviste sıcaklık hedefi durduruluyor");
    legacy_rpc("temperature_target_stop");
    println!("2. Eski servis durduruluyor (ExecStopPost fanı auto'ya alır, GPU saat kilidini sıfırlar)");
    systemctl(&["stop", "slayer-r9t.service"])?;
    println!("3. Varsayılanlar eski servis durduktan sonra yeniden okunacak (önceki deneme kayıtları siliniyor)");
    for f in STALE {
        let _ = std::fs::remove_file(f);
    }
    println!("4. r9td kontrol modunda başlıyor");
    if let Err(e) = systemctl(&["start", "r9td.service"]) {
        eprintln!("r9td başlamadı; eski servise dönülüyor, hiçbir şey kalıcı yapılmadı");
        rollback()?;
        return Err(e);
    }
    Ok(())
}

fn trial(minutes: &str) -> Step {
    legacy_present()?;
    let m: u32 = minutes.parse().ok().filter(|m| (1..=720).contains(m)).ok_or_else(|| Fail("1–720 dakika".into()))?;
    // Its return timer runs --rollback, which would undo a permanent switch.
    if Path::new(COMMITTED).exists() {
        return Err(Fail("Kalıcı geçiş yapılmış; deneme gerekmez (geri dönüş: --rollback)".into()));
    }
    take_control()?;
    let me = lib_dir().join("r9tctl");
    let st = Command::new("systemd-run")
        .args(["--unit=r9t-switch-return", &format!("--on-active={m}m"), "--timer-property=AccuracySec=1s"])
        .arg(&me)
        .args(["switch", "--rollback"])
        .stdout(std::process::Stdio::null())
        .status()
        .map_err(|e| Fail(format!("systemd-run: {e}")))?;
    if !st.success() {
        rollback()?;
        return Err(Fail("dönüş zamanlayıcısı kurulamadı; eski servise dönüldü".into()));
    }
    println!("Deneme: r9td {m} dk kontrolde; süre dolunca eski servis geri gelir.");
    print_authority();
    Ok(())
}

fn commit() -> Step {
    for u in ["r9td.service", "r9t-fallback.service"] {
        if !Command::new("systemctl").args(["cat", u]).output().is_ok_and(|o| o.status.success()) {
            return Err(Fail(format!("{u} yok: önce kurulum (paket)")));
        }
    }
    if !lib_dir().join("r9td").exists() {
        return Err(Fail(format!("{}/r9td yok", lib_dir().display())));
    }
    systemctl_quiet(&["stop", "r9t-switch-return.timer"]);
    if unit_state("is-active", "r9td.service") == "active" {
        println!("1–4. r9td zaten denetimde (deneme süresi iptal edildi)");
    } else {
        take_control()?;
    }
    println!("5. Açılış: r9td etkin; eski servis, gölge ve karşılaştırma kapalı");
    systemctl(&["enable", "r9td.service"])?;
    systemctl(&["disable", "slayer-r9t.service", "r9td-shadow.service"])?;
    systemctl_quiet(&["disable", "--now", "r9t-shadow-compare.service"]);
    std::fs::create_dir_all("/var/lib/r9t").map_err(|e| Fail(e.to_string()))?;
    let now = Command::new("date").arg("-Is").output().map(|o| o.stdout).unwrap_or_default();
    std::fs::write(COMMITTED, now).map_err(|e| Fail(format!("{COMMITTED}: {e}")))?;
    println!("Kalıcı geçiş tamam. Geri dönüş: r9tctl switch --rollback (ve r9tctl desktop --rollback).");
    print_authority();
    Ok(())
}

fn status() -> Step {
    for u in ["slayer-r9t.service", "r9td.service", "r9td-shadow.service", "r9t-switch-return.timer"] {
        println!("{u:<26} {:<9} {}", unit_state("is-active", u), unit_state("is-enabled", u));
    }
    match std::fs::read_to_string(COMMITTED) {
        Ok(t) => println!("kalıcı geçiş: {}", t.trim()),
        Err(_) => println!("kalıcı geçiş: yok"),
    }
    Ok(())
}

pub const USAGE: &str = "Kullanım: r9tctl switch --trial <dk> | --commit | --rollback | --fallback | --status";

pub fn run(args: &[String]) -> ExitCode {
    if euid() != 0 {
        eprintln!("root gerekli");
        return ExitCode::from(1);
    }
    let r = match (args.first().map(String::as_str), args.get(1)) {
        (Some("--trial"), Some(m)) => trial(m),
        (Some("--commit"), _) => commit(),
        (Some("--rollback"), _) => rollback().and_then(|_| uncommit()),
        (Some("--fallback"), _) => if Path::new(crate::legacy::REMOVED).exists() { fallback_without_legacy() } else { rollback() },
        (Some("--status"), _) => status(),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(Fail(m)) => {
            eprintln!("{m}");
            ExitCode::FAILURE
        }
    }
}
