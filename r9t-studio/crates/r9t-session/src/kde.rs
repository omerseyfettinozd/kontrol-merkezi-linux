//! KDE Plasma fallback via `kscreen-doctor` (only when the session is KDE and
//! `org.kde.KScreen` has an owner; short timeouts so nothing blocks the agent).

use std::process::Command;
use std::time::Duration;

use serde_json::Value;

use r9t_protocol::session::OutputState;

fn run(args: &[&str]) -> Result<String, String> {
    let mut child = Command::new("kscreen-doctor").args(args).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(_st) = child.try_wait().map_err(|e| e.to_string())? {
            let out = child.wait_with_output().map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
            }
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            return Err("kscreen-doctor zaman aşımı".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn doc() -> Result<Value, String> {
    serde_json::from_str(&run(&["-j"])?).map_err(|e| e.to_string())
}

pub fn outputs_from(v: &Value) -> Vec<OutputState> {
    v["outputs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["connected"] == true && o["enabled"] == true)
        .filter_map(|o| {
            let modes = o["modes"].as_array()?;
            let cur = modes.iter().find(|m| m["id"] == o["currentModeId"])?;
            let (w, h) = (cur["size"]["width"].as_u64()? as u32, cur["size"]["height"].as_u64()? as u32);
            let mut available: Vec<f32> = modes
                .iter()
                .filter(|m| m["size"]["width"].as_u64() == Some(w as u64) && m["size"]["height"].as_u64() == Some(h as u64))
                .filter_map(|m| m["refreshRate"].as_f64().map(|r| r as f32))
                .collect();
            available.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            available.dedup_by(|a, b| (*a - *b).abs() < 0.5);
            Some(OutputState { name: o["name"].as_str()?.to_owned(), width: w, height: h, refresh_hz: cur["refreshRate"].as_f64()? as f32, available_hz: available })
        })
        .collect()
}

pub fn outputs() -> Result<Vec<OutputState>, String> {
    Ok(outputs_from(&doc()?))
}

pub fn set_refresh(output: &str, hz: u32) -> Result<f32, String> {
    let d = doc()?;
    let o = d["outputs"].as_array().into_iter().flatten().find(|o| o["name"] == output).ok_or("ekran bulunamadı")?;
    let modes = o["modes"].as_array().ok_or("mod yok")?;
    let cur = modes.iter().find(|m| m["id"] == o["currentModeId"]).ok_or("geçerli mod yok")?;
    let target = modes
        .iter()
        .find(|m| m["size"] == cur["size"] && m["refreshRate"].as_f64().is_some_and(|r| (r - hz as f64).abs() < 1.0))
        .ok_or_else(|| format!("{hz} Hz bu çözünürlükte yok"))?;
    let id = o["id"].as_u64().ok_or("ekran kimliği yok")?;
    let mode_id = target["id"].as_str().ok_or("mod kimliği yok")?;
    run(&[&format!("output.{id}.mode.{mode_id}")])?;
    let now = outputs()?;
    now.iter().find(|x| x.name == output && (x.refresh_hz - hz as f32).abs() < 1.0).map(|x| x.refresh_hz).ok_or_else(|| "geri okunamadı".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kscreen_json_modes() {
        let v = json!({"outputs":[{"id":1,"name":"eDP-1","connected":true,"enabled":true,"currentModeId":"2",
            "modes":[{"id":"1","refreshRate":60.0,"size":{"width":2560,"height":1600}},
                     {"id":"2","refreshRate":300.0,"size":{"width":2560,"height":1600}},
                     {"id":"3","refreshRate":239.98,"size":{"width":2560,"height":1600}},
                     {"id":"4","refreshRate":60.0,"size":{"width":1920,"height":1200}}]}]});
        let o = outputs_from(&v).pop().unwrap();
        assert_eq!((o.width, o.refresh_hz), (2560, 300.0));
        assert_eq!(o.available_hz.len(), 3);
    }
}
