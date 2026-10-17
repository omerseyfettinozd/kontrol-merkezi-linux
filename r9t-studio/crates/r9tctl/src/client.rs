//! Blocking JSON-RPC client for the r9td socket.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use r9t_protocol::rpc::{Call, Id, Incoming, Notification, Request, RpcError};
use r9t_protocol::RESPONSE_MAX;
use serde_json::Value;

pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: u64,
}

#[derive(Debug)]
pub enum ClientError {
    Io(std::io::Error),
    Rpc(RpcError),
    Protocol(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Rpc(e) => write!(f, "{e}"),
            Self::Protocol(m) => write!(f, "{m}"),
        }
    }
}

impl From<std::io::Error> for ClientError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl Client {
    pub fn connect(path: &Path, timeout: Option<Duration>) -> Result<Self, ClientError> {
        let s = UnixStream::connect(path)?;
        s.set_read_timeout(timeout)?;
        Ok(Self { reader: BufReader::new(s.try_clone()?), writer: s, next_id: 1 })
    }

    pub fn call(&mut self, call: Call) -> Result<Value, ClientError> {
        let id = Id::Num(self.next_id);
        self.next_id += 1;
        let line = serde_json::to_vec(&Request { id: id.clone(), call }).map_err(|e| ClientError::Protocol(e.to_string()))?;
        self.writer.write_all(&line)?;
        self.writer.write_all(b"\n")?;
        loop {
            match self.read()? {
                Incoming::Response(r) if r.id.as_ref() == Some(&id) || r.id.is_none() => {
                    return r.into_result().map_err(ClientError::Rpc);
                }
                Incoming::Response(_) | Incoming::Notification(_) => continue,
            }
        }
    }

    pub fn read(&mut self) -> Result<Incoming, ClientError> {
        let mut line = String::new();
        let n = (&mut self.reader).take(RESPONSE_MAX as u64 + 1).read_line(&mut line)?;
        if n == 0 {
            return Err(ClientError::Protocol("bağlantı kapandı".into()));
        }
        serde_json::from_str(&line).map_err(|e| ClientError::Protocol(format!("yanıt çözümlenemedi: {e}")))
    }

    pub fn notification(&mut self) -> Result<Notification, ClientError> {
        loop {
            if let Incoming::Notification(n) = self.read()? {
                return Ok(n);
            }
        }
    }
}

use std::io::Read;
