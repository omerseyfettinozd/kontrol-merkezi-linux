//! Unix socket JSON-RPC server. Reads are answered from the snapshot cache; in shadow
//! mode every control method is refused with `READ_ONLY`.

use std::os::fd::{AsFd, FromRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc, Semaphore};

use r9t_protocol::frame::{encode_line, FrameError, LineDecoder};
use r9t_protocol::rpc::{code, decode_request, Call, Notification, Response, RpcError, Topic};
use r9t_protocol::{PROTOCOL_VERSION, REQUEST_MAX, RESPONSE_MAX, SUBSCRIBER_BACKLOG};

use crate::audit::{Audit, Event};
use crate::sampler::{unix_ms, Shared};

pub const MAX_CONNECTIONS: usize = 32;

pub struct Server {
    pub shared: Arc<Shared>,
    pub audit: Arc<Mutex<Audit>>,
    pub capabilities: Value,
    pub shadow: bool,
    pub r9t_gid: Option<u32>,
    pub allow_uid: Option<u32>,
    pub snapshots: broadcast::Sender<Arc<Vec<u8>>>,
    /// Present in control mode; `None` in shadow mode or when control was refused.
    pub control: Option<crate::control::ControlHandle>,
    /// Why control was refused at start (shown to clients instead of READ_ONLY).
    pub refused: Option<String>,
    pub polkit: Option<zbus::blocking::Connection>,
    pub conn_seq: std::sync::atomic::AtomicU64,
    pub state_dir: std::path::PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub struct Peer {
    pub uid: u32,
    pub gid: u32,
    pub pid: i32,
    /// `/proc/<pid>/stat` start time at connect (pid-reuse guard for polkit).
    pub start: Option<u64>,
}

/// Polkit answers are cached per connection for this long.
pub const AUTH_CACHE: std::time::Duration = std::time::Duration::from_secs(60);

/// Binds the socket (or adopts the systemd-passed one) with 0660 root:r9t.
pub fn listener(path: &Path, r9t_gid: Option<u32>) -> std::io::Result<UnixListener> {
    if std::env::var("LISTEN_PID").ok().and_then(|p| p.parse::<u32>().ok()) == Some(std::process::id())
        && std::env::var("LISTEN_FDS").ok().as_deref() == Some("1")
    {
        // SAFETY: systemd passes the listening socket as fd 3 when LISTEN_PID matches.
        let fd = unsafe { OwnedFd::from_raw_fd(3) };
        let std_listener = std::os::unix::net::UnixListener::from(fd);
        std_listener.set_nonblocking(true)?;
        return UnixListener::from_std(std_listener);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = std::fs::remove_file(path);
    let l = UnixListener::bind(path)?;
    match r9t_gid {
        Some(gid) => {
            nix::unistd::chown(path, Some(nix::unistd::Uid::from_raw(0)), Some(nix::unistd::Gid::from_raw(gid)))
                .map_err(std::io::Error::other)
                .or_else(|e| if nix::unistd::geteuid().is_root() { Err(e) } else { Ok(()) })?;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660))?;
        }
        None => std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?,
    }
    Ok(l)
}

fn peer_of(stream: &UnixStream) -> std::io::Result<Peer> {
    let c = nix::sys::socket::getsockopt(&stream.as_fd(), nix::sys::socket::sockopt::PeerCredentials)
        .map_err(std::io::Error::other)?;
    Ok(Peer { uid: c.uid(), gid: c.gid(), pid: c.pid(), start: crate::auth::start_time(c.pid()) })
}

/// Supplementary + primary groups of the peer process (snapshot at connect time).
fn peer_groups(peer: &Peer) -> Vec<u32> {
    let mut groups = vec![peer.gid];
    if let Ok(status) = std::fs::read_to_string(format!("/proc/{}/status", peer.pid)) {
        if let Some(line) = status.lines().find(|l| l.starts_with("Groups:")) {
            groups.extend(line[7..].split_whitespace().filter_map(|g| g.parse::<u32>().ok()));
        }
    }
    groups
}

