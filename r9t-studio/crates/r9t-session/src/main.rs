//! r9t-session — per-user agent for desktop work the root daemon must never do:
//! display refresh rate and brightness, game and idle detection, audio, radios
//! and the GPU wake inspector. Socket: `$XDG_RUNTIME_DIR/r9t/session.sock` (0600).

mod audio;
mod brightness;
mod daemon;
mod game;
mod hypr;
mod idle;
mod kde;
mod procs;
mod radio;

use std::collections::HashMap;
use std::os::fd::AsFd;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

use r9t_protocol::rpc::{code, Response, RpcError, SessionEvent};
use r9t_protocol::session::{decode_request, Desktop, SessionCall};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Config {
    idle_timeout_s: u32,
    /// Lowest refresh rate on battery, restored on AC — skipped while
    /// `~/dotfiles/bin/power-watch.sh` is running (plan decision 6.3).
    low_hz_on_battery: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { idle_timeout_s: 120, low_hz_on_battery: true }
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("r9t-studio/session.json")
}

struct Agent {
    desktop: Desktop,
    tx: mpsc::Sender<daemon::Outgoing>,
    connected: Arc<AtomicBool>,
    daemon_error: Arc<Mutex<String>>,
    idle_timeout: Arc<AtomicU32>,
    idle: Arc<AtomicBool>,
    game: Arc<Mutex<Option<String>>>,
    audio_on: Arc<AtomicBool>,
    hz_policy: Arc<Mutex<String>>,
    nvml: Arc<r9t_hal::gpu::Nvml>,
    config: Mutex<Config>,
}

fn desktop() -> Desktop {
    if hypr::dir().is_some() {
        Desktop::Hyprland
    } else if std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().contains("KDE") || std::env::var_os("KDE_FULL_SESSION").is_some() {
        Desktop::Kde
    } else {
        Desktop::Other
    }
}

fn outputs(d: Desktop) -> Result<Vec<r9t_protocol::session::OutputState>, String> {
    match d {
        Desktop::Hyprland => hypr::outputs().map_err(|e| e.to_string()),
        Desktop::Kde => kde::outputs(),
        Desktop::Other => Err("desteklenmeyen masaüstü".into()),
    }
}

fn set_refresh(d: Desktop, output: &str, hz: u32) -> Result<f32, String> {
    match d {
        Desktop::Hyprland => hypr::set_refresh(output, hz),
        Desktop::Kde => kde::set_refresh(output, hz),
        Desktop::Other => Err("desteklenmeyen masaüstü".into()),
    }
}

fn power_watch_running() -> bool {
    std::fs::read_dir("/proc").into_iter().flatten().flatten().any(|e| {
        std::fs::read(e.path().join("cmdline")).is_ok_and(|c| String::from_utf8_lossy(&c).contains("power-watch.sh"))
    })
}

fn on_ac() -> Option<bool> {
    for e in std::fs::read_dir("/sys/class/power_supply").ok()?.flatten() {
        let p = e.path();
        if std::fs::read_to_string(p.join("type")).ok()?.trim() == "Mains" {
            return Some(std::fs::read_to_string(p.join("online")).ok()?.trim() == "1");
        }
    }
    None
}

impl Agent {
    fn status(&self) -> Value {
        json!({
            "desktop": self.desktop,
            "outputs": outputs(self.desktop).unwrap_or_default(),
            "brightness_pct": brightness::get(),
            "volume": audio::get(audio::SINK),
            "mic": audio::get(audio::SOURCE),
            "wifi": radio::enabled(radio::Radio::Wifi),
            "bluetooth": radio::enabled(radio::Radio::Bluetooth),
            "idle": self.idle.load(Ordering::Relaxed),
            "idle_timeout_s": self.idle_timeout.load(Ordering::Relaxed),
            "game": *self.game.lock().unwrap_or_else(|p| p.into_inner()),
            "audio_reactive": self.audio_on.load(Ordering::Relaxed),
            "hz_policy": *self.hz_policy.lock().unwrap_or_else(|p| p.into_inner()),
            "r9td_connected": self.connected.load(Ordering::Relaxed),
            "r9td_error": *self.daemon_error.lock().unwrap_or_else(|p| p.into_inner()),
        })
    }

