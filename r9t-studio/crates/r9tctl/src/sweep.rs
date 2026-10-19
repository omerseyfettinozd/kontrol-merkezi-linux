//! `r9tctl fan-sweep [--out dosya] [--steps 45,40,...]` (plan Faz 5, r9t_fan 0.3.0):
//! the low-duty sweep. Root, r9td stopped (this takes its `control.lock`, safety
//! invariant 6), AC on, fans in EC auto, CPU < 70 °C and GPU < 65 °C.
//!
//! Both fans step down from 45 % with `allow_low_duty` on. Each step lasts 24 s; the
//! first 8 s are discarded as spin-down, then a sample is taken every 2 s. The lease
//! is renewed every 3 s. The module enforces its temperature floor and hands control
//! back to the EC when a fan stays under 500 rpm for two ticks (error -32). That is the
//! stall this sweep looks for: it stops descending there. Stops: CPU ≥ 80 or GPU ≥
//! 75 °C, a status error, an unexpected return to auto, Ctrl-C/SIGTERM. On every exit
//! the tool writes `auto` and `allow_low_duty=0` and checks that mode=0 and the EC
//! manual bit is clear.
//!
//! The proposed `min_duty` is max(25, stall duty + 10), or 25 when nothing stalled
//! down to 20 %. The plan asks for spin-up + 5 %; a fan here only stops through this
//! sweep (the kernel restarts it in auto), so the stall point + 10 is the conservative
//! stand-in. The tool only prints the modprobe line; it does not apply it.

use std::os::fd::AsRawFd;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use r9t_hal::fan::{FanBridge, FanCommand, FanStatus2, SysfsFan};
use r9t_hal::power::Power;
use r9t_hal::Sysfs;

const STEPS: [u8; 6] = [45, 40, 35, 30, 25, 20];
const STEP: Duration = Duration::from_secs(24);
const SETTLE: Duration = Duration::from_secs(8);
const SAMPLE_EVERY: Duration = Duration::from_secs(2);
const RENEW_EVERY: Duration = Duration::from_secs(3);
const START_MAX: (u32, u32) = (70, 65);
const STOP_AT: (u32, u32) = (80, 75);
const EPIPE: i32 = -32;
const LEGACY_SOCK: &str = "/run/slayer-r9t/control.sock";

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: nix::libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

fn trap_signals() {
    use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
    let act = SigAction::new(SigHandler::Handler(on_signal), SaFlags::empty(), SigSet::empty());
    for s in [Signal::SIGINT, Signal::SIGTERM] {
        // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
        let _ = unsafe { sigaction(s, &act) };
    }
}

/// What one status2 reading means for the running step.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Continue,
    /// The module returned the fans to the EC because one stalled.
    Stalled,
    Stop(String),
}

pub fn judge(s: &FanStatus2) -> Verdict {
    let v = &s.v1;
    if v.cpu_temp >= STOP_AT.0 || v.gpu_temp >= STOP_AT.1 {
        return Verdict::Stop(format!("sıcaklık {} / {} °C (sınır {} / {})", v.cpu_temp, v.gpu_temp, STOP_AT.0, STOP_AT.1));
    }
    if v.mode == 0 && v.error == EPIPE {
        return Verdict::Stalled;
    }
    if v.mode != 1 {
        return Verdict::Stop(format!("beklenmeyen mod {} (error={})", v.mode, v.error));
    }
    Verdict::Continue
}

pub fn median(xs: &[i32]) -> Option<i32> {
    let mut v = xs.to_vec();
    v.sort_unstable();
    v.get(v.len() / 2).copied()
}

/// max(25, stall + 10), capped at 50; 25 when no step stalled.
pub fn proposal(stalled_at: Option<u8>) -> u8 {
    stalled_at.map_or(25, |d| (d + 10).clamp(25, 50))
}

fn wait(d: Duration) -> bool {
    let end = Instant::now() + d;
    while Instant::now() < end {
        if STOP.load(Ordering::SeqCst) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(250).min(end.saturating_duration_since(Instant::now())));
    }
    !STOP.load(Ordering::SeqCst)
}

