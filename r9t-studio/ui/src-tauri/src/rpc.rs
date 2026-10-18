//! Blocking NDJSON JSON-RPC 2.0 client for the r9td and r9t-session sockets.
//! Requests and results stay `serde_json::Value`: the services validate params.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

/// Same limit as `r9t_protocol::RESPONSE_MAX`.
const RESPONSE_MAX: u64 = 1 << 20;

pub struct Conn {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: u64,
}

#[derive(Debug)]
pub enum CallError {
    /// Nothing was sent; safe to reconnect and retry.
    Connect(String),
    /// The request may have reached the service; never retried.
    Io(String),
    /// JSON-RPC error object from the service.
    Rpc(Value),
}

impl CallError {
    pub fn to_value(&self) -> Value {
        match self {
            Self::Connect(m) => json!({"code": -1, "message": m, "kind": "connect"}),
            Self::Io(m) => json!({"code": -2, "message": m, "kind": "io"}),
            Self::Rpc(e) => {
                let mut e = e.clone();
                if let Some(o) = e.as_object_mut() {
                    o.insert("kind".into(), "rpc".into());
                }
                e
            }
        }
    }
}

pub fn daemon_socket() -> PathBuf {
    std::env::var_os("R9TD_SOCKET").map(PathBuf::from).unwrap_or_else(|| "/run/r9td/r9td.sock".into())
}

pub fn session_socket() -> PathBuf {
    let run = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|| "/run/user/1000".into());
    run.join("r9t/session.sock")
}

impl Conn {
    pub fn connect(path: &Path, timeout: Duration) -> Result<Self, CallError> {
        let s = UnixStream::connect(path).map_err(|e| CallError::Connect(format!("{}: {e}", path.display())))?;
        s.set_read_timeout(Some(timeout)).map_err(|e| CallError::Connect(e.to_string()))?;
        s.set_write_timeout(Some(Duration::from_secs(5))).map_err(|e| CallError::Connect(e.to_string()))?;
        let reader = BufReader::new(s.try_clone().map_err(|e| CallError::Connect(e.to_string()))?);
        Ok(Self { reader, writer: s, next_id: 1 })
    }

    pub fn send(&mut self, method: &str, params: &Value) -> Result<u64, CallError> {
        let id = self.next_id;
        self.next_id += 1;
        let mut line = serde_json::to_vec(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .map_err(|e| CallError::Io(e.to_string()))?;
        line.push(b'\n');
        // A failed write means the peer is gone before reading anything.
        self.writer.write_all(&line).map_err(|e| CallError::Connect(e.to_string()))?;
        Ok(id)
    }

    pub fn call(&mut self, method: &str, params: &Value) -> Result<Value, CallError> {
        let id = self.send(method, params)?;
        loop {
            let msg = self.read()?;
            if msg.get("method").is_some() {
                continue; // notification
            }
            let rid = msg.get("id");
            if rid.and_then(Value::as_u64) != Some(id) && !rid.is_some_and(Value::is_null) {
                continue;
            }
            if let Some(e) = msg.get("error") {
                return Err(CallError::Rpc(e.clone()));
            }
            return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    pub fn read(&mut self) -> Result<Value, CallError> {
        let mut line = String::new();
        let n = (&mut self.reader).take(RESPONSE_MAX + 1).read_line(&mut line).map_err(|e| CallError::Io(e.to_string()))?;
        if n == 0 {
            return Err(CallError::Io("bağlantı kapandı".into()));
        }
        serde_json::from_str(&line).map_err(|e| CallError::Io(format!("yanıt çözümlenemedi: {e}")))
    }
}

/// One persistent connection, reconnected on demand. Keeping it open lets r9td's
/// per-connection polkit cache (60 s) cover consecutive control calls.
pub struct Persistent {
    path: PathBuf,
    timeout: Duration,
    conn: Option<Conn>,
}

impl Persistent {
    pub fn new(path: PathBuf, timeout: Duration) -> Self {
        Self { path, timeout, conn: None }
    }

    pub fn call(&mut self, method: &str, params: &Value) -> Result<Value, CallError> {
        for attempt in 0..2 {
            if self.conn.is_none() {
                self.conn = Some(Conn::connect(&self.path, self.timeout)?);
            }
            let conn = self.conn.as_mut().expect("connected above");
            match conn.call(method, params) {
                Err(CallError::Connect(e)) => {
                    self.conn = None;
                    if attempt == 1 {
                        return Err(CallError::Connect(e));
                    }
                }
                Err(CallError::Io(e)) => {
                    self.conn = None;
                    return Err(CallError::Io(e));
                }
                other => return other,
            }
        }
        unreachable!("loop returns on the second attempt")
    }
}