    fn save_config(&self) {
        let c = self.config.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let p = config_path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, serde_json::to_vec_pretty(&c).unwrap_or_default());
    }

    /// Blocking handler (run inside spawn_blocking).
    fn handle(self: &Arc<Self>, call: SessionCall) -> Result<Value, RpcError> {
        let fail = |m: String| RpcError::new(code::HARDWARE, m);
        match call {
            SessionCall::Status(_) => Ok(self.status()),
            SessionCall::SetRefreshRate(p) => {
                let hz = set_refresh(self.desktop, &p.output, p.hz).map_err(fail)?;
                Ok(json!({"verified": true, "message": format!("{} {hz:.0} Hz", p.output)}))
            }
            SessionCall::SetBrightness(p) => Ok(json!({"verified": true, "brightness_pct": brightness::set(p.percent).map_err(fail)?})),
            SessionCall::SetVolume(p) => Ok(json!({"verified": true, "volume": audio::set(audio::SINK, p.percent).map_err(fail)?})),
            SessionCall::SetMicVolume(p) => Ok(json!({"verified": true, "mic": audio::set(audio::SOURCE, p.percent).map_err(fail)?})),
            SessionCall::Wifi(p) => radio::set(radio::Radio::Wifi, p.enabled).map(|_| json!({"verified": true})).map_err(fail),
            SessionCall::Bluetooth(p) => radio::set(radio::Radio::Bluetooth, p.enabled).map(|_| json!({"verified": true})).map_err(fail),
            SessionCall::Airplane(p) => radio::airplane(p.enabled).map(|_| json!({"verified": true})).map_err(fail),
            SessionCall::GpuProcesses(_) => Ok(procs::gpu_processes(&self.nvml)),
            SessionCall::KillProcess(p) => procs::kill(p.pid, p.start_time).map(|m| json!({"message": m})).map_err(|m| RpcError::new(code::UNAUTHORIZED, m)),
            SessionCall::AudioReactive(p) => {
                if p.enabled && !self.audio_on.swap(true, Ordering::AcqRel) {
                    let me = self.clone();
                    std::thread::spawn(move || audio_loop(me));
                } else if !p.enabled {
                    self.audio_on.store(false, Ordering::Release);
                }
                Ok(json!({"audio_reactive": p.enabled}))
            }
            SessionCall::SetIdleTimeout(p) => {
                if p.seconds > 3600 {
                    return Err(RpcError::new(code::INVALID_PARAMS, "0–3600 sn"));
                }
                self.idle_timeout.store(p.seconds, Ordering::Relaxed);
                self.config.lock().unwrap_or_else(|p| p.into_inner()).idle_timeout_s = p.seconds;
                self.save_config();
                Ok(json!({"idle_timeout_s": p.seconds}))
            }
        }
    }
}

fn audio_loop(agent: Arc<Agent>) {
    let mut cap = match audio::Capture::start() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("r9t-session: {e}");
            agent.audio_on.store(false, Ordering::Release);
            return;
        }
    };
    let mut peaks = [1e-4f32; 7];
    while agent.audio_on.load(Ordering::Acquire) {
        let Some(samples) = cap.next() else { break };
        // Dropped when r9td is slow: frames are only useful in real time.
        let _ = agent.tx.try_send(daemon::Outgoing::Frame(audio::frame(&samples, &mut peaks)));
    }
    agent.audio_on.store(false, Ordering::Release);
}

