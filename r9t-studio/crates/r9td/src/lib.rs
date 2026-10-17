//! r9td library: pure control logic (thermal authority, automation) and the daemon
//! runtime pieces. The binary in `main.rs` only wires them together.

pub mod audit;
pub mod history;
pub mod legacy;
pub mod policy;
pub mod sampler;
pub mod server;
pub mod state;
pub mod thermal;
