//! Safety invariant 7: hold a logind `delay` sleep inhibitor; on `PrepareForSleep(true)`
//! let the control thread return the fans to auto (≤ 2 s), then release it.

use std::time::Duration;

use zbus::blocking::{Connection, Proxy};

/// Upper bound for the pre-sleep handover before the inhibitor is released anyway.
pub const MAX_DELAY: Duration = Duration::from_secs(2);

fn manager(conn: &Connection) -> zbus::Result<Proxy<'_>> {
    Proxy::new(conn, "org.freedesktop.login1", "/org/freedesktop/login1", "org.freedesktop.login1.Manager")
}

fn inhibit(mgr: &Proxy<'_>) -> zbus::Result<zbus::zvariant::OwnedFd> {
    mgr.call("Inhibit", &("sleep", "r9td", "Fanlar uyku öncesi EC otomatiğine alınıyor", "delay"))
}

/// Runs forever on its own thread. `on_sleep(true)` must finish the handover and
/// return; `on_sleep(false)` is sent after resume.
pub fn run(on_sleep: impl Fn(bool) + Send + 'static) {
    let conn = match Connection::system() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("r9td: logind'e bağlanılamadı, uyku koruması yalnız çekirdekte: {e}");
            return;
        }
    };
    let mgr = match manager(&conn) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("r9td: logind proxy: {e}");
            return;
        }
    };
    let mut lock = inhibit(&mgr).map_err(|e| eprintln!("r9td: uyku inhibitor alınamadı: {e}")).ok();
    let signals = match mgr.receive_signal("PrepareForSleep") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("r9td: PrepareForSleep dinlenemiyor: {e}");
            return;
        }
    };
    for msg in signals {
        let Ok(entering) = msg.body().deserialize::<bool>() else { continue };
        if entering {
            // `on_sleep` is bounded by MAX_DELAY on the control side.
            on_sleep(true);
            lock = None; // release: sleep may proceed
        } else {
            on_sleep(false);
            if lock.is_none() {
                lock = inhibit(&mgr).map_err(|e| eprintln!("r9td: inhibitor yenilenemedi: {e}")).ok();
            }
        }
    }
}
