//! Wi-Fi / Bluetooth / airplane via `/dev/rfkill` (desktop independent; the user has
//! an ACL on it through logind). NetworkManager and BlueZ follow the soft block.

use std::io::Write;

const TYPE_WLAN: u8 = 1;
const TYPE_BLUETOOTH: u8 = 2;
const OP_CHANGE_ALL: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Radio {
    Wifi,
    Bluetooth,
}

impl Radio {
    fn kind(self) -> (u8, &'static str) {
        match self {
            Self::Wifi => (TYPE_WLAN, "wlan"),
            Self::Bluetooth => (TYPE_BLUETOOTH, "bluetooth"),
        }
    }
}

/// `Some(true)` when every device of that type is unblocked.
pub fn enabled(r: Radio) -> Option<bool> {
    let (_, name) = r.kind();
    let mut seen = false;
    let mut on = true;
    for e in std::fs::read_dir("/sys/class/rfkill").ok()?.flatten() {
        let p = e.path();
        if std::fs::read_to_string(p.join("type")).ok()?.trim() != name {
            continue;
        }
        seen = true;
        let soft = std::fs::read_to_string(p.join("soft")).ok()?.trim() == "1";
        let hard = std::fs::read_to_string(p.join("hard")).ok()?.trim() == "1";
        on &= !soft && !hard;
    }
    seen.then_some(on)
}

pub fn set(r: Radio, on: bool) -> Result<(), String> {
    let (kind, _) = r.kind();
    // struct rfkill_event { u32 idx; u8 type; u8 op; u8 soft; u8 hard; }
    let mut ev = [0u8; 8];
    ev[4] = kind;
    ev[5] = OP_CHANGE_ALL;
    ev[6] = u8::from(!on);
    let mut f = std::fs::OpenOptions::new().write(true).open("/dev/rfkill").map_err(|e| format!("/dev/rfkill: {e}"))?;
    f.write_all(&ev).map_err(|e| e.to_string())?;
    for _ in 0..20 {
        if enabled(r) == Some(on) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Err("rfkill durumu doğrulanamadı (donanım anahtarı?)".into())
}

/// Airplane on = both radios off; restores both on failure.
pub fn airplane(on: bool) -> Result<(), String> {
    let before = (enabled(Radio::Wifi), enabled(Radio::Bluetooth));
    let r = set(Radio::Wifi, !on).and_then(|_| set(Radio::Bluetooth, !on));
    if r.is_err() {
        if let Some(w) = before.0 {
            let _ = set(Radio::Wifi, w);
        }
        if let Some(b) = before.1 {
            let _ = set(Radio::Bluetooth, b);
        }
    }
    r
}