fn take_lock() -> Result<std::fs::File, String> {
    std::fs::create_dir_all("/run/r9td").map_err(|e| format!("/run/r9td: {e}"))?;
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open("/run/r9td/control.lock")
        .map_err(|e| format!("control.lock: {e}"))?;
    // SAFETY: plain flock on a descriptor we own.
    let rc = unsafe { nix::libc::flock(f.as_raw_fd(), nix::libc::LOCK_EX | nix::libc::LOCK_NB) };
    if rc != 0 {
        return Err("control.lock tutuluyor: önce sudo systemctl stop r9td".into());
    }
    Ok(f)
}

fn preflight(fan: &SysfsFan, fs: &Sysfs) -> Result<FanStatus2, String> {
    if fan.version().as_deref() != Some("0.3.0") {
        return Err(format!("r9t_fan 0.3.0 gerekli, yüklü: {}", fan.version().unwrap_or_else(|| "yok".into())));
    }
    if std::os::unix::net::UnixStream::connect(LEGACY_SOCK).is_ok() {
        return Err("eski servis çalışıyor (slayer-r9t)".into());
    }
    if Power::new(fs.clone()).ac_online() != Some(true) {
        return Err("AC bağlı değil".into());
    }
    let s = fan.status2().map_err(|e| format!("status2: {e}"))?;
    if !s.v1.is_auto() {
        return Err(format!("fanlar EC otomatikte değil (mode={}, mode_byte={})", s.v1.mode, s.v1.mode_byte));
    }
    if s.v1.cpu_temp >= START_MAX.0 || s.v1.gpu_temp >= START_MAX.1 {
        return Err(format!("çok sıcak: {} / {} °C (başlangıç sınırı {} / {})", s.v1.cpu_temp, s.v1.gpu_temp, START_MAX.0, START_MAX.1));
    }
    Ok(s)
}

