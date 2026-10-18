//! JSON-RPC 2.0 messages for the root daemon. One request per line, no batches,
//! unknown fields rejected everywhere.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::policy::PolicyDocument;
use crate::profile::{Interpolation, PowerProfile, ProfileSettings};

/// Request id: unsigned integer or a short string. `null` ids (notifications) are refused.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    Num(u64),
    Str(String),
}

/// Standard and application error codes.
pub mod code {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL: i32 = -32603;
    /// Hardware operation failed or could not be verified.
    pub const HARDWARE: i32 = -32000;
    /// Caller is not in group `r9t` or polkit denied the action.
    pub const UNAUTHORIZED: i32 = -32001;
    /// Capability not present on this machine (e.g. charge limit).
    pub const UNSUPPORTED: i32 = -32002;
    /// Another operation holds the hardware, or sleep is in progress.
    pub const BUSY: i32 = -32003;
    pub const VERSION_MISMATCH: i32 = -32004;
    pub const TOO_LARGE: i32 = -32005;
    /// r9td is in shadow (read-only) mode or refused control.
    pub const READ_ONLY: i32 = -32006;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), data: None }
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for RpcError {}

/// Parameterless methods still take an (empty) object so unknown fields are refused.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

macro_rules! params {
    ($name:ident { $($(#[$m:meta])* $field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name { $($(#[$m])* pub $field: $ty),* }
    };
}

params!(Hello { protocol_version: u32, client: String });
params!(HistoryParams { seconds: u32 });
params!(EventsParams { limit: u32 });
params!(SubscribeParams { topics: Vec<Topic> });
params!(PresetParams { preset: String });
params!(ManualParams { cpu: u8, gpu: u8 });
params!(CurveParams { points: Vec<[u8; 2]>, #[serde(default)] interpolation: Interpolation });
params!(PowerParams { profile: PowerProfile });
params!(EnabledParams { enabled: bool });
params!(ProfileParams { settings: ProfileSettings });
params!(ApplyProfileParams { name: String });
params!(RgbParams { rgb: [u8; 3], brightness: u8 });
params!(BrightnessParams { brightness: u8 });
params!(RgbMapParams { colors: Vec<[u8; 3]>, brightness: u8 });
params!(RgbEffectParams { effect: RgbEffect });
params!(RgbFrameParams { colors: Vec<[u8; 3]> });
params!(IdleParams { idle: bool });
params!(ChargeLimitParams { end: u8, #[serde(default)] start: Option<u8> });
params!(TargetParams { cpu: u8, gpu: u8 });
params!(ConfigureParams { document: PolicyDocument });
params!(SessionRegisterParams { logind_session_id: String, desktop: String, pid: u32 });
params!(SessionEventParams { event: SessionEvent });
params!(ScenarioParams { name: String });

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    Snapshot,
    Events,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RgbEffect {
    Static,
    Breathing { rgb: [u8; 3], period_ms: u32 },
    Wave { speed: u8 },
    ThermalGradient,
    /// Frames are pushed by r9t-session with `rgb_frame`.
    AudioReactive,
}

/// Desktop events forwarded by the registered r9t-session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionEvent {
    GameStarted { executable: String, class: String },
    GameStopped,
    Idle { idle: bool },
    LidClosed { closed: bool },
}

/// Every daemon method. Wire form: `{"method": "<snake_case>", "params": {...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Call {
    Hello(Hello),
    Status(Empty),
    /// Saved profiles + automation rules and whether automation is paused.
    Policy(Empty),
    /// Current keyboard colours read back from sysfs (126 LEDs) and brightness.
    RgbState(Empty),
    History(HistoryParams),
    Capabilities(Empty),
    Events(EventsParams),
    Subscribe(SubscribeParams),
    Unsubscribe(Empty),
    Auto(Empty),
    FanBoost(Empty),
    FanPreset(PresetParams),
    Manual(ManualParams),
    Curve(CurveParams),
    Power(PowerParams),
    Boost(EnabledParams),
    ThermalPreset(PresetParams),
    Profile(ProfileParams),
    ApplyProfile(ApplyProfileParams),
    Rgb(RgbParams),
    RgbMap(RgbMapParams),
    /// Global keyboard brightness only (%); colours and effect stay.
    RgbBrightness(BrightnessParams),
    RgbEffect(RgbEffectParams),
    RgbFrame(RgbFrameParams),
    LightingIdle(IdleParams),
    FnLock(EnabledParams),
    Camera(EnabledParams),
    ChargeLimit(ChargeLimitParams),
    TemperatureTarget(TargetParams),
    TemperatureTargetStop(Empty),
    Configure(ConfigureParams),
    AutomationPause(Empty),
    AutomationResume(Empty),
    SessionRegister(SessionRegisterParams),
    SessionEvent(SessionEventParams),
    /// One atomic server-side command (fixes the legacy dropped-command bug).
    Scenario(ScenarioParams),
}

impl Call {
    pub const METHODS: &'static [&'static str] = &[
        "hello", "status", "policy", "rgb_state", "history", "capabilities", "events", "subscribe", "unsubscribe", "auto",
        "fan_boost", "fan_preset", "manual", "curve", "power", "boost", "thermal_preset", "profile",
        "apply_profile", "rgb", "rgb_map", "rgb_brightness", "rgb_effect", "rgb_frame", "lighting_idle", "fn_lock",
        "camera", "charge_limit", "temperature_target", "temperature_target_stop", "configure",
        "automation_pause", "automation_resume", "session_register", "session_event", "scenario",
    ];

    pub fn name(&self) -> &'static str {
        match serde_json::to_value(self) {
            Ok(Value::Object(m)) => m
                .get("method")
                .and_then(Value::as_str)
                .and_then(|s| Self::METHODS.iter().copied().find(|k| *k == s))
                .unwrap_or("?"),
            _ => "?",
        }
    }

    /// Reads that only need group membership (no polkit).
    pub fn is_read_only(&self) -> bool {
        matches!(
            self,
            Self::Hello(_)
                | Self::Status(_)
                | Self::Policy(_)
                | Self::RgbState(_)
                | Self::History(_)
                | Self::Capabilities(_)
                | Self::Events(_)
                | Self::Subscribe(_)
                | Self::Unsubscribe(_)
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub id: Id,
    pub call: Call,
}

impl Serialize for Request {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = match serde_json::to_value(&self.call).map_err(serde::ser::Error::custom)? {
            Value::Object(m) => m,
            _ => return Err(serde::ser::Error::custom("call is not an object")),
        };
        m.insert("jsonrpc".into(), Value::from("2.0"));
        m.insert("id".into(), serde_json::to_value(&self.id).map_err(serde::ser::Error::custom)?);
        m.serialize(s)
    }
}

/// Parses one request line. The returned error carries the id when it was readable,
/// so the server can still answer it.
pub fn decode_request(line: &[u8]) -> Result<Request, (Option<Id>, RpcError)> {
    let value: Value = serde_json::from_slice(line)
        .map_err(|e| (None, RpcError::new(code::PARSE_ERROR, format!("JSON çözümlenemedi: {e}"))))?;
    let obj = match value {
        Value::Object(o) => o,
        Value::Array(_) => return Err((None, RpcError::new(code::INVALID_REQUEST, "Batch istekleri desteklenmez."))),
        _ => return Err((None, RpcError::new(code::INVALID_REQUEST, "İstek bir JSON nesnesi olmalı."))),
    };
    let id = match obj.get("id") {
        Some(Value::Number(n)) if n.as_u64().is_some() => Some(Id::Num(n.as_u64().unwrap())),
        Some(Value::String(s)) if (1..=64).contains(&s.len()) => Some(Id::Str(s.clone())),
        _ => None,
    };
    let fail = |c, m: &str| Err((id.clone(), RpcError::new(c, m)));
    if let Some(k) = obj.keys().find(|k| !matches!(k.as_str(), "jsonrpc" | "id" | "method" | "params")) {
        return fail(code::INVALID_REQUEST, &format!("Bilinmeyen alan: {k}"));
    }
    if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return fail(code::INVALID_REQUEST, "jsonrpc \"2.0\" olmalı.");
    }
    let Some(id_ok) = id.clone() else {
        return fail(code::INVALID_REQUEST, "id gerekli (sayı ya da 1–64 karakter).");
    };
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        return fail(code::INVALID_REQUEST, "method gerekli.");
    };
    if !Call::METHODS.contains(&method) {
        return fail(code::METHOD_NOT_FOUND, &format!("Bilinmeyen yöntem: {method}"));
    }
    let params = match obj.get("params") {
        None | Some(Value::Null) => Value::Object(Map::new()),
        Some(p @ Value::Object(_)) => p.clone(),
        Some(_) => return fail(code::INVALID_PARAMS, "params bir nesne olmalı."),
    };
    let mut tagged = Map::new();
    tagged.insert("method".into(), Value::from(method));
    tagged.insert("params".into(), params);
    match serde_json::from_value::<Call>(Value::Object(tagged)) {
        Ok(call) => Ok(Request { id: id_ok, call }),
        Err(e) => fail(code::INVALID_PARAMS, &format!("Geçersiz parametre: {e}")),
    }
}

/// A response or a server notification, as seen by clients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Incoming {
    Response(Response),
    Notification(Notification),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    pub fn ok(id: Id, result: Value) -> Self {
        Self { jsonrpc: "2.0".into(), id: Some(id), result: Some(result), error: None }
    }

    pub fn err(id: Option<Id>, error: RpcError) -> Self {
        Self { jsonrpc: "2.0".into(), id, result: None, error: Some(error) }
    }

    pub fn into_result(self) -> Result<Value, RpcError> {
        match (self.result, self.error) {
            (_, Some(e)) => Err(e),
            (Some(v), None) => Ok(v),
            (None, None) => Err(RpcError::new(code::INTERNAL, "Boş yanıt.")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

impl Notification {
    pub fn new(method: &str, params: Value) -> Self {
        Self { jsonrpc: "2.0".into(), method: method.into(), params }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> Result<Request, (Option<Id>, RpcError)> {
        decode_request(s.as_bytes())
    }

    #[test]
    fn decodes_methods_with_and_without_params() {
        let r = dec(r#"{"jsonrpc":"2.0","id":1,"method":"status"}"#).unwrap();
        assert_eq!(r.call, Call::Status(Empty {}));
        let r = dec(r#"{"jsonrpc":"2.0","id":"a","method":"manual","params":{"cpu":60,"gpu":70}}"#).unwrap();
        assert_eq!(r.id, Id::Str("a".into()));
        assert_eq!(r.call, Call::Manual(ManualParams { cpu: 60, gpu: 70 }));
        assert_eq!(r.call.name(), "manual");
    }

    #[test]
    fn round_trip_every_serialized_request() {
        let req = Request { id: Id::Num(7), call: Call::Curve(CurveParams { points: vec![[40, 50], [85, 100]], interpolation: Interpolation::MonotoneCubic }) };
        let line = serde_json::to_vec(&req).unwrap();
        assert_eq!(decode_request(&line).unwrap(), req);
    }

    #[test]
    fn rejects_batches_unknown_fields_and_bad_ids() {
        assert_eq!(dec(r#"[{"jsonrpc":"2.0","id":1,"method":"status"}]"#).unwrap_err().1.code, code::INVALID_REQUEST);
        let e = dec(r#"{"jsonrpc":"2.0","id":1,"method":"status","extra":1}"#).unwrap_err();
        assert_eq!((e.0, e.1.code), (Some(Id::Num(1)), code::INVALID_REQUEST));
        let e = dec(r#"{"jsonrpc":"2.0","id":2,"method":"manual","params":{"cpu":60,"gpu":70,"x":0}}"#).unwrap_err();
        assert_eq!((e.0, e.1.code), (Some(Id::Num(2)), code::INVALID_PARAMS));
        assert_eq!(dec(r#"{"jsonrpc":"2.0","id":3,"method":"status","params":{"a":1}}"#).unwrap_err().1.code, code::INVALID_PARAMS);
        assert_eq!(dec(r#"{"jsonrpc":"2.0","method":"status"}"#).unwrap_err().1.code, code::INVALID_REQUEST);
        assert_eq!(dec(r#"{"jsonrpc":"2.0","id":null,"method":"status"}"#).unwrap_err().1.code, code::INVALID_REQUEST);
        assert_eq!(dec(r#"{"jsonrpc":"2.0","id":-1,"method":"status"}"#).unwrap_err().1.code, code::INVALID_REQUEST);
        assert_eq!(dec(&format!(r#"{{"jsonrpc":"2.0","id":"{}","method":"status"}}"#, "x".repeat(65))).unwrap_err().1.code, code::INVALID_REQUEST);
        assert_eq!(dec(r#"{"jsonrpc":"1.0","id":1,"method":"status"}"#).unwrap_err().1.code, code::INVALID_REQUEST);
        assert_eq!(dec(r#"{"jsonrpc":"2.0","id":1,"method":"rm_rf"}"#).unwrap_err().1.code, code::METHOD_NOT_FOUND);
        assert_eq!(dec(r#"{"jsonrpc":"2.0","id":1,"method":"manual","params":[60,70]}"#).unwrap_err().1.code, code::INVALID_PARAMS);
        assert_eq!(dec("not json").unwrap_err().1.code, code::PARSE_ERROR);
        assert_eq!(dec("42").unwrap_err().1.code, code::INVALID_REQUEST);
    }

    #[test]
    fn methods_list_matches_enum() {
        for m in Call::METHODS {
            let line = format!(r#"{{"jsonrpc":"2.0","id":1,"method":"{m}"}}"#);
            let err = decode_request(line.as_bytes()).err().map(|e| e.1.code);
            assert_ne!(err, Some(code::METHOD_NOT_FOUND), "{m}");
            assert_ne!(err, Some(code::INTERNAL), "{m}");
        }
    }

    #[test]
    fn responses_and_notifications_parse_as_incoming() {
        let ok = serde_json::to_string(&Response::ok(Id::Num(1), Value::from(3))).unwrap();
        assert!(matches!(serde_json::from_str::<Incoming>(&ok).unwrap(), Incoming::Response(_)));
        let n = serde_json::to_string(&Notification::new("snapshot", Value::Null)).unwrap();
        assert!(matches!(serde_json::from_str::<Incoming>(&n).unwrap(), Incoming::Notification(_)));
        let e = Response::err(None, RpcError::new(code::BUSY, "x"));
        assert_eq!(e.into_result().unwrap_err().code, code::BUSY);
    }

    #[test]
    fn read_only_classification() {
        assert!(Call::Status(Empty {}).is_read_only());
        assert!(!Call::Auto(Empty {}).is_read_only());
        assert!(!Call::Manual(ManualParams { cpu: 60, gpu: 60 }).is_read_only());
    }
}