impl Server {
    fn may_read(&self, peer: &Peer) -> bool {
        peer.uid == 0
            || self.allow_uid == Some(peer.uid)
            || self.r9t_gid.is_some_and(|gid| peer_groups(peer).contains(&gid))
    }

    pub async fn serve(self: Arc<Self>, listener: UnixListener) {
        let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
        loop {
            let Ok((stream, _)) = listener.accept().await else { continue };
            let Ok(permit) = slots.clone().try_acquire_owned() else {
                drop(stream);
                continue;
            };
            let me = self.clone();
            tokio::spawn(async move {
                let _permit = permit;
                me.connection(stream).await;
            });
        }
    }

    async fn connection(self: Arc<Self>, stream: UnixStream) {
        let Ok(peer) = peer_of(&stream) else { return };
        let conn_id = self.conn_seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut authorized_at: Option<Instant> = None;
        let (mut rd, mut wr) = stream.into_split();
        let (tx, mut rx) = mpsc::channel::<Arc<Vec<u8>>>(SUBSCRIBER_BACKLOG);
        let writer = tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                if wr.write_all(&line).await.is_err() {
                    break;
                }
            }
        });
        if !self.may_read(&peer) {
            let r = Response::err(None, RpcError::new(code::UNAUTHORIZED, "r9t grubu üyeliği gerekli."));
            if let Ok(line) = encode_line(&r, RESPONSE_MAX) {
                let _ = tx.send(Arc::new(line)).await;
            }
            self.audit(&peer, "connect", false, 0, "yetkisiz bağlantı");
            drop(tx);
            let _ = writer.await;
            return;
        }
        let mut decoder = LineDecoder::new(REQUEST_MAX);
        let mut buf = vec![0u8; 8192];
        let mut sub: Option<tokio::task::JoinHandle<()>> = None;
        // Fired by the forwarder when this client falls 256 messages behind.
        let (kill_tx, mut kill_rx) = tokio::sync::watch::channel(false);
        'conn: loop {
            let n = tokio::select! {
                r = rd.read(&mut buf) => match r {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                },
                _ = kill_rx.changed() => break,
            };
            for item in decoder.push(&buf[..n]) {
                let line = match item {
                    Ok(l) => l,
                    Err(FrameError::TooLarge { .. }) => {
                        let r = Response::err(None, RpcError::new(code::TOO_LARGE, "İstek 64 KiB sınırını aştı."));
                        if let Ok(l) = encode_line(&r, RESPONSE_MAX) {
                            let _ = tx.send(Arc::new(l)).await;
                        }
                        break 'conn;
                    }
                };
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                let started = Instant::now();
                let response = match decode_request(&line) {
                    Err((id, e)) => Response::err(id, e),
                    Ok(req) => {
                        let method = req.call.name();
                        let result = match &req.call {
                            Call::Subscribe(p) => {
                                if p.topics.contains(&Topic::Snapshot) && sub.is_none() {
                                    sub = Some(self.forward(tx.clone(), kill_tx.clone()));
                                }
                                Ok(json!({"subscribed": p.topics}))
                            }
                            Call::Unsubscribe(_) => {
                                if let Some(h) = sub.take() {
                                    h.abort();
                                }
                                Ok(json!({"subscribed": []}))
                            }
                            other if !other.is_read_only() => {
                                self.clone().control_call(other.clone(), peer, conn_id, &mut authorized_at).await
                            }
                            other => self.dispatch(other),
                        };
                        let ok = result.is_ok();
                        // The control thread audits the calls it executes itself.
                        if !req.call.is_read_only() && self.control.is_none() {
                            let msg = result.as_ref().err().map(|e| e.message.clone()).unwrap_or_default();
                            self.audit(&peer, method, ok, started.elapsed().as_millis() as u64, &msg);
                        }
                        match result {
                            Ok(v) => Response::ok(req.id, v),
                            Err(e) => Response::err(Some(req.id), e),
                        }
                    }
                };
                let line = encode_line(&response, RESPONSE_MAX).unwrap_or_else(|e| {
                    encode_line(&Response::err(response.id.clone(), RpcError::new(code::INTERNAL, e.to_string())), RESPONSE_MAX)
                        .unwrap_or_default()
                });
                if tx.send(Arc::new(line)).await.is_err() {
                    break 'conn;
                }
            }
        }
        if let Some(h) = sub {
            h.abort();
        }
        if let Some(c) = &self.control {
            c.conn_closed(conn_id);
        }
        drop(tx);
        let _ = writer.await;
    }

    /// Control methods: polkit (`org.r9t.control`) then the single-writer thread.
    async fn control_call(self: Arc<Self>, call: Call, peer: Peer, conn_id: u64, authorized_at: &mut Option<Instant>) -> Result<Value, RpcError> {
        let Some(handle) = self.control.clone() else {
            return Err(RpcError::new(
                code::READ_ONLY,
                self.refused.clone().unwrap_or_else(|| "r9td gölge (salt okunur) modda; kontrol eski servistedir.".into()),
            ));
        };
        // High-rate session traffic is bound to a polkit-checked session_register.
        let needs_polkit = !matches!(call, Call::RgbFrame(_) | Call::SessionEvent(_));
        let cached = authorized_at.is_some_and(|t| t.elapsed() < AUTH_CACHE);
        if needs_polkit && !cached {
            let Some(start) = peer.start else {
                return Err(RpcError::new(code::UNAUTHORIZED, "istemci süreci okunamadı"));
            };
            let polkit = self.polkit.clone();
            let decision = tokio::task::spawn_blocking(move || match polkit {
                Some(conn) => crate::auth::check(&conn, peer.pid, peer.uid, start, crate::auth::ACTION_CONTROL),
                None if peer.uid == 0 => crate::auth::Decision::Allowed,
                None => crate::auth::Decision::Denied("polkit'e ulaşılamıyor".into()),
            })
            .await
            .map_err(|e| RpcError::new(code::INTERNAL, e.to_string()))?;
            match decision {
                crate::auth::Decision::Allowed => *authorized_at = Some(Instant::now()),
                crate::auth::Decision::Denied(m) => return Err(RpcError::new(code::UNAUTHORIZED, m)),
            }
        }
        tokio::task::spawn_blocking(move || handle.call(call, peer, conn_id))
            .await
            .map_err(|e| RpcError::new(code::INTERNAL, e.to_string()))?
    }

    /// Forwards snapshot notifications; a subscriber more than 256 behind is dropped.
    fn forward(&self, tx: mpsc::Sender<Arc<Vec<u8>>>, kill: tokio::sync::watch::Sender<bool>) -> tokio::task::JoinHandle<()> {
        let mut rx = self.snapshots.subscribe();
        tokio::spawn(async move {
            // Lagged (broadcast ring overrun) or a full per-client queue: disconnect.
            while let Ok(line) = rx.recv().await {
                if tx.try_send(line).is_err() {
                    break;
                }
            }
            let _ = kill.send(true);
        })
    }

    fn dispatch(&self, call: &Call) -> Result<Value, RpcError> {
        let now = Instant::now();
        match call {
            Call::Hello(h) => {
                if h.protocol_version != PROTOCOL_VERSION {
                    return Err(RpcError::new(code::VERSION_MISMATCH, format!("protokol {PROTOCOL_VERSION} bekleniyor")));
                }
                Ok(json!({"server": "r9td", "version": env!("CARGO_PKG_VERSION"), "protocol_version": PROTOCOL_VERSION,
                          "mode": if self.shadow { "shadow" } else { "control" }}))
            }
            Call::Status(_) => {
                let live = self.shared.live.read().unwrap_or_else(|p| p.into_inner());
                serde_json::to_value(live.snapshot(now, unix_ms())).map_err(|e| RpcError::new(code::INTERNAL, e.to_string()))
            }
            Call::Policy(_) => {
                let doc = std::fs::read(self.state_dir.join("policy.json"))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<r9t_protocol::policy::PolicyDocument>(&b).ok())
                    .unwrap_or_default();
                // Last applied settings (fan curve, RGB, limits) so clients can show the active state.
                let last = std::fs::read(self.state_dir.join(crate::control::LAST))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<r9t_protocol::profile::ProfileSettings>(&b).ok());
                Ok(json!({"document": doc, "paused": self.state_dir.join("policy.paused").exists(), "last": last}))
            }
            Call::RgbState(_) => {
                let r = r9t_hal::rgb::Rgb::discover(r9t_hal::Sysfs::from_env()).map_err(|e| RpcError::new(code::UNSUPPORTED, e.to_string()))?;
                let colors = r.read_frame().map_err(|e| RpcError::new(code::HARDWARE, e.to_string()))?;
                Ok(json!({"colors": colors, "brightness": r.brightness().ok(), "max": r.max_brightness()}))
            }
            Call::History(p) => {
                let h = self.shared.history.lock().unwrap_or_else(|p| p.into_inner());
                Ok(json!({"window_s": p.seconds.min(300), "points": h.since(unix_ms(), p.seconds)}))
            }
            Call::Capabilities(_) => {
                let m = &self.shared.metrics;
                use std::sync::atomic::Ordering::Relaxed;
                let presets: Vec<Value> = r9t_protocol::presets::THERMAL_PRESETS
                    .iter()
                    .map(|p| json!({"key": p.key, "name": p.name, "description": p.description, "settings": r9t_protocol::presets::thermal_preset(p.key)}))
                    .collect();
                Ok(json!({"protocol_version": PROTOCOL_VERSION, "mode": if self.shadow { "shadow" } else { "control" },
                          "probe": self.capabilities,
                          "thermal_presets": presets,
                          "fan_presets": r9t_protocol::presets::FAN_PRESET_KEYS,
                          "scenarios": ["quiet", "balanced", "game", "turbo"],
                          "metrics": {"ec_ops": m.ec_ops.load(Relaxed), "fan_status_reads": m.fan_status_reads.load(Relaxed),
                                      "legacy_queries": m.legacy_queries.load(Relaxed), "samples": m.samples.load(Relaxed),
                                      "nvml_calls": m.nvml_calls.load(Relaxed), "nvml_skipped_inactive": m.nvml_skipped_inactive.load(Relaxed)}}))
            }
            Call::Events(p) => {
                let a = self.audit.lock().unwrap_or_else(|p| p.into_inner());
                Ok(json!({"events": a.latest(p.limit as usize)}))
            }
            _ => Err(RpcError::new(code::READ_ONLY, "r9td gölge (salt okunur) modda; kontrol eski servistedir.")),
        }
    }

    fn audit(&self, peer: &Peer, method: &str, ok: bool, duration_ms: u64, message: &str) {
        self.audit.lock().unwrap_or_else(|p| p.into_inner()).record(Event {
            time_ms: unix_ms(),
            method: method.into(),
            ok,
            uid: Some(peer.uid),
            pid: Some(peer.pid),
            duration_ms,
            message: message.into(),
        });
    }
}

/// Encodes the current snapshot as a `snapshot` notification line.
pub fn snapshot_line(shared: &Shared) -> Option<Arc<Vec<u8>>> {
    let live = shared.live.read().unwrap_or_else(|p| p.into_inner());
    let snap = serde_json::to_value(live.snapshot(Instant::now(), unix_ms())).ok()?;
    encode_line(&Notification::new("snapshot", snap), RESPONSE_MAX).ok().map(Arc::new)
}

/// `sd_notify(3)` without libsystemd.
pub fn sd_notify(msg: &str) {
    let Some(addr) = std::env::var_os("NOTIFY_SOCKET") else { return };
    let Ok(sock) = std::os::unix::net::UnixDatagram::unbound() else { return };
    let addr = PathBuf::from(addr);
    let s = addr.to_string_lossy();
    if let Some(name) = s.strip_prefix('@') {
        use std::os::linux::net::SocketAddrExt;
        if let Ok(a) = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes()) {
            let _ = sock.send_to_addr(msg.as_bytes(), &a);
        }
    } else {
        let _ = sock.send_to(msg.as_bytes(), &addr);
    }
}
