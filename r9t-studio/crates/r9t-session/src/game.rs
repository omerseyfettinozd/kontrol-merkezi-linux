//! Game detection from window events: Steam app classes, Wine/Proton and Steam
//! library executables (`/proc/<pid>/exe`, read once per event).

pub fn is_game(class: &str, exe: Option<&str>) -> bool {
    let c = class.to_ascii_lowercase();
    if c.starts_with("steam_app_") || c == "gamescope" || c.ends_with(".exe") {
        return true;
    }
    match exe {
        Some(e) => {
            let e = e.to_ascii_lowercase();
            e.contains("/steamapps/common/") || e.ends_with("/wine64-preloader") || e.ends_with("/wine-preloader") || e.contains("/proton")
        }
        None => false,
    }
}

pub fn exe_of(pid: u32) -> Option<String> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok().map(|p| p.to_string_lossy().into_owned())
}

/// Installed Steam games from `steamapps/appmanifest_*.acf` (all library folders).
pub fn steam_games() -> Vec<serde_json::Value> {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let mut libs = vec![home.join(".local/share/Steam/steamapps"), home.join(".steam/steam/steamapps")];
    if let Ok(vdf) = std::fs::read_to_string(home.join(".local/share/Steam/steamapps/libraryfolders.vdf")) {
        for line in vdf.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("\"path\"") {
                let path = rest.trim().trim_matches('"').replace("\\\\", "\\");
                libs.push(std::path::PathBuf::from(path).join("steamapps"));
            }
        }
    }
    let field = |text: &str, key: &str| {
        text.lines().find_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix(&format!("\"{key}\""))?;
            Some(rest.trim().trim_matches('"').to_owned())
        })
    };
    let mut seen = std::collections::BTreeMap::new();
    for lib in libs {
        let Ok(rd) = std::fs::read_dir(&lib) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
            let (Some(app), Some(title)) = (field(&text, "appid"), field(&text, "name")) else { continue };
            let lower = title.to_ascii_lowercase();
            if ["proton", "steam linux runtime", "steamworks", "redistributable"].iter().any(|k| lower.contains(k)) {
                continue;
            }
            let dir = field(&text, "installdir").map(|d| lib.join("common").join(d).to_string_lossy().into_owned());
            seen.insert(app.clone(), serde_json::json!({"appid": app, "name": title, "dir": dir, "class": format!("steam_app_{app}")}));
        }
    }
    seen.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification() {
        assert!(is_game("steam_app_730", None));
        assert!(is_game("cs2", Some("/home/u/.local/share/Steam/steamapps/common/Counter-Strike Global Offensive/game/bin/linuxsteamrt64/cs2")));
        assert!(is_game("battle.net.exe", None));
        assert!(is_game("x", Some("/usr/lib/wine/x86_64-unix/wine64-preloader")));
        assert!(!is_game("kitty", Some("/usr/bin/kitty")));
        assert!(!is_game("steam", Some("/usr/lib/steam/steam")));
    }
}
