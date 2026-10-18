//! Hyprland IPC: `.socket.sock` requests (`j/monitors`, `j/clients`, `keyword monitor`)
//! and the `.socket2.sock` event stream. No extra crate: plain Unix sockets.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use r9t_protocol::session::OutputState;

pub fn dir() -> Option<PathBuf> {
    let sig = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let d = PathBuf::from(runtime).join("hypr").join(sig);
    d.join(".socket.sock").exists().then_some(d)
}

pub fn request(cmd: &str) -> std::io::Result<String> {
    let d = dir().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Hyprland yok"))?;
    let mut s = UnixStream::connect(d.join(".socket.sock"))?;
    s.set_read_timeout(Some(Duration::from_secs(2)))?;
    s.write_all(cmd.as_bytes())?;
    let mut out = String::new();
    s.read_to_string(&mut out)?;
    Ok(out)
}

pub fn json(cmd: &str) -> std::io::Result<Value> {
    serde_json::from_str(&request(&format!("j/{cmd}"))?).map_err(std::io::Error::other)
}

/// `2560x1600@300.00Hz` → (2560, 1600, 300.0)
pub fn parse_mode(m: &str) -> Option<(u32, u32, f32)> {
    let (res, hz) = m.split_once('@')?;
    let (w, h) = res.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?, hz.trim_end_matches("Hz").parse().ok()?))
}

pub fn outputs_from(v: &Value) -> Vec<OutputState> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let (w, h) = (m["width"].as_u64()? as u32, m["height"].as_u64()? as u32);
            let mut available: Vec<f32> = m["availableModes"]
                .as_array()?
                .iter()
                .filter_map(|x| parse_mode(x.as_str()?))
                .filter(|(mw, mh, _)| (*mw, *mh) == (w, h))
                .map(|(_, _, hz)| hz)
                .collect();
            available.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            available.dedup();
            Some(OutputState { name: m["name"].as_str()?.to_owned(), width: w, height: h, refresh_hz: m["refreshRate"].as_f64()? as f32, available_hz: available })
        })
        .collect()
}

pub fn outputs() -> std::io::Result<Vec<OutputState>> {
    Ok(outputs_from(&json("monitors")?))
}

/// `keyword monitor` line that keeps position, scale and transform, changing only Hz.
pub fn monitor_rule(m: &Value, hz: f32) -> Option<String> {
    let name = m["name"].as_str()?;
    let (w, h) = (m["width"].as_u64()?, m["height"].as_u64()?);
    let (x, y) = (m["x"].as_i64()?, m["y"].as_i64()?);
    let scale = m["scale"].as_f64()?;
    let transform = m["transform"].as_i64().unwrap_or(0);
    let mut rule = format!("{name},{w}x{h}@{hz:.2},{x}x{y},{scale}");
    if transform != 0 {
        rule.push_str(&format!(",transform,{transform}"));
    }
    Some(rule)
}

/// Changes only the refresh rate; refuses modes the compositor does not list and
/// verifies the result with a fresh `j/monitors`.
pub fn set_refresh(output: &str, hz: u32) -> Result<f32, String> {
    let mons = json("monitors").map_err(|e| e.to_string())?;
    let m = mons.as_array().into_iter().flatten().find(|m| m["name"] == output).ok_or("ekran bulunamadı")?;
    let state = outputs_from(&Value::Array(vec![m.clone()])).pop().ok_or("ekran okunamadı")?;
    let target = state.available_hz.iter().copied().find(|a| (a - hz as f32).abs() < 1.0).ok_or_else(|| format!("{hz} Hz bu çözünürlükte yok: {:?}", state.available_hz))?;
    let rule = monitor_rule(m, target).ok_or("monitör satırı oluşturulamadı")?;
    let reply = request(&format!("keyword monitor {rule}")).map_err(|e| e.to_string())?;
    if reply.trim() != "ok" {
        return Err(format!("Hyprland: {}", reply.trim()));
    }
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(100));
        if let Ok(now) = outputs() {
            if let Some(o) = now.iter().find(|o| o.name == output) {
                if (o.refresh_hz - target).abs() < 1.0 {
                    return Ok(o.refresh_hz);
                }
            }
        }
    }
    Err("yeni yenileme hızı geri okunamadı".into())
}

/// One `.socket2.sock` event: `name>>data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub name: String,
    pub data: String,
}

pub fn parse_event(line: &str) -> Option<Event> {
    let (name, data) = line.split_once(">>")?;
    Some(Event { name: name.to_owned(), data: data.to_owned() })
}

/// pid of a client window address (`0x...`) from `j/clients`.
pub fn pid_of(address: &str) -> Option<u32> {
    let want = if address.starts_with("0x") { address.to_owned() } else { format!("0x{address}") };
    json("clients").ok()?.as_array()?.iter().find(|c| c["address"] == want.as_str())?["pid"].as_u64().map(|p| p as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mon() -> Value {
        json!({"name":"eDP-1","width":2560,"height":1600,"refreshRate":300.0,"x":0,"y":0,"scale":1.0,"transform":0,
               "availableModes":["2560x1600@60.00Hz","2560x1600@300.00Hz","2560x1600@240.00Hz","1920x1200@60.00Hz"]})
    }

    #[test]
    fn modes_are_read_at_runtime_for_current_resolution() {
        let o = outputs_from(&Value::Array(vec![mon()])).pop().unwrap();
        assert_eq!(o.available_hz, vec![60.0, 240.0, 300.0]);
        assert_eq!(o.refresh_hz, 300.0);
    }

    #[test]
    fn rule_keeps_position_scale_transform() {
        assert_eq!(monitor_rule(&mon(), 240.0).unwrap(), "eDP-1,2560x1600@240.00,0x0,1");
        let mut m = mon();
        m["x"] = json!(1920);
        m["scale"] = json!(1.25);
        m["transform"] = json!(1);
        assert_eq!(monitor_rule(&m, 60.0).unwrap(), "eDP-1,2560x1600@60.00,1920x0,1.25,transform,1");
    }

    #[test]
    fn events_parse() {
        assert_eq!(parse_event("openwindow>>55d1,2,steam_app_730,Counter-Strike 2").unwrap().name, "openwindow");
        assert!(parse_event("garbage").is_none());
        assert_eq!(parse_mode("2560x1600@239.96Hz"), Some((2560, 1600, 239.96)));
    }
}