async fn serve_client(agent: Arc<Agent>, stream: UnixStream) {
    let ok = nix::sys::socket::getsockopt(&stream.as_fd(), nix::sys::socket::sockopt::PeerCredentials)
        .is_ok_and(|c| c.uid() == nix::unistd::getuid().as_raw());
    let (rd, mut wr) = stream.into_split();
    if !ok {
        let r = Response::err(None, RpcError::new(code::UNAUTHORIZED, "farklı kullanıcı"));
        let _ = wr.write_all(format!("{}\n", serde_json::to_string(&r).unwrap_or_default()).as_bytes()).await;
        return;
    }
    let mut lines = BufReader::new(rd.take_limit()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let resp = match decode_request(line.as_bytes()) {
            Err((id, e)) => Response::err(id, e),
            Ok((id, call)) => {
                let a = agent.clone();
                match tokio::task::spawn_blocking(move || a.handle(call)).await {
                    Ok(Ok(v)) => Response::ok(id, v),
                    Ok(Err(e)) => Response::err(Some(id), e),
                    Err(e) => Response::err(Some(id), RpcError::new(code::INTERNAL, e.to_string())),
                }
            }
        };
        if wr.write_all(format!("{}\n", serde_json::to_string(&resp).unwrap_or_default()).as_bytes()).await.is_err() {
            break;
        }
    }
}

trait TakeLimit {
    fn take_limit(self) -> tokio::io::Take<tokio::net::unix::OwnedReadHalf>;
}

impl TakeLimit for tokio::net::unix::OwnedReadHalf {
    fn take_limit(self) -> tokio::io::Take<tokio::net::unix::OwnedReadHalf> {
        use tokio::io::AsyncReadExt;
        self.take(16 * 1024 * 1024)
    }
}

/// Hyprland `.socket2.sock`: game windows open/close. EOF = Hyprland exited → clean exit.
async fn hypr_events(agent: Arc<Agent>) {
    let Some(dir) = hypr::dir() else { return };
    let Ok(stream) = UnixStream::connect(dir.join(".socket2.sock")).await else { return };
    let mut lines = BufReader::new(stream).lines();
    let mut games: HashMap<String, String> = HashMap::new();
    while let Ok(Some(line)) = lines.next_line().await {
        let Some(ev) = hypr::parse_event(&line) else { continue };
        match ev.name.as_str() {
            "openwindow" => {
                let mut parts = ev.data.splitn(4, ',');
                let (Some(addr), _, Some(class)) = (parts.next(), parts.next(), parts.next()) else { continue };
                let (addr, class) = (addr.to_owned(), class.to_owned());
                let a2 = addr.clone();
                let exe = tokio::task::spawn_blocking(move || hypr::pid_of(&a2).and_then(game::exe_of)).await.ok().flatten();
                if game::is_game(&class, exe.as_deref()) {
                    games.insert(addr, class.clone());
                    *agent.game.lock().unwrap_or_else(|p| p.into_inner()) = Some(class.clone());
                    let _ = agent.tx.send(daemon::Outgoing::Event(SessionEvent::GameStarted { executable: exe.unwrap_or_default(), class })).await;
                }
            }
            "closewindow" if games.remove(&ev.data).is_some() && games.is_empty() => {
                *agent.game.lock().unwrap_or_else(|p| p.into_inner()) = None;
                let _ = agent.tx.send(daemon::Outgoing::Event(SessionEvent::GameStopped)).await;
            }
            _ => {}
        }
    }
    eprintln!("r9t-session: Hyprland olay soketi kapandı; çıkılıyor");
    std::process::exit(0);
}

/// Low refresh rate on battery (only eDP, only modes the compositor lists).
async fn hz_policy(agent: Arc<Agent>) {
    let mut saved: HashMap<String, u32> = HashMap::new();
    let mut last_ac: Option<bool> = None;
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let enabled = agent.config.lock().unwrap_or_else(|p| p.into_inner()).low_hz_on_battery;
        let watch = power_watch_running();
        *agent.hz_policy.lock().unwrap_or_else(|p| p.into_inner()) = match (enabled, watch) {
            (false, _) => "kapalı".into(),
            (true, true) => "power-watch.sh çalışıyor; Hz'e dokunulmuyor".into(),
            (true, false) => "pilde en düşük Hz".into(),
        };
        if !enabled || watch {
            last_ac = None;
            continue;
        }
        let Some(ac) = on_ac() else { continue };
        if Some(ac) == last_ac {
            continue;
        }
        last_ac = Some(ac);
        let d = agent.desktop;
        let saved_now = saved.clone();
        let result = tokio::task::spawn_blocking(move || -> Result<HashMap<String, u32>, String> {
            let mut s = saved_now;
            for o in outputs(d)?.into_iter().filter(|o| o.name.starts_with("eDP")) {
                if !ac {
                    let low = o.available_hz.iter().copied().fold(f32::MAX, f32::min);
                    if low < o.refresh_hz - 1.0 {
                        s.insert(o.name.clone(), o.refresh_hz.round() as u32);
                        set_refresh(d, &o.name, low.round() as u32)?;
                    }
                } else if let Some(hz) = s.remove(&o.name) {
                    set_refresh(d, &o.name, hz)?;
                }
            }
            Ok(s)
        })
        .await;
        match result {
            Ok(Ok(s)) => saved = s,
            Ok(Err(e)) => eprintln!("r9t-session: Hz politikası: {e}"),
            Err(e) => eprintln!("r9t-session: {e}"),
        }
    }
}

