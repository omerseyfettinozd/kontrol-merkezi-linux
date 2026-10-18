//! Connection to r9td: `hello`, `session_register` (polkit-checked), then session
//! events and audio frames. Reconnects every 2 s; the registration lives exactly as
//! long as this connection (no persistent `session_ready` flag).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

use r9t_protocol::rpc::SessionEvent;

pub enum Outgoing {
    Event(SessionEvent),
    Frame(Vec<[u8; 3]>),
}

pub async fn run(socket: String, desktop: String, mut rx: mpsc::Receiver<Outgoing>, connected: Arc<AtomicBool>, last_error: Arc<std::sync::Mutex<String>>) {
    let session_id = std::env::var("XDG_SESSION_ID").unwrap_or_default();
    let mut next_id: u64 = 1;
    loop {
        match UnixStream::connect(&socket).await {
            Ok(stream) => {
                let (rd, mut wr) = stream.into_split();
                let mut lines = BufReader::new(rd).lines();
                let hello = line(&mut next_id, "hello", json!({"protocol_version": r9t_protocol::PROTOCOL_VERSION, "client": "r9t-session"}));
                let reg = line(&mut next_id, "session_register", json!({"logind_session_id": session_id, "desktop": desktop, "pid": std::process::id()}));
                let reg_id = next_id;
                if wr.write_all(hello.as_bytes()).await.is_err() || wr.write_all(reg.as_bytes()).await.is_err() {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                // The registration reply decides whether we are connected.
                let mut registered = false;
                let mut read_only = false;
                for _ in 0..2 {
                    match tokio::time::timeout(Duration::from_secs(30), lines.next_line()).await {
                        Ok(Ok(Some(l))) => {
                            let v: serde_json::Value = serde_json::from_str(&l).unwrap_or_default();
                            if v["id"] == json!(reg_id) {
                                registered = v.get("result").is_some();
                                read_only = v["error"]["code"] == json!(r9t_protocol::rpc::code::READ_ONLY);
                                if !registered {
                                    *last_error.lock().unwrap_or_else(|p| p.into_inner()) = v["error"]["message"].as_str().unwrap_or("kayıt reddedildi").to_owned();
                                }
                            }
                        }
                        _ => break,
                    }
                }
                if read_only {
                    // Shadow r9td refuses registration until it restarts in control
                    // mode, which closes this socket. Retrying every few seconds only
                    // floods the audit log; wait for the close (or 5 min).
                    let _ = tokio::time::timeout(Duration::from_secs(300), async {
                        while let Ok(Some(_)) = lines.next_line().await {}
                    })
                    .await;
                    continue;
                }
                if !registered {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
                connected.store(true, Ordering::Relaxed);
                last_error.lock().unwrap_or_else(|p| p.into_inner()).clear();
                loop {
                    tokio::select! {
                        out = rx.recv() => {
                            let Some(out) = out else { return };
                            let line = match out {
                                Outgoing::Event(e) => line(&mut next_id, "session_event", json!({"event": e})),
                                Outgoing::Frame(f) => line(&mut next_id, "rgb_frame", json!({"colors": f})),
                            };
                            if wr.write_all(line.as_bytes()).await.is_err() { break; }
                        }
                        l = lines.next_line() => {
                            match l {
                                Ok(Some(_)) => {}
                                _ => break,
                            }
                        }
                    }
                }
                connected.store(false, Ordering::Relaxed);
            }
            Err(e) => {
                *last_error.lock().unwrap_or_else(|p| p.into_inner()) = format!("r9td: {e}");
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

fn line(next_id: &mut u64, method: &str, params: serde_json::Value) -> String {
    *next_id += 1;
    format!("{}\n", json!({"jsonrpc": "2.0", "id": *next_id, "method": method, "params": params}))
}
