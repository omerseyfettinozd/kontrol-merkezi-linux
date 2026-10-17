//! R9T Studio wire protocol shared by `r9td`, `r9t-session`, `r9tctl` and the UI.
//!
//! Transport: one JSON object per line (NDJSON) over a Unix socket. Requests are
//! JSON-RPC 2.0 without batches; unknown fields are rejected. See `docs/04_ana_plan.md`.

pub mod capability;
pub mod frame;
pub mod policy;
pub mod presets;
pub mod profile;
pub mod rpc;
pub mod session;
pub mod snapshot;

/// Bumped on any incompatible change to methods, params or results.
pub const PROTOCOL_VERSION: u32 = 1;
/// Maximum size of one request line, newline excluded.
pub const REQUEST_MAX: usize = 64 * 1024;
/// Maximum size of one response or notification line, newline excluded.
pub const RESPONSE_MAX: usize = 1024 * 1024;
/// A subscriber that falls this many messages behind is disconnected.
pub const SUBSCRIBER_BACKLOG: usize = 256;

/// Root daemon socket (systemd socket unit, 0660 root:r9t).
pub const DAEMON_SOCKET: &str = "/run/r9td/r9td.sock";
/// Session agent socket name below `$XDG_RUNTIME_DIR`.
pub const SESSION_SOCKET: &str = "r9t/session.sock";