fn main() {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", nix::unistd::getuid())));
    if nix::unistd::getuid().is_root() {
        eprintln!("r9t-session root olarak çalışmaz");
        std::process::exit(2);
    }
    let sock_dir = runtime_dir.join("r9t");
    let sock = sock_dir.join("session.sock");
    let config: Config = std::fs::read(config_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let d = desktop();
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("tokio");
    rt.block_on(async move {
        let _ = std::fs::create_dir_all(&sock_dir);
        let _ = std::fs::set_permissions(&sock_dir, std::fs::Permissions::from_mode(0o700));
        if UnixStream::connect(&sock).await.is_ok() {
            eprintln!("r9t-session zaten çalışıyor");
            std::process::exit(0);
        }
        let _ = std::fs::remove_file(&sock);
        let listener = UnixListener::bind(&sock).expect("session socket");
        let _ = std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o600));
        let (tx, rx) = mpsc::channel(64);
        let connected = Arc::new(AtomicBool::new(false));
        let daemon_error = Arc::new(Mutex::new(String::new()));
        let agent = Arc::new(Agent {
            desktop: d,
            tx: tx.clone(),
            connected: connected.clone(),
            daemon_error: daemon_error.clone(),
            idle_timeout: Arc::new(AtomicU32::new(config.idle_timeout_s)),
            idle: Arc::new(AtomicBool::new(false)),
            game: Arc::new(Mutex::new(None)),
            audio_on: Arc::new(AtomicBool::new(false)),
            hz_policy: Arc::new(Mutex::new(String::new())),
            nvml: Arc::new(r9t_hal::gpu::Nvml::new(r9t_hal::Sysfs::from_env())),
            config: Mutex::new(config),
        });
        let desktop_name = serde_json::to_value(d).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default();
        tokio::spawn(daemon::run(r9t_protocol::DAEMON_SOCKET.into(), desktop_name, rx, connected, daemon_error));
        // Idle: Wayland thread → std channel → async forwarder.
        let (itx, irx) = std::sync::mpsc::channel::<bool>();
        let timeout = agent.idle_timeout.clone();
        std::thread::spawn(move || {
            if let Err(e) = idle::run(timeout, itx) {
                eprintln!("r9t-session: boşta algılama yok: {e}");
            }
        });
        let a2 = agent.clone();
        std::thread::spawn(move || {
            let mut last = false;
            while let Ok(idle) = irx.recv() {
                a2.idle.store(idle, Ordering::Relaxed);
                if idle != last {
                    last = idle;
                    let _ = a2.tx.blocking_send(daemon::Outgoing::Event(SessionEvent::Idle { idle }));
                }
            }
        });
        tokio::spawn(hypr_events(agent.clone()));
        tokio::spawn(hz_policy(agent.clone()));
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM");
        loop {
            tokio::select! {
                acc = listener.accept() => {
                    if let Ok((s, _)) = acc {
                        tokio::spawn(serve_client(agent.clone(), s));
                    }
                }
                _ = term.recv() => break,
                _ = tokio::signal::ctrl_c() => break,
            }
        }
        agent.audio_on.store(false, Ordering::Release);
        let _ = std::fs::remove_file(&sock);
    });
}
