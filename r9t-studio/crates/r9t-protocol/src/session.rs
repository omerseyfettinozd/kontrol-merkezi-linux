//! r9t-session (user agent) contract. Same framing and JSON-RPC envelope as the
//! daemon; the socket is `$XDG_RUNTIME_DIR/r9t/session.sock` (0600, same uid only).

use serde::{Deserialize, Serialize};

use crate::rpc::Empty;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshParams {
    /// Output name, e.g. `eDP-1`.
    pub output: String,
    /// Must be one of the modes the compositor reports for the current resolution.
    pub hz: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PercentParams {
    pub percent: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RadioParams {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KillParams {
    pub pid: u32,
    /// `/proc/<pid>/stat` field 22; guards against pid reuse.
    pub start_time: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecondsParams {
    /// 0 disables.
    pub seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum SessionCall {
    Status(Empty),
    SetRefreshRate(RefreshParams),
    SetBrightness(PercentParams),
    SetVolume(PercentParams),
    SetMicVolume(PercentParams),
    Wifi(RadioParams),
    Bluetooth(RadioParams),
    Airplane(RadioParams),
    GpuProcesses(Empty),
    KillProcess(KillParams),
    /// Capture the default sink monitor and push keyboard frames to r9td.
    AudioReactive(RadioParams),
    /// Seconds without input before `Idle{idle:true}` is reported to r9td.
    SetIdleTimeout(SecondsParams),
    /// Installed Steam games (name, appid, install dir) for automation rules.
    SteamGames(Empty),
}

impl SessionCall {
    pub const METHODS: &'static [&'static str] = &[
        "status", "set_refresh_rate", "set_brightness", "set_volume", "set_mic_volume", "wifi",
        "bluetooth", "airplane", "gpu_processes", "kill_process", "audio_reactive", "set_idle_timeout", "steam_games",
    ];
}

/// Display state reported by the session agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputState {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: f32,
    /// Refresh rates available at the current resolution, read at runtime.
    pub available_hz: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Desktop {
    Hyprland,
    Kde,
    Other,
}

/// Parses one session request line (same rules as the daemon: no batches, unknown
/// fields refused).
pub fn decode_request(line: &[u8]) -> Result<(crate::rpc::Id, SessionCall), (Option<crate::rpc::Id>, crate::rpc::RpcError)> {
    use crate::rpc::{code, Id, RpcError};
    use serde_json::{Map, Value};
    let value: Value = serde_json::from_slice(line).map_err(|e| (None, RpcError::new(code::PARSE_ERROR, e.to_string())))?;
    let Value::Object(obj) = value else {
        return Err((None, RpcError::new(code::INVALID_REQUEST, "İstek bir JSON nesnesi olmalı.")));
    };
    let id = match obj.get("id") {
        Some(Value::Number(n)) if n.as_u64().is_some() => Some(Id::Num(n.as_u64().unwrap_or(0))),
        Some(Value::String(s)) if (1..=64).contains(&s.len()) => Some(Id::Str(s.clone())),
        _ => None,
    };
    if obj.keys().any(|k| !matches!(k.as_str(), "jsonrpc" | "id" | "method" | "params")) || obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err((id, RpcError::new(code::INVALID_REQUEST, "Geçersiz JSON-RPC isteği.")));
    }
    let Some(id_ok) = id.clone() else {
        return Err((None, RpcError::new(code::INVALID_REQUEST, "id gerekli.")));
    };
    let method = obj.get("method").and_then(Value::as_str).unwrap_or_default();
    if !SessionCall::METHODS.contains(&method) {
        return Err((id, RpcError::new(code::METHOD_NOT_FOUND, format!("Bilinmeyen yöntem: {method}"))));
    }
    let params = match obj.get("params") {
        None | Some(Value::Null) => Value::Object(Map::new()),
        Some(p @ Value::Object(_)) => p.clone(),
        Some(_) => return Err((id, RpcError::new(code::INVALID_PARAMS, "params bir nesne olmalı."))),
    };
    let mut tagged = Map::new();
    tagged.insert("method".into(), Value::from(method));
    tagged.insert("params".into(), params);
    serde_json::from_value::<SessionCall>(Value::Object(tagged))
        .map(|c| (id_ok, c))
        .map_err(|e| (id, RpcError::new(code::INVALID_PARAMS, e.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_requests_decode_and_reject() {
        let (_, c) = decode_request(br#"{"jsonrpc":"2.0","id":1,"method":"set_refresh_rate","params":{"output":"eDP-1","hz":240}}"#).unwrap();
        assert_eq!(c, SessionCall::SetRefreshRate(RefreshParams { output: "eDP-1".into(), hz: 240 }));
        assert!(decode_request(br#"{"jsonrpc":"2.0","id":1,"method":"status","params":{"x":1}}"#).is_err());
        assert!(decode_request(br#"{"jsonrpc":"2.0","id":1,"method":"reboot"}"#).is_err());
        assert!(decode_request(br#"[]"#).is_err());
        for m in SessionCall::METHODS {
            let line = format!(r#"{{"jsonrpc":"2.0","id":1,"method":"{m}"}}"#);
            let e = decode_request(line.as_bytes()).err().map(|e| e.1.code);
            assert_ne!(e, Some(crate::rpc::code::METHOD_NOT_FOUND), "{m}");
        }
    }
}
