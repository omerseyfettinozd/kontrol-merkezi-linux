//! r9td — R9T Studio root daemon.
//!
//! `--shadow`: read-only next to the legacy service (Faz 2).
//! default: control mode (single writer). Control is opened only when the legacy
//! service does not answer, `/run/r9td/control.lock` is free and r9t_fan is 0.2.0;
//! otherwise the daemon stays read-only and reports why.
//! `--emergency-auto`: ExecStopPost helper that returns the fans to EC control.

use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use r9t_hal::fan::{FanBridge, FanCommand, SysfsFan};
use r9t_hal::Sysfs;
use r9t_protocol::snapshot::{AuthoritySnapshot, AuthorityState};
use r9td::audit::{Audit, Event};
use r9td::control::{self, Control};
use r9td::history::History;
use r9td::sampler::{self, unix_ms, Config, Metrics, Shared};
use r9td::server::{self, Server};
use r9td::state::Live;

struct Args {
    shadow: bool,
    socket: PathBuf,
    state_dir: PathBuf,
    allow_uid: Option<u32>,
    nvml: bool,
    legacy_socket: PathBuf,
}

fn parse() -> Result<Args, String> {
    let mut a = Args {
        shadow: false,
        socket: PathBuf::from(r9t_protocol::DAEMON_SOCKET),
        state_dir: PathBuf::from("/var/lib/r9t"),
        allow_uid: None,
        nvml: true,
        legacy_socket: PathBuf::from(r9td::legacy::SOCKET),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--shadow" => a.shadow = true,
            "--no-nvml" => a.nvml = false,
            "--socket" => a.socket = it.next().ok_or("--socket yol ister")?.into(),
            "--state-dir" => a.state_dir = it.next().ok_or("--state-dir yol ister")?.into(),
            "--legacy-socket" => a.legacy_socket = it.next().ok_or("--legacy-socket yol ister")?.into(),
            "--allow-uid" => a.allow_uid = Some(it.next().and_then(|v| v.parse().ok()).ok_or("--allow-uid sayı ister")?),
            "--emergency-auto" => std::process::exit(emergency_auto()),
            "--version" => {
                println!("r9td {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            other => return Err(format!("bilinmeyen seçenek: {other}")),
        }
    }
    if a.allow_uid.is_some() && !a.shadow {
        return Err("--allow-uid yalnız gölge modda".into());
    }
    Ok(a)
}

/// ExecStopPost: fans back to firmware control, verified; tuxedo_io W_UW_FANAUTO as
/// the last resort (after the mandatory hardware check).
fn emergency_auto() -> i32 {
    let fs = Sysfs::from_env();
    restore_defaults(&fs, Path::new("/var/lib/r9t"));
    let fan = SysfsFan::new(fs.clone());
    if fan.loaded() && fan.control(FanCommand::Auto).is_ok() {
        std::thread::sleep(Duration::from_millis(300));
        if fan.status().map(|s| s.is_auto()).unwrap_or(false) {
            eprintln!("r9td --emergency-auto: fanlar EC otomatiğinde (doğrulandı)");
            return 0;
        }
    }
    match r9t_hal::tuxedo::RealDevice::open(&fs, true)
        .and_then(|d| r9t_hal::tuxedo::Unverified::new(d).verify())
        .and_then(|mut v| v.emergency_fan_auto())
    {
        Ok(()) => {
            eprintln!("r9td --emergency-auto: köprü doğrulanamadı, W_UW_FANAUTO gönderildi");
            0
        }
        Err(e) => {
            eprintln!("r9td --emergency-auto: başarısız: {e}");
            1
        }
    }
}

/// Invariant 2 after a crash: CPU/GPU limits back to the persisted defaults, GPU clock
/// lock released (ownership marker), keyboard HID buffer released.
fn restore_defaults(fs: &Sysfs, state_dir: &Path) {
    use r9td::control::power::{self, GpuClocks, PowerState, Ppd};
    if let Ok(r) = r9t_hal::rgb::Rgb::discover(fs.clone()) {
        let _ = r.release_buffer();
    }
    let cpu = r9t_hal::cpufreq::Cpufreq::new(fs.clone());
    let mut gpu = GpuClocks::new(r9t_hal::gpu::Nvml::new(fs.clone()), state_dir);
    if let Err(e) = gpu.recover() {
        eprintln!("r9td --emergency-auto: GPU saat kilidi sıfırlanamadı: {e}");
    }
    let Some(base) = std::fs::read(state_dir.join(control::BASELINE)).ok().and_then(|b| serde_json::from_slice::<PowerState>(&b).ok()) else {
        return;
    };
    match power::restore(&cpu, &Ppd::new(), &mut gpu, &base) {
        Ok(()) => eprintln!("r9td --emergency-auto: CPU/GPU sınırları kayıtlı varsayılana döndü"),
        Err(e) => eprintln!("r9td --emergency-auto: varsayılan geri yükleme eksik: {e}"),
    }
}

/// Exclusive control lock (safety invariant 6). Held for the process lifetime.
fn take_lock(dir: &Path) -> Result<std::fs::File, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("control.lock"))
        .map_err(|e| e.to_string())?;
    // SAFETY: plain flock on a descriptor we own.
    let rc = unsafe { nix::libc::flock(f.as_raw_fd(), nix::libc::LOCK_EX | nix::libc::LOCK_NB) };
    if rc != 0 {
        return Err("control.lock başka bir r9td'de".into());
    }
    Ok(f)
}

