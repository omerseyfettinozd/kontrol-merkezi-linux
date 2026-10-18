//! GPU wake inspector helpers: processes holding the dGPU (Graphics + Compute, via
//! NVML only while it is active) and safe termination: same uid, not on the
//! blacklist, start time unchanged, signalled through a pidfd.

use std::os::fd::{FromRawFd, OwnedFd};

use r9t_hal::gpu::{Gpu, Nvml, ProcessKind};
use serde_json::{json, Value};

/// Desktop plumbing that must never be killed from the UI.
pub const BLACKLIST: &[&str] = &[
    "Hyprland", "Xwayland", "kwin_wayland", "kwin_x11", "plasmashell", "xdg-desktop-portal", "xdg-desktop-portal-hyprland",
    "xdg-desktop-portal-kde", "polkit-kde-authentication-agent-1", "hyprpolkitagent", "waybar", "pipewire", "wireplumber",
    "pipewire-pulse", "systemd", "dbus-daemon", "dbus-broker", "r9t-session", "sddm", "gdm", "Xorg",
];

pub fn comm(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm")).ok().map(|s| s.trim().to_owned())
}

pub fn start_time(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()
}

fn uid_of(pid: u32) -> Option<u32> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    s.lines().find(|l| l.starts_with("Uid:"))?.split_whitespace().nth(1)?.parse().ok()
}

pub fn gpu_processes(nvml: &Nvml) -> Value {
    let status = nvml.runtime_status().unwrap_or_else(|e| format!("bilinmiyor ({e})"));
    if status != "active" {
        return json!({"runtime_status": status, "note": "dGPU uyuyor (D3); NVML çağrılmadı", "processes": []});
    }
    let me = nix::unistd::getuid().as_raw();
    let list = match nvml.processes() {
        Ok(l) => l,
        Err(e) => return json!({"runtime_status": status, "error": e.to_string(), "processes": []}),
    };
    let procs: Vec<Value> = list
        .iter()
        .map(|p| {
            let name = comm(p.pid);
            let killable = uid_of(p.pid) == Some(me) && name.as_deref().is_some_and(|n| !BLACKLIST.contains(&n));
            json!({"pid": p.pid, "kind": if p.kind == ProcessKind::Graphics { "G" } else { "C" }, "name": name,
                   "used_mib": p.used_mib, "start_time": start_time(p.pid), "killable": killable})
        })
        .collect();
    json!({"runtime_status": status,
           "note": "Ekran (eDP-1) dGPU'ya bağlı olduğundan dGPU bu yapılandırmada D3cold'a giremez.",
           "processes": procs})
}

/// SIGTERM through a pidfd after re-checking uid, name and start time.
pub fn kill(pid: u32, expected_start: u64) -> Result<String, String> {
    let me = nix::unistd::getuid().as_raw();
    let name = comm(pid).ok_or("süreç yok")?;
    if BLACKLIST.contains(&name.as_str()) {
        return Err(format!("{name} masaüstü bileşeni; kapatılmaz"));
    }
    if uid_of(pid) != Some(me) {
        return Err("yalnız kendi süreçleriniz kapatılabilir".into());
    }
    // SAFETY: pidfd_open(2) with a plain pid; returns a new descriptor or -1.
    let fd = unsafe { nix::libc::syscall(nix::libc::SYS_pidfd_open, pid as nix::libc::pid_t, 0) };
    if fd < 0 {
        return Err(format!("pidfd_open: {}", std::io::Error::last_os_error()));
    }
    // SAFETY: `fd` was just returned by the kernel and is owned here.
    let pidfd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
    // The pidfd pins the process; now the start time must still match.
    if start_time(pid) != Some(expected_start) {
        return Err("süreç değişti (pid yeniden kullanıldı); işlem yapılmadı".into());
    }
    use std::os::fd::AsRawFd;
    // SAFETY: pidfd_send_signal(2) on a valid pidfd, no siginfo, no flags.
    let rc = unsafe { nix::libc::syscall(nix::libc::SYS_pidfd_send_signal, pidfd.as_raw_fd(), nix::libc::SIGTERM, std::ptr::null::<u8>(), 0) };
    if rc != 0 {
        return Err(format!("pidfd_send_signal: {}", std::io::Error::last_os_error()));
    }
    Ok(format!("{name} ({pid}) için SIGTERM gönderildi"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_blacklisted_foreign_and_reused() {
        let me = std::process::id();
        assert!(kill(me, start_time(me).unwrap() + 1).unwrap_err().contains("değişti"));
        assert!(kill(1, start_time(1).unwrap_or(0)).is_err(), "pid 1 (systemd) blacklisted / foreign");
        assert!(kill(u32::MAX - 1, 0).is_err());
    }

    #[test]
    fn terminates_own_child() {
        let mut child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        let st = start_time(pid).unwrap();
        kill(pid, st).unwrap();
        let status = child.wait().unwrap();
        assert!(!status.success());
    }
}
