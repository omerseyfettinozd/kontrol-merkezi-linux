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
}

impl SessionCall {
    pub const METHODS: &'static [&'static str] = &[
        "status", "set_refresh_rate", "set_brightness", "set_volume", "set_mic_volume", "wifi",
        "bluetooth", "airplane", "gpu_processes", "kill_process",
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
