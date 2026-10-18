//! Read-only local diagnostics that need no service: process CPU/RSS sampler and
//! s2idle snapshots (port of legacy `processes.py` / `sleepdiag.py`). Nothing here
//! writes to sysfs; only the sleep baseline and exports go to the user's home.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

fn read(p: impl AsRef<Path>) -> Option<String> {
    std::fs::read_to_string(p).ok().map(|s| s.trim().to_owned())
}

fn num(p: impl AsRef<Path>) -> Option<i64> {
    read(p)?.parse().ok()
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/tmp".into())
}

/// `(comm, utime+stime, starttime)` from `/proc/<pid>/stat`.
fn parse_stat(text: &str) -> Option<(String, u64, u64)> {
    let l = text.find('(')?;
    let r = text.rfind(')')?;
    let comm = text.get(l + 1..r)?.to_owned();
    let f: Vec<&str> = text.get(r + 2..)?.split_whitespace().collect();
    Some((comm, f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?, f.get(19)?.parse().ok()?))
}

/// pid → (start time, CPU ticks) at the previous sample.
type Ticks = HashMap<u32, (u64, u64)>;

#[derive(Default)]
pub struct ProcSampler {
    last: Option<(Instant, Ticks)>,
}

impl ProcSampler {
    /// Top `limit` processes by CPU % since the previous call. The first call takes
    /// a 300 ms baseline so the table never opens with every process at 0 %.
    pub fn sample(&mut self, limit: usize) -> Value {
        if self.last.is_none() {
            self.sample_once(0);
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        self.sample_once(limit)
    }

    fn sample_once(&mut self, limit: usize) -> Value {
        let now = Instant::now();
        let page = 4096u64;
        let tck = 100.0f64;
        let mut ticks = HashMap::new();
        let mut rows = Vec::new();
        let mut unreadable = 0u32;
        if let Ok(rd) = std::fs::read_dir("/proc") {
            for e in rd.flatten() {
                let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
                let Some((comm, t, start)) = read(e.path().join("stat")).as_deref().and_then(parse_stat) else {
                    unreadable += 1;
                    continue;
                };
                let rss = read(e.path().join("statm"))
                    .and_then(|s| s.split_whitespace().nth(1).and_then(|v| v.parse::<u64>().ok()))
                    .map(|p| p * page);
                ticks.insert(pid, (start, t));
                rows.push((pid, comm, t, start, rss));
            }
        }
        let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;
        let mut out: Vec<Value> = rows
            .into_iter()
            .map(|(pid, comm, t, start, rss)| {
                let pct = match &self.last {
                    Some((then, prev)) => match prev.get(&pid) {
                        Some((s, pt)) if *s == start && t >= *pt => {
                            let dt = now.duration_since(*then).as_secs_f64().max(1e-3);
                            (t - pt) as f64 / tck / dt * 100.0
                        }
                        _ => 0.0,
                    },
                    None => 0.0,
                };
                json!({"pid": pid, "name": comm, "cpu_pct": pct, "cpu_share_pct": pct / cpus, "rss": rss, "start_time": start})
            })
            .collect();
        out.sort_by(|a, b| {
            let k = |v: &Value| (v["cpu_pct"].as_f64().unwrap_or(0.0), v["rss"].as_u64().unwrap_or(0));
            k(b).partial_cmp(&k(a)).unwrap_or(std::cmp::Ordering::Equal)
        });
        let total = out.len();
        out.truncate(limit);
        self.last = Some((now, ticks));
        json!({"processes": out, "total": total, "unreadable": unreadable, "cpus": cpus})
    }
}

/// Battery, AC and suspend counters (incl. AMD hardware-sleep residency).
pub fn sleep_snapshot() -> Value {
    let ps = Path::new("/sys/class/power_supply");
    let (mut ac, mut bat) = (None::<bool>, Value::Null);
    if let Ok(rd) = std::fs::read_dir(ps) {
        let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            match read(p.join("type")).as_deref() {
                Some("Mains") | Some("USB") => {
                    if let Some(o) = num(p.join("online")) {
                        ac = Some(ac.unwrap_or(false) || o != 0);
                    }
                }
                Some("Battery") if bat.is_null() => {
                    let volt = num(p.join("voltage_now"));
                    // Same as r9t-hal power.rs: charge × voltage_min_design (stable across load).
                    let v_design = num(p.join("voltage_min_design")).or(volt);
                    let energy = num(p.join("energy_now"))
                        .or_else(|| Some(num(p.join("charge_now"))? * v_design? / 1_000_000));
                    let power = num(p.join("power_now"))
                        .or_else(|| Some(num(p.join("current_now"))?.abs() * volt? / 1_000_000));
                    bat = json!({
                        "name": p.file_name().map(|n| n.to_string_lossy().into_owned()),
                        "status": read(p.join("status")),
                        "capacity": num(p.join("capacity")),
                        "energy_uwh": energy,
                        "power_uw": power,
                    });
                }
                _ => {}
            }
        }
    }
    let st = Path::new("/sys/power/suspend_stats");
    // max_hw_sleep is u64::MAX on this firmware: keep it as text.
    let hw = |n: &str| read(st.join(n)).and_then(|s| s.parse::<u64>().ok()).filter(|v| *v != u64::MAX);
    json!({
        "time": SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0),
        "ac_online": ac,
        "battery": bat,
        "stats": {
            "success": num(st.join("success")),
            "fail": num(st.join("fail")),
            "last_failed_dev": read(st.join("last_failed_dev")).filter(|s| !s.is_empty()),
            "last_failed_errno": read(st.join("last_failed_errno")).filter(|s| !s.is_empty() && s != "0"),
            "last_failed_step": read(st.join("last_failed_step")).filter(|s| !s.is_empty()),
            "last_hw_sleep_us": hw("last_hw_sleep"),
            "total_hw_sleep_us": hw("total_hw_sleep"),
        },
        "mem_sleep": read("/sys/power/mem_sleep"),
        "wakeup_count": num("/sys/power/wakeup_count"),
    })
}