/// The legacy socket is the desktop user's (0600): EACCES means "present", so only
/// "missing" or "connection refused" count as not running.
fn legacy_running(path: &Path) -> bool {
    match std::os::unix::net::UnixStream::connect(path) {
        Ok(_) => true,
        Err(e) => !matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused),
    }
}

fn record(audit: &Mutex<Audit>, method: &str, ok: bool, message: String) {
    audit.lock().unwrap_or_else(|p| p.into_inner()).record(Event {
        time_ms: unix_ms(),
        method: method.into(),
        ok,
        uid: None,
        pid: Some(std::process::id() as i32),
        duration_ms: 0,
        message,
    });
}

fn main() -> ExitCode {
    let args = match parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("r9td: {e}");
            return ExitCode::from(2);
        }
    };
    let fs = Sysfs::from_env();
    let _ = std::fs::create_dir_all(&args.state_dir);
    let history_path = args.state_dir.join("history.bin");
    let shared = Arc::new(Shared {
        live: RwLock::new(Live::default()),
        history: Mutex::new(History::load(&history_path, unix_ms())),
        metrics: Metrics::default(),
        stop: AtomicBool::new(false),
        owns_fans: AtomicBool::new(false),
    });
    let audit = Arc::new(Mutex::new(Audit::new(Some(args.state_dir.join("actions.jsonl")))));
    let capabilities = serde_json::to_value(r9t_hal::probe::probe(&fs, args.nvml)).unwrap_or_default();
    let r9t_gid = nix::unistd::Group::from_name("r9t").ok().flatten().map(|g| g.gid.as_raw());

    // ---- control mode preconditions ----
    let mut _lock = None;
    let mut refused: Option<String> = None;
    let mut control_parts = None;
    if !args.shadow {
        let run_dir = args.socket.parent().unwrap_or(Path::new("/run/r9td")).to_path_buf();
        let pre = if legacy_running(&args.legacy_socket) {
            Err("Eski servis (slayer-r9t) çalışıyor; kontrol reddedildi.".to_string())
        } else {
            take_lock(&run_dir)
        };
        match pre.and_then(|l| {
            _lock = Some(l);
            Control::start(shared.clone(), fs.clone(), &args.state_dir, audit.clone())
        }) {
            Ok(parts) => control_parts = Some(parts),
            Err(reason) => {
                eprintln!("r9td: kontrol açılmadı, salt okunur: {reason}");
                refused = Some(reason);
            }
        }
    }
    let control_mode = control_parts.is_some();
    if let Some(reason) = &refused {
        shared.live.write().unwrap_or_else(|p| p.into_inner()).authority = Some(AuthoritySnapshot {
            state: AuthorityState::Refused,
            reason: reason.clone(),
            active_profile: None,
            guard_active: false,
            session: None,
        });
    }

    let sampler_shared = shared.clone();
    let cfg = Config { shadow: !control_mode, legacy_socket: args.legacy_socket.clone(), nvml: args.nvml };
    let sampler_fs = fs.clone();
    // Watchdog comes from the control loop in control mode (invariant 1).
    let sampler = std::thread::Builder::new()
        .name("sampler".into())
        .spawn(move || {
            sampler::run(sampler_shared, cfg, sampler_fs, move || {
                if !control_mode {
                    server::sd_notify("WATCHDOG=1");
                }
            })
        })
        .expect("sampler thread");

    let mut handle = None;
    let mut control_thread = None;
    if let Some((ctl, h, rx)) = control_parts {
        handle = Some(h.clone());
        control_thread = Some(std::thread::Builder::new().name("control".into()).spawn(move || ctl.run(rx, server::sd_notify)).expect("control thread"));
        let sleep_handle = h.clone();
        let _ = std::thread::Builder::new().name("sleep".into()).spawn(move || control::sleep::run(move |entering| sleep_handle.sleep(entering)));
    }

    let rt = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("r9td: tokio: {e}");
            return ExitCode::FAILURE;
        }
    };
    let code = rt.block_on(async {
        let socket_gid = r9t_gid.or_else(|| {
            args.allow_uid
                .and_then(|uid| nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid)).ok().flatten())
                .map(|u| u.gid.as_raw())
        });
        let listener = match server::listener(&args.socket, socket_gid) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("r9td: soket {}: {e}", args.socket.display());
                return ExitCode::FAILURE;
            }
        };
        let (snap_tx, _) = tokio::sync::broadcast::channel(r9t_protocol::SUBSCRIBER_BACKLOG);
        let srv = Arc::new(Server {
            shared: shared.clone(),
            audit: audit.clone(),
            capabilities,
            shadow: !control_mode,
            r9t_gid,
            allow_uid: args.allow_uid,
            snapshots: snap_tx.clone(),
            control: handle.clone(),
            refused: refused.clone(),
            polkit: if control_mode { zbus::blocking::Connection::system().ok() } else { None },
            conn_seq: AtomicU64::new(1),
        });
        let mode = if control_mode { "kontrol" } else if refused.is_some() { "salt okunur (kontrol reddedildi)" } else { "gölge" };
        record(&audit, "start", true, format!("r9td {} {mode} modda başladı", env!("CARGO_PKG_VERSION")));
        let publisher_shared = shared.clone();
        tokio::spawn(async move {
            let mut iv = tokio::time::interval(Duration::from_secs(1));
            loop {
                iv.tick().await;
                if snap_tx.receiver_count() > 0 {
                    if let Some(line) = server::snapshot_line(&publisher_shared) {
                        let _ = snap_tx.send(line);
                    }
                }
            }
        });
        tokio::spawn(srv.serve(listener));
        server::sd_notify("READY=1");
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM");
        tokio::select! {
            _ = term.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
        server::sd_notify("STOPPING=1");
        ExitCode::SUCCESS
    });
    // Invariant 2: hand everything back before the sampler stops.
    if let Some(h) = handle {
        h.shutdown();
    }
    if let Some(t) = control_thread {
        let _ = t.join();
    }
    shared.stop.store(true, Ordering::Relaxed);
    let _ = sampler.join();
    if let Err(e) = shared.history.lock().unwrap_or_else(|p| p.into_inner()).save(&history_path, unix_ms()) {
        eprintln!("r9td: geçmiş kaydedilemedi: {e}");
    }
    record(&audit, "stop", true, "r9td durdu".into());
    if std::fs::metadata(&args.socket).is_ok() && std::env::var_os("LISTEN_FDS").is_none() {
        let _ = std::fs::remove_file(&args.socket);
    }
    code
}
