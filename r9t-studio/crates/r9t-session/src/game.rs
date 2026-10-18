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