fn state_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/state")).join("r9t-studio")
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

pub fn sleep_baseline_save(snap: &Value) -> Result<(), String> {
    write_atomic(&state_dir().join("sleepdiag.json"), &serde_json::to_vec(snap).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

pub fn sleep_baseline_load() -> Value {
    std::fs::read(state_dir().join("sleepdiag.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null)
}

/// `XDG_DOWNLOAD_DIR` from user-dirs.dirs, else `$HOME`.
fn download_dir() -> PathBuf {
    let h = home();
    let cfg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| h.join(".config"));
    read(cfg.join("user-dirs.dirs"))
        .and_then(|t| {
            t.lines().find_map(|l| {
                let v = l.trim().strip_prefix("XDG_DOWNLOAD_DIR=")?.trim_matches('"');
                Some(PathBuf::from(v.replace("$HOME", &h.to_string_lossy())))
            })
        })
        .filter(|p| p.is_dir())
        .unwrap_or(h)
}

/// Writes `content` to the download directory; the name is reduced to a safe stem.
pub fn export_file(name: &str, content: &str) -> Result<String, String> {
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, "json"));
    let safe: String = stem.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).take(64).collect();
    let ext = if matches!(ext, "json" | "csv") { ext } else { "json" };
    let path = download_dir().join(format!("{}.{ext}", if safe.is_empty() { "r9t" } else { &safe }));
    write_atomic(&path, content.as_bytes()).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

pub fn keymap() -> Value {
    serde_json::from_str(include_str!("../../../data/keymap-tr-q.json")).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_with_spaces_in_comm() {
        let t = "1234 (Web Content) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 999 20";
        assert_eq!(parse_stat(t), Some(("Web Content".into(), 11 + 12, 999)));
        assert_eq!(parse_stat("garbage"), None);
    }

    #[test]
    fn sampler_reports_self() {
        let mut s = ProcSampler::default();
        let v = s.sample(5000);
        let me = std::process::id();
        assert!(v["processes"].as_array().unwrap().iter().any(|p| p["pid"] == me));
        // First call already carries a baseline: this busy thread shows up with CPU > 0.
        let mut fresh = ProcSampler::default();
        let spin = std::thread::spawn(|| {
            let t = Instant::now();
            while t.elapsed() < std::time::Duration::from_millis(600) {
                std::hint::black_box(0u64);
            }
        });
        let v = fresh.sample(5000);
        spin.join().unwrap();
        let mine = v["processes"].as_array().unwrap().iter().find(|p| p["pid"] == me).unwrap();
        assert!(mine["cpu_pct"].as_f64().unwrap() > 0.0, "{mine}");
    }

    #[test]
    fn keymap_has_126_keys() {
        assert_eq!(keymap()["keys"].as_array().map(Vec::len), Some(126));
    }
}
