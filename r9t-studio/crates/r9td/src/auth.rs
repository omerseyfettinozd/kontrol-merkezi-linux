//! polkit authorization for control methods (plan "Yetki").
//!
//! The caller's pid comes from SO_PEERCRED; its start time (`/proc/<pid>/stat`
//! field 22) is captured at connect and re-checked after polkit answers, so a pid
//! reused in between is not authorized.

use std::collections::HashMap;

use zbus::zvariant::Value;

pub const ACTION_CONTROL: &str = "org.r9t.control";
pub const ACTION_DANGEROUS: &str = "org.r9t.dangerous";

/// Process start time in clock ticks since boot.
pub fn start_time(pid: i32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // comm may contain spaces/parentheses: split after the last ')'.
    stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allowed,
    Denied(String),
}

/// Asks polkit whether `pid`/`uid` may perform `action`. Root is always allowed.
pub fn check(conn: &zbus::blocking::Connection, pid: i32, uid: u32, start: u64, action: &str) -> Decision {
    if uid == 0 {
        return Decision::Allowed;
    }
    let proxy = match zbus::blocking::Proxy::new(
        conn,
        "org.freedesktop.PolicyKit1",
        "/org/freedesktop/PolicyKit1/Authority",
        "org.freedesktop.PolicyKit1.Authority",
    ) {
        Ok(p) => p,
        Err(e) => return Decision::Denied(format!("polkit erişilemedi: {e}")),
    };
    let mut subject: HashMap<&str, Value> = HashMap::new();
    subject.insert("pid", Value::U32(pid as u32));
    subject.insert("start-time", Value::U64(start));
    subject.insert("uid", Value::I32(uid as i32));
    let details: HashMap<&str, &str> = HashMap::new();
    // flags 1 = AllowUserInteraction (lets an agent prompt for auth_admin_keep).
    let reply: Result<(bool, bool, HashMap<String, String>), _> =
        proxy.call("CheckAuthorization", &(("unix-process", subject), action, details, 1u32, ""));
    match reply {
        Ok((true, _, _)) => {
            if start_time(pid) == Some(start) {
                Decision::Allowed
            } else {
                Decision::Denied("istemci süreci değişti".into())
            }
        }
        Ok((false, true, _)) => Decision::Denied("polkit kimlik doğrulaması gerekli ama yapılmadı".into()),
        Ok((false, false, _)) => Decision::Denied(format!("polkit {action} iznini vermedi")),
        Err(e) => Decision::Denied(format!("polkit hatası: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_start_time_is_readable_and_stable() {
        let me = std::process::id() as i32;
        let a = start_time(me).unwrap();
        assert_eq!(start_time(me), Some(a));
        assert_eq!(start_time(-1), None);
    }
}
