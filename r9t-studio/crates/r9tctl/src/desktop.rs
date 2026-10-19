//! `r9tctl desktop` (also installed as `r9t-switch-user`): desktop side of the
//! permanent switch, run as the desktop user after `r9tctl switch --commit`.
//! r9t-session replaces the legacy session agent, the Hyprland exec-once line starts
//! it and pulls in the tray (r9t-studio-tray), the menu shows R9T Studio. Every change is undone by `--rollback`; `--dry-run`
//! (after the command) prints what would change.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Legacy line 50 and its replacement (packaging/hyprland/r9t-session.conf).
pub const OLD_LINE: &str = "exec-once = systemctl --user import-environment WAYLAND_DISPLAY XDG_CURRENT_DESKTOP HYPRLAND_INSTANCE_SIGNATURE 2>/dev/null";
pub const NEW_LINE: &str = "exec-once = systemctl --user import-environment WAYLAND_DISPLAY XDG_CURRENT_DESKTOP HYPRLAND_INSTANCE_SIGNATURE && systemctl --user start r9t-session.service";
const MARK: &str = "# r9tctl desktop";

struct Ctx {
    hypr: PathBuf,
    legacy_desktop: PathBuf,
    dry: bool,
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(fallback))
}

impl Ctx {
    fn new(dry: bool) -> Self {
        let hypr = xdg("XDG_CONFIG_HOME", ".config").join("hypr/hyprland.conf");
        let hypr = std::fs::canonicalize(&hypr).unwrap_or(hypr);
        Self { hypr, legacy_desktop: xdg("XDG_DATA_HOME", ".local/share").join("applications/slayer-r9t-control-center.desktop"), dry }
    }

    fn run(&self, cmd: &str, args: &[&str]) -> Result<(), String> {
        if self.dry {
            println!("  [dry-run] {cmd} {}", args.join(" "));
            return Ok(());
        }
        let st = Command::new(cmd).args(args).status().map_err(|e| format!("{cmd}: {e}"))?;
        if st.success() { Ok(()) } else { Err(format!("{cmd} {} başarısız", args.join(" "))) }
    }

    fn user_systemctl(&self, args: &[&str]) -> Result<(), String> {
        let mut a = vec!["--user"];
        a.extend_from_slice(args);
        self.run("systemctl", &a)
    }

    fn refresh_menu(&self) {
        if let Some(dir) = self.legacy_desktop.parent().and_then(Path::to_str) {
            let _ = self.run("update-desktop-database", &["-q", dir]);
        }
    }
}

/// Replace one exact line; refuse if it is missing or not unique (hand-edited file).
/// Returns the new text, or `None` when the file already has `to`.
pub fn swap_line(text: &str, from: &str, to: &str) -> Result<Option<String>, String> {
    let n = text.split('\n').filter(|l| *l == from).count();
    if n == 0 && text.split('\n').any(|l| l == to) {
        return Ok(None);
    }
    if n != 1 {
        return Err(format!("hyprland.conf: beklenen satır {n} kez bulundu, dosyaya dokunulmadı:\n  {from}"));
    }
    Ok(Some(text.split('\n').map(|l| if l == from { to } else { l }).collect::<Vec<_>>().join("\n")))
}

fn swap_file(c: &Ctx, from: &str, to: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(&c.hypr).map_err(|e| format!("{}: {e}", c.hypr.display()))?;
    match swap_line(&text, from, to)? {
        None => println!("  hyprland.conf zaten güncel"),
        Some(_) if c.dry => println!("  [dry-run] {}: satır değişir → {to}", c.hypr.display()),
        Some(new) => {
            let bak = PathBuf::from(format!("{}.r9t-bak", c.hypr.display()));
            std::fs::copy(&c.hypr, &bak).map_err(|e| e.to_string())?;
            std::fs::write(&c.hypr, new).map_err(|e| e.to_string())?;
            println!("  hyprland.conf güncellendi (yedek: {})", bak.display());
        }
    }
    Ok(())
}

fn has_line(path: &Path, line: &str) -> bool {
    std::fs::read_to_string(path).is_ok_and(|t| t.lines().any(|l| l == line))
}

