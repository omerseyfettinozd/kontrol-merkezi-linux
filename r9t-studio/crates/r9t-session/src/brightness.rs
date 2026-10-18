//! Panel brightness through logind `Session.SetBrightness` (no root, works on any
//! desktop). The eDP panel is driven by the dGPU, so the device is `nvidia_0`.

pub fn device() -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir("/sys/class/backlight").ok()?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names.iter().find(|n| n.starts_with("nvidia")).or(names.first()).cloned()
}

fn read(dev: &str, f: &str) -> Option<u32> {
    std::fs::read_to_string(format!("/sys/class/backlight/{dev}/{f}")).ok()?.trim().parse().ok()
}

pub fn get() -> Option<u8> {
    let d = device()?;
    let (cur, max) = (read(&d, "brightness")?, read(&d, "max_brightness")?);
    (max > 0).then(|| ((cur as f32 / max as f32) * 100.0).round() as u8)
}

pub fn set(pct: u8) -> Result<u8, String> {
    if !(5..=100).contains(&pct) {
        return Err("parlaklık %5–100 olmalı".into());
    }
    let dev = device().ok_or("arka ışık aygıtı yok")?;
    let max = read(&dev, "max_brightness").ok_or("max_brightness okunamadı")?;
    let raw = ((pct as f32 / 100.0) * max as f32).round() as u32;
    let conn = zbus::blocking::Connection::system().map_err(|e| e.to_string())?;
    let session = zbus::blocking::Proxy::new(&conn, "org.freedesktop.login1", "/org/freedesktop/login1/session/auto", "org.freedesktop.login1.Session").map_err(|e| e.to_string())?;
    let _: () = session.call("SetBrightness", &("backlight", dev.as_str(), raw)).map_err(|e| format!("logind: {e}"))?;
    std::thread::sleep(std::time::Duration::from_millis(50));
    match get() {
        Some(now) if now.abs_diff(pct) <= 1 => Ok(now),
        other => Err(format!("geri okuma {other:?}")),
    }
}
