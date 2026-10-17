//! Minimal client for the legacy service socket (`/run/slayer-r9t/control.sock`).
//! Used in shadow mode for fan RPM while the legacy service owns the fans, and in
//! control mode to refuse control while the legacy service still answers.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

pub const SOCKET: &str = "/run/slayer-r9t/control.sock";

pub fn status(socket: &Path, timeout: Duration) -> std::io::Result<Value> {
    let mut s = UnixStream::connect(socket)?;
    s.set_read_timeout(Some(timeout))?;
    s.set_write_timeout(Some(timeout))?;
    let req = json!({"version": 1, "id": format!("r9td-{}", std::process::id()), "op": "status"});
    s.write_all(format!("{req}\n").as_bytes())?;
    let mut line = String::new();
    BufReader::new(s.take_limit()).read_line(&mut line)?;
    let v: Value = serde_json::from_str(&line).map_err(std::io::Error::other)?;
    if v["ok"] != Value::Bool(true) {
        return Err(std::io::Error::other(v["error"].as_str().unwrap_or("legacy error").to_owned()));
    }
    Ok(v["data"].clone())
}

/// True when something answers on the legacy socket.
pub fn answers(socket: &Path) -> bool {
    status(socket, Duration::from_secs(3)).is_ok()
}

trait TakeLimit {
    fn take_limit(self) -> std::io::Take<UnixStream>;
}

impl TakeLimit for UnixStream {
    fn take_limit(self) -> std::io::Take<UnixStream> {
        std::io::Read::take(self, 131_072)
    }
}