/// Every exit path: auto, low duty off, then mode=0 and EC bit 0x40 clear (3 tries).
fn restore(fan: &SysfsFan) -> bool {
    let _ = fan.set_allow_low_duty(false);
    for _ in 0..3 {
        if fan.control(FanCommand::Auto).is_ok() {
            std::thread::sleep(Duration::from_millis(500));
            if fan.status().is_ok_and(|s| s.is_auto()) {
                return true;
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    false
}

struct StepResult {
    duty: u8,
    cpu_rpm: Vec<i32>,
    gpu_rpm: Vec<i32>,
    floors: (u32, u32),
    temps: Vec<(u32, u32)>,
}

impl StepResult {
    fn to_json(&self, stalled: bool) -> Value {
        json!({
            "duty": self.duty, "samples": self.cpu_rpm.len(), "stalled": stalled,
            "cpu_rpm_median": median(&self.cpu_rpm), "gpu_rpm_median": median(&self.gpu_rpm),
            "cpu_rpm": self.cpu_rpm, "gpu_rpm": self.gpu_rpm, "floors": [self.floors.0, self.floors.1],
            "temps": self.temps.iter().map(|t| [t.0, t.1]).collect::<Vec<_>>(),
        })
    }
}

/// One step. Ok(true) = stalled here, Ok(false) = held, Err = stop reason.
fn step(fan: &SysfsFan, duty: u8, r: &mut StepResult) -> Result<bool, String> {
    fan.control_low(duty, duty).map_err(|e| format!("manual {duty} {duty}: {e}"))?;
    let t0 = Instant::now();
    let (mut renewed, mut sampled) = (t0, t0);
    while t0.elapsed() < STEP {
        if !wait(Duration::from_secs(1)) {
            return Err("kullanıcı durdurdu".into());
        }
        let s = fan.status2().map_err(|e| format!("status2: {e}"))?;
        match judge(&s) {
            Verdict::Stalled => return Ok(true),
            Verdict::Stop(why) => return Err(why),
            Verdict::Continue => {}
        }
        if renewed.elapsed() >= RENEW_EVERY {
            fan.control_low(duty, duty).map_err(|e| format!("lease: {e}"))?;
            renewed = Instant::now();
        }
        if t0.elapsed() >= SETTLE && sampled.elapsed() >= SAMPLE_EVERY {
            r.cpu_rpm.push(s.v1.cpu_rpm);
            r.gpu_rpm.push(s.v1.gpu_rpm);
            r.temps.push((s.v1.cpu_temp, s.v1.gpu_temp));
            r.floors = (s.cpu_floor, s.gpu_floor);
            sampled = Instant::now();
        }
    }
    Ok(false)
}

fn parse_steps(v: &str) -> Option<Vec<u8>> {
    let steps: Vec<u8> = v.split(',').map(|s| s.trim().parse().ok()).collect::<Option<_>>()?;
    (!steps.is_empty() && steps.iter().all(|d| (20..50).contains(d)) && steps.windows(2).all(|w| w[0] > w[1])).then_some(steps)
}

pub fn run(args: &[String]) -> ExitCode {
    let (mut out, mut steps) = ("fan-sweep.json".to_owned(), STEPS.to_vec());
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match (a.as_str(), it.next()) {
            ("--out", Some(v)) => out = v.clone(),
            ("--steps", Some(v)) => match parse_steps(v) {
                Some(s) => steps = s,
                None => {
                    eprintln!("--steps: azalan, 20–49 arası, virgülle (ör. 45,40,35)");
                    return ExitCode::from(2);
                }
            },
            _ => {
                eprintln!("Kullanım: r9tctl fan-sweep [--out dosya] [--steps 45,40,35,30,25,20]");
                return ExitCode::from(2);
            }
        }
    }
    if crate::switch::euid() != 0 {
        eprintln!("root gerekli: sudo r9tctl fan-sweep");
        return ExitCode::FAILURE;
    }
    let fs = Sysfs::from_env();
    let fan = SysfsFan::new(fs.clone());
    let _lock = match take_lock() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let start = match preflight(&fan, &fs) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ön koşul: {e}");
            return ExitCode::FAILURE;
        }
    };
    trap_signals();
    println!("başlangıç: CPU {} °C, GPU {} °C, {} / {} rpm (auto)", start.v1.cpu_temp, start.v1.gpu_temp, start.v1.cpu_rpm, start.v1.gpu_rpm);
    if let Err(e) = fan.set_allow_low_duty(true) {
        eprintln!("allow_low_duty açılamadı: {e}");
        return ExitCode::FAILURE;
    }
    let (mut rows, mut stalled_at, mut stop_reason) = (Vec::new(), None, None);
    for &duty in &steps {
        let mut r = StepResult { duty, cpu_rpm: vec![], gpu_rpm: vec![], floors: (0, 0), temps: vec![] };
        let res = step(&fan, duty, &mut r);
        let stalled = matches!(res, Ok(true));
        println!(
            "%{duty}: CPU {} rpm, GPU {} rpm, taban {}/{} %{}",
            median(&r.cpu_rpm).map_or("—".into(), |v| v.to_string()),
            median(&r.gpu_rpm).map_or("—".into(), |v| v.to_string()),
            r.floors.0,
            r.floors.1,
            if stalled { " — DURDU, modül EC'ye bıraktı" } else { "" }
        );
        rows.push(r.to_json(stalled));
        match res {
            Ok(true) => {
                stalled_at = Some(duty);
                break;
            }
            Ok(false) => {}
            Err(why) => {
                stop_reason = Some(why);
                break;
            }
        }
    }
    let restored = restore(&fan);
    let end = fan.status2().ok();
    let complete = stop_reason.is_none();
    let min_duty = complete.then(|| proposal(stalled_at));
    let doc = json!({
        "tool": "r9tctl fan-sweep", "module_version": fan.version(),
        "start": {"cpu_temp": start.v1.cpu_temp, "gpu_temp": start.v1.gpu_temp, "cpu_rpm": start.v1.cpu_rpm, "gpu_rpm": start.v1.gpu_rpm},
        "protocol": {"step_s": STEP.as_secs(), "settle_s": SETTLE.as_secs(), "sample_s": SAMPLE_EVERY.as_secs(), "start_max_c": [START_MAX.0, START_MAX.1], "stop_c": [STOP_AT.0, STOP_AT.1]},
        "steps": rows, "stalled_at": stalled_at, "stop_reason": stop_reason, "complete": complete,
        "proposed_min_duty": min_duty, "restored_auto": restored,
        "end": end.map(|s| json!({"mode": s.v1.mode, "mode_byte": s.v1.mode_byte, "error": s.v1.error, "stalls": s.stalls, "allow_low_duty": s.allow_low_duty})),
    });
    if let Err(e) = std::fs::write(&out, serde_json::to_string_pretty(&doc).unwrap_or_default()) {
        eprintln!("{out}: {e}");
    }
    println!("sonuç: {out}");
    if !restored {
        eprintln!("UYARI: auto doğrulanamadı. Fanları kontrol et; r9td'yi başlatmak auto'ya döndürür: sudo systemctl start r9td");
        return ExitCode::FAILURE;
    }
    println!("fanlar EC otomatikte (mode=0, bit 0x40 temiz), allow_low_duty=0");
    match (stop_reason, min_duty) {
        (Some(why), _) => {
            println!("tarama tamamlanmadı: {why}. Öneri yok.");
            ExitCode::FAILURE
        }
        (None, Some(d)) => {
            println!("önerilen min_duty: {d} (%{} ile durma)", stalled_at.map_or("20'ye kadar durma yok".into(), |s| s.to_string()));
            println!("uygulamak için: echo 'options r9t_fan min_duty={d}' | sudo tee /etc/modprobe.d/r9t-fan.conf  (sonra yeniden başlat)");
            println!("r9td'yi geri başlat: sudo systemctl start r9td");
            ExitCode::SUCCESS
        }
        (None, None) => ExitCode::SUCCESS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1: &str = "mode=1 cpu_target=40 gpu_target=40 cpu_rpm=2400 gpu_rpm=2450 cpu_temp=66 gpu_temp=60 \
                      mode_byte=192 cpu_pwm=80 gpu_pwm=80 lease_ms=14000 error=0 thermal=0";

    fn s2(v1: &str) -> FanStatus2 {
        FanStatus2::parse(&format!("{v1} sample_age_ms=10 cache_ms=1000 allow_low_duty=1 min_duty=50 cpu_floor=20 gpu_floor=20 stalls=0")).unwrap()
    }

    #[test]
    fn judges_steps() {
        assert_eq!(judge(&s2(V1)), Verdict::Continue);
        assert!(matches!(judge(&s2(&V1.replace("cpu_temp=66", "cpu_temp=80"))), Verdict::Stop(_)));
        assert!(matches!(judge(&s2(&V1.replace("gpu_temp=60", "gpu_temp=75"))), Verdict::Stop(_)));
        let stalled = V1.replace("mode=1", "mode=0").replace("error=0", "error=-32").replace("mode_byte=192", "mode_byte=128");
        assert_eq!(judge(&s2(&stalled)), Verdict::Stalled);
        let expired = stalled.replace("error=-32", "error=-110");
        assert!(matches!(judge(&s2(&expired)), Verdict::Stop(_)), "lease expiry is not a stall");
        assert!(matches!(judge(&s2(&V1.replace("mode=1", "mode=2"))), Verdict::Stop(_)));
    }

    #[test]
    fn proposes_a_conservative_floor() {
        assert_eq!(proposal(None), 25);
        assert_eq!(proposal(Some(20)), 30);
        assert_eq!(proposal(Some(30)), 40);
        assert_eq!(proposal(Some(45)), 50);
        assert_eq!(median(&[3, 1, 2]), Some(2));
        assert_eq!(median(&[]), None);
    }

    #[test]
    fn steps_must_descend_below_50() {
        assert_eq!(parse_steps("45,40,35"), Some(vec![45, 40, 35]));
        assert_eq!(parse_steps("40,45"), None);
        assert_eq!(parse_steps("50,45"), None);
        assert_eq!(parse_steps("30,19"), None);
        assert_eq!(parse_steps(""), None);
    }
}