fn commit(c: &Ctx) -> Result<(), String> {
    let unit_ok = Command::new("systemctl").args(["--user", "cat", "r9t-session.service"]).output().is_ok_and(|o| o.status.success());
    if !crate::switch::lib_dir().join("r9t-session").exists() || !unit_ok {
        return Err("r9t-session kurulu değil: önce paket kurulumu".into());
    }
    let enabled = Command::new("systemctl").args(["is-enabled", "r9td.service"]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim() == "enabled");
    if !enabled.unwrap_or(false) {
        eprintln!("Uyarı: r9td açılışta etkin değil; önce sudo r9tctl switch --commit");
    }
    println!("1. Eski oturum ajanı kapatılıyor, r9t-session ve tepsi etkinleşiyor (KDE: graphical-session.target)");
    c.user_systemctl(&["disable", "--now", "slayer-r9t-session.service"])?;
    c.user_systemctl(&["enable", "r9t-session.service"])?;
    c.user_systemctl(&["start", "r9t-session.service"])?;
    // Tray at login (WantedBy=r9t-session); a running UI just forwards to itself.
    c.user_systemctl(&["enable", "--now", "r9t-studio-tray.service"])?;
    println!("2. Hyprland exec-once satırı");
    swap_file(c, OLD_LINE, NEW_LINE)?;
    println!("3. Menü: eski 'Slayer R9T Kontrol Merkezi' gizleniyor, R9T Studio görünüyor");
    let d = &c.legacy_desktop;
    if d.exists() && !has_line(d, "NoDisplay=true") {
        if c.dry {
            println!("  [dry-run] {} += NoDisplay=true", d.display());
        } else {
            std::fs::copy(d, format!("{}.r9t-bak", d.display())).map_err(|e| e.to_string())?;
            let mut t = std::fs::read_to_string(d).map_err(|e| e.to_string())?;
            if !t.ends_with('\n') {
                t.push('\n');
            }
            t.push_str("NoDisplay=true\n");
            std::fs::write(d, t).map_err(|e| e.to_string())?;
        }
    } else if !d.exists() {
        // Shadows /usr/share/applications/slayer-r9t-control-center.desktop for this user only.
        if c.dry {
            println!("  [dry-run] {} oluşturulur (NoDisplay=true)", d.display());
        } else {
            if let Some(p) = d.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            std::fs::write(d, format!("[Desktop Entry]\nType=Application\nName=Slayer R9T Kontrol Merkezi\nNoDisplay=true\n{MARK}\n")).map_err(|e| e.to_string())?;
        }
    }
    if !Path::new("/usr/share/applications/r9t-studio.desktop").exists() && !Path::new("/usr/local/share/applications/r9t-studio.desktop").exists() {
        eprintln!("  Uyarı: R9T Studio menü girişi kurulu değil");
    }
    c.refresh_menu();
    println!("Masaüstü geçişi tamam. Geri dönüş: r9tctl desktop --rollback");
    Ok(())
}

fn rollback(c: &Ctx) -> Result<(), String> {
    if std::path::Path::new(crate::legacy::REMOVED).exists() {
        return Err("eski sistem kaldırıldı; masaüstü geri dönüşü yok (önce sistem yedeğini geri yükleyin)".into());
    }
    println!("1. r9t-session ve tepsi kapatılıyor, eski oturum ajanı geri");
    c.user_systemctl(&["disable", "--now", "r9t-studio-tray.service"])?;
    c.user_systemctl(&["disable", "--now", "r9t-session.service"])?;
    c.user_systemctl(&["enable", "slayer-r9t-session.service"])?;
    println!("2. Hyprland exec-once satırı");
    swap_file(c, NEW_LINE, OLD_LINE)?;
    println!("3. Menü");
    let d = &c.legacy_desktop;
    let bak = PathBuf::from(format!("{}.r9t-bak", d.display()));
    if bak.exists() {
        if c.dry {
            println!("  [dry-run] {} → {}", bak.display(), d.display());
        } else {
            std::fs::rename(&bak, d).map_err(|e| e.to_string())?;
        }
    } else if d.exists() && (has_line(d, MARK) || has_line(d, "# r9t-switch-user")) {
        if c.dry {
            println!("  [dry-run] {} silinir", d.display());
        } else {
            std::fs::remove_file(d).map_err(|e| e.to_string())?;
        }
    }
    c.refresh_menu();
    println!("Masaüstü eski düzene döndü. Sistem tarafı: sudo r9tctl switch --rollback");
    Ok(())
}

fn status(c: &Ctx) -> Result<(), String> {
    for u in ["r9t-session.service", "r9t-studio-tray.service", "slayer-r9t-session.service"] {
        let q = |verb: &str| {
            Command::new("systemctl").args(["--user", verb, u]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default()
        };
        println!("{u:<28} {:<9} {}", q("is-active"), q("is-enabled"));
    }
    let h = if has_line(&c.hypr, NEW_LINE) { "r9t-session" } else if has_line(&c.hypr, OLD_LINE) { "eski" } else { "tanınmadı" };
    println!("hyprland.conf: {h}");
    let hidden = has_line(&c.legacy_desktop, "NoDisplay=true");
    println!("menü: eski giriş {}", if hidden { "gizli" } else { "görünür" });
    Ok(())
}

pub const USAGE: &str = "Kullanım: r9tctl desktop --commit | --rollback | --status [--dry-run] | --remove-legacy [--yes]";

pub fn run(args: &[String]) -> ExitCode {
    if crate::switch::euid() == 0 {
        eprintln!("kullanıcı olarak çalıştırın (sudo değil)");
        return ExitCode::from(1);
    }
    let c = Ctx::new(args.iter().any(|a| a == "--dry-run"));
    let r = match args.first().map(String::as_str) {
        Some("--commit") => commit(&c),
        Some("--rollback") => rollback(&c),
        Some("--status") => status(&c),
        Some("--remove-legacy") => crate::legacy::home(args.iter().any(|a| a == "--yes")),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(m) => {
            eprintln!("{m}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_round_trips_byte_for_byte() {
        let original = format!("a\n{OLD_LINE}\nb\n");
        let new = swap_line(&original, OLD_LINE, NEW_LINE).unwrap().unwrap();
        assert!(new.contains(NEW_LINE) && !new.contains(OLD_LINE));
        assert_eq!(swap_line(&new, OLD_LINE, NEW_LINE).unwrap(), None, "idempotent");
        assert_eq!(swap_line(&new, NEW_LINE, OLD_LINE).unwrap().unwrap(), original);
    }

    #[test]
    fn swap_refuses_missing_or_duplicate_lines() {
        assert!(swap_line("a\nb\n", OLD_LINE, NEW_LINE).is_err());
        assert!(swap_line(&format!("{OLD_LINE}\n{OLD_LINE}\n"), OLD_LINE, NEW_LINE).is_err());
    }
}
