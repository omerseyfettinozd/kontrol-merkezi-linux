//! r9td library: pure control logic (thermal authority, automation) and the daemon
//! runtime pieces. The binary in `main.rs` only wires them together.

pub mod audit;
pub mod auth;
pub mod control;
pub mod history;
pub mod legacy;
pub mod policy;
pub mod sampler;
pub mod server;
pub mod state;
pub mod thermal;

use std::time::{Duration, Instant};

/// NVML preflight (see `r9t_hal::gpu`): runs `r9td --nvml-probe` with a 2 s limit, so
/// neither a failing nor a hanging early-boot init happens inside the daemon. When no
/// child can be started the in-process init goes ahead.
pub fn nvml_preflight() -> bool {
    let Ok(exe) = std::env::current_exe() else { return true };
    let Ok(mut child) = std::process::Command::new(exe)
        .arg("--nvml-probe")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return true;
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}
