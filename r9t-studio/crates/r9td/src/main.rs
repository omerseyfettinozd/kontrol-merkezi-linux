//! r9td — R9T Studio root daemon.
//!
//! Faz 2: only `--shadow` (read-only) is implemented; the control path arrives in
//! Faz 3 and the daemon refuses to start without `--shadow` until then.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use r9t_hal::Sysfs;
use r9td::audit::Audit;
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
}

fn parse() -> Result<Args, String> {
    let mut a = Args {
        shadow: false,
        socket: PathBuf::from(r9t_protocol::DAEMON_SOCKET),
        state_dir: PathBuf::from("/var/lib/r9t"),
        allow_uid: None,
        nvml: true,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--shadow" => a.shadow = true,
            "--no-nvml" => a.nvml = false,
            "--socket" => a.socket = it.next().ok_or("--socket yol ister")?.into(),
            "--state-dir" => a.state_dir = it.next().ok_or("--state-dir yol ister")?.into(),
            "--allow-uid" => a.allow_uid = Some(it.next().and_then(|v| v.parse().ok()).ok_or("--allow-uid sayı ister")?),
            "--version" => {
                println!("r9td {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            other => return Err(format!("bilinmeyen seçenek: {other}")),
        }
    }
    if !a.shadow {
        return Err("Kontrol modu Faz 3'te; şimdilik yalnız --shadow desteklenir.".into());
    }
    if a.allow_uid.is_some() && !a.shadow {
        return Err("--allow-uid yalnız gölge modda".into());
    }
    Ok(a)
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
    // One-time probe before the sampler takes over as the only reader.
    let capabilities = serde_json::to_value(r9t_hal::probe::probe(&fs, args.nvml)).unwrap_or_default();
    let r9t_gid = nix::unistd::Group::from_name("r9t").ok().flatten().map(|g| g.gid.as_raw());

    let sampler_shared = shared.clone();
    let cfg = Config { shadow: args.shadow, legacy_socket: PathBuf::from(r9td::legacy::SOCKET), nvml: args.nvml };
    let sampler_fs = fs.clone();
    let sampler = std::thread::Builder::new()
        .name("sampler".into())
        .spawn(move || sampler::run(sampler_shared, cfg, sampler_fs, || server::sd_notify("WATCHDOG=1")))
        .expect("sampler thread");

    let rt = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("r9td: tokio: {e}");
            return ExitCode::FAILURE;
        }
    };
    let code = rt.block_on(async {
        // Development (`--allow-uid`) without an r9t group: open the socket to that
        // user's primary group instead.
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
            shadow: args.shadow,
            r9t_gid,
            allow_uid: args.allow_uid,
            snapshots: snap_tx.clone(),
        });
        audit.lock().unwrap().record(r9td::audit::Event {
            time_ms: unix_ms(),
            method: "start".into(),
            ok: true,
            uid: None,
            pid: Some(std::process::id() as i32),
            duration_ms: 0,
            message: format!("r9td {} {} modda başladı", env!("CARGO_PKG_VERSION"), if args.shadow { "gölge" } else { "kontrol" }),
        });
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
    shared.stop.store(true, Ordering::Relaxed);
    let _ = sampler.join();
    if let Err(e) = shared.history.lock().unwrap().save(&history_path, unix_ms()) {
        eprintln!("r9td: geçmiş kaydedilemedi: {e}");
    }
    audit.lock().unwrap().record(r9td::audit::Event {
        time_ms: unix_ms(),
        method: "stop".into(),
        ok: true,
        uid: None,
        pid: Some(std::process::id() as i32),
        duration_ms: 0,
        message: "r9td durdu".into(),
    });
    if std::fs::metadata(&args.socket).is_ok() && std::env::var_os("LISTEN_FDS").is_none() {
        let _ = std::fs::remove_file(&args.socket);
    }
    code
}
