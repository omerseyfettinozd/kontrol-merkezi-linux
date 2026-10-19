//! Removing the legacy Python system (plan Faz 5), after a week on r9td.
//!
//! - `sudo r9tctl legacy-remove [--yes] [--force-early]`: system files. Without `--yes`
//!   it only prints the plan. Refuses unless the permanent switch is at least 7 days
//!   old (`--force-early` skips only that), r9td is active, the legacy service is not,
//!   no path or parent is a symlink and no path belongs to a package. Everything is
//!   archived to `/var/lib/r9t/legacy-backup-<time>.tar` before deletion.
//! - `r9tctl desktop --remove-legacy [--yes]` (as the user, after the above): the
//!   copies under `~/.local`, archived to `~/.local/state/r9t/`. The legacy settings
//!   in `~/.config/slayer-r9t-control-center` stay (the `r9tctl migrate` source).
//!
//! Afterwards `r9tctl switch --fallback` keeps the fans in EC auto instead of starting
//! the legacy service, and `--rollback`/`--trial` refuse with the restore command.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, SystemTime};

pub const REMOVED: &str = "/var/lib/r9t/legacy-removed";
const COMMITTED: &str = "/var/lib/r9t/committed";
const MIN_AGE: Duration = Duration::from_secs(7 * 24 * 3600);

/// Legacy system paths (none belongs to a package).
pub const SYSTEM: [&str; 8] = [
    "/usr/local/lib/slayer-r9t",
    "/var/lib/slayer-r9t",
    "/etc/systemd/system/slayer-r9t.service",
    "/etc/systemd/system/slayer-r9t.service.d",
    "/etc/systemd/user/slayer-r9t-session.service",
    "/usr/share/applications/slayer-r9t-control-center.desktop",
    "/etc/modules-load.d/slayer-r9t.conf",
    "/var/lib/r9t/slayer-r9t.modules-load.conf.bak",
];

/// Legacy paths under the user's home.
pub const HOME: [&str; 3] = [
    ".local/lib/slayer-r9t",
    ".local/share/applications/slayer-r9t-control-center.desktop",
    ".local/share/applications/slayer-r9t-control-center.desktop.r9t-bak",
];

/// `root` + absolute or home-relative path.
fn under(root: &Path, p: &str) -> PathBuf {
    root.join(p.trim_start_matches('/'))
}

/// Existing paths to remove. Refuses when the path or any parent below `root` is a
/// symlink: deletion must never follow a link out of the expected location.
pub fn plan(root: &Path, paths: &[&str]) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    for p in paths {
        let full = under(root, p);
        let mut cur = root.to_path_buf();
        for part in Path::new(p.trim_start_matches('/')).components() {
            cur.push(part);
            match std::fs::symlink_metadata(&cur) {
                Ok(m) if m.file_type().is_symlink() => return Err(format!("{}: sembolik bağ, dokunulmadı", cur.display())),
                Ok(_) => {}
                Err(_) => break,
            }
        }
        if std::fs::symlink_metadata(&full).is_ok() {
            found.push(full);
        }
    }
    Ok(found)
}

/// The permanent switch must be at least a week old.
pub fn old_enough(committed: SystemTime, now: SystemTime) -> bool {
    now.duration_since(committed).is_ok_and(|d| d >= MIN_AGE)
}

fn size(p: &Path) -> u64 {
    let Ok(m) = std::fs::symlink_metadata(p) else { return 0 };
    if !m.is_dir() {
        return m.len();
    }
    std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| size(&e.path())).sum()
}

fn print_plan(found: &[PathBuf]) {
    for p in found {
        println!("  {}  ({} KB)", p.display(), size(p).div_ceil(1024));
    }
}

/// `tar -cpf out -C root <relative paths>`, then a listing check.
fn archive(root: &Path, found: &[PathBuf], out: &Path) -> Result<(), String> {
    let rel: Vec<PathBuf> = found.iter().map(|p| p.strip_prefix(root).unwrap_or(p).to_path_buf()).collect();
    let st = Command::new("tar").arg("-cpf").arg(out).arg("-C").arg(root).args(&rel).status().map_err(|e| format!("tar: {e}"))?;
    if !st.success() {
        return Err(format!("tar başarısız: {}", out.display()));
    }
    let list = Command::new("tar").arg("-tf").arg(out).output().map_err(|e| format!("tar -t: {e}"))?;
    let listed = String::from_utf8_lossy(&list.stdout);
    for r in &rel {
        let r = r.to_string_lossy();
        if !listed.lines().any(|l| l.trim_end_matches('/') == r) {
            return Err(format!("yedekte yok: {r}"));
        }
    }
    Ok(())
}

fn remove(found: &[PathBuf]) -> Result<(), String> {
    for p in found {
        let r = if std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir()) { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
        r.map_err(|e| format!("{}: {e}", p.display()))?;
    }
    Ok(())
}

fn stamp() -> String {
    Command::new("date").arg("+%Y%m%d-%H%M%S").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_else(|_| "now".into())
}

fn quiet(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd).args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default()
}

fn system(args: &[String]) -> Result<(), String> {
    let yes = args.iter().any(|a| a == "--yes");
    let early = args.iter().any(|a| a == "--force-early");
    if crate::switch::euid() != 0 {
        return Err("root gerekli: sudo r9tctl legacy-remove".into());
    }
    if Path::new(REMOVED).exists() {
        return Err(format!("eski sistem zaten kaldırılmış: {}", std::fs::read_to_string(REMOVED).unwrap_or_default().trim()));
    }
    let committed = std::fs::metadata(COMMITTED).and_then(|m| m.modified()).map_err(|_| "kalıcı geçiş yok (r9tctl switch --commit)".to_owned())?;
    if !early && !old_enough(committed, SystemTime::now()) {
        let days = SystemTime::now().duration_since(committed).map(|d| d.as_secs() / 86400).unwrap_or(0);
        return Err(format!("kalıcı geçişten bu yana {days} gün geçti; plan 1 hafta günlük kullanım istiyor (--force-early ile atlanır)"));
    }
    if quiet("systemctl", &["is-active", "r9td.service"]) != "active" {
        return Err("r9td etkin değil".into());
    }
    if quiet("systemctl", &["is-active", "slayer-r9t.service"]) == "active" {
        return Err("eski servis çalışıyor".into());
    }
    let root = Path::new("/");
    let found = plan(root, &SYSTEM)?;
    for p in &found {
        if Command::new("pacman").arg("-Qo").arg(p).output().is_ok_and(|o| o.status.success()) {
            return Err(format!("{}: bir pakete ait, dokunulmadı", p.display()));
        }
    }
    if found.is_empty() {
        println!("Kaldırılacak eski sistem dosyası yok.");
        return Ok(());
    }
    println!("Kaldırılacak (önce yedeklenir):");
    print_plan(&found);
    if !yes {
        println!("Yalnız plan. Uygulamak için: sudo r9tctl legacy-remove --yes");
        return Ok(());
    }
    let backup = PathBuf::from(format!("/var/lib/r9t/legacy-backup-{}.tar", stamp()));
    println!("1. Eski birimler kapatılıyor");
    let _ = Command::new("systemctl").args(["disable", "--now", "slayer-r9t.service"]).status();
    let _ = Command::new("systemctl").args(["--global", "disable", "slayer-r9t-session.service"]).status();
    println!("2. Yedek: {}", backup.display());
    archive(root, &found, &backup)?;
    println!("3. Siliniyor");
    remove(&found)?;
    let _ = Command::new("systemctl").arg("daemon-reload").status();
    let _ = Command::new("update-desktop-database").args(["-q", "/usr/share/applications"]).status();
    std::fs::write(REMOVED, format!("{} {}\n", stamp(), backup.display())).map_err(|e| format!("{REMOVED}: {e}"))?;
    println!("Eski sistem kaldırıldı. Kullanıcı tarafı: r9tctl desktop --remove-legacy --yes");
    println!("Geri yükleme: sudo tar -xpf {} -C / && sudo rm {REMOVED} && sudo systemctl daemon-reload", backup.display());
    Ok(())
}

pub fn run(args: &[String]) -> ExitCode {
    match system(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// `r9tctl desktop --remove-legacy [--yes]` (as the user).
pub fn home(yes: bool) -> Result<(), String> {
    if !Path::new(REMOVED).exists() {
        return Err("önce sistem tarafı: sudo r9tctl legacy-remove --yes".into());
    }
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME yok")?);
    let found = plan(&home, &HOME)?;
    if found.is_empty() {
        println!("Ev dizininde eski sistem dosyası yok.");
        return Ok(());
    }
    println!("Kaldırılacak (önce yedeklenir; ayarlar ~/.config/slayer-r9t-control-center kalır):");
    print_plan(&found);
    if !yes {
        println!("Yalnız plan. Uygulamak için: r9tctl desktop --remove-legacy --yes");
        return Ok(());
    }
    let dir = home.join(".local/state/r9t");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let backup = dir.join(format!("legacy-home-{}.tar", stamp()));
    archive(&home, &found, &backup)?;
    remove(&found)?;
    let _ = Command::new("update-desktop-database").args(["-q"]).arg(home.join(".local/share/applications")).status();
    println!("Kaldırıldı. Yedek: {}", backup.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_only_existing_paths_and_refuses_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("usr/local/lib/slayer-r9t/slayer_r9t")).unwrap();
        std::fs::write(root.join("usr/local/lib/slayer-r9t/r9t-service.py"), "x").unwrap();
        std::fs::create_dir_all(root.join("etc/systemd/system")).unwrap();
        std::fs::write(root.join("etc/systemd/system/slayer-r9t.service"), "[Unit]").unwrap();
        let found = plan(root, &SYSTEM).unwrap();
        assert_eq!(found, vec![root.join("usr/local/lib/slayer-r9t"), root.join("etc/systemd/system/slayer-r9t.service")]);

        // A parent replaced by a link to somewhere else is refused, not followed.
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(elsewhere.path().join("slayer-r9t")).unwrap();
        std::fs::create_dir_all(root.join("var")).unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), root.join("var/lib")).unwrap();
        assert!(plan(root, &SYSTEM).unwrap_err().contains("sembolik bağ"));
    }

    #[test]
    fn archives_then_removes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".local/lib/slayer-r9t/x")).unwrap();
        std::fs::write(root.join(".local/lib/slayer-r9t/x/a.py"), "a").unwrap();
        std::fs::create_dir_all(root.join(".local/share/applications")).unwrap();
        std::fs::write(root.join(".local/share/applications/slayer-r9t-control-center.desktop"), "d").unwrap();
        let found = plan(root, &HOME).unwrap();
        assert_eq!(found.len(), 2);
        let out = root.join("b.tar");
        archive(root, &found, &out).unwrap();
        remove(&found).unwrap();
        assert!(plan(root, &HOME).unwrap().is_empty());
        let list = String::from_utf8(Command::new("tar").arg("-tf").arg(&out).output().unwrap().stdout).unwrap();
        assert!(list.contains(".local/lib/slayer-r9t/x/a.py") && list.contains("slayer-r9t-control-center.desktop"));
    }

    #[test]
    fn waits_a_week_after_the_switch() {
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert!(!old_enough(t0, t0 + Duration::from_secs(6 * 86400)));
        assert!(old_enough(t0, t0 + MIN_AGE));
        assert!(!old_enough(t0 + Duration::from_secs(10), t0), "a commit in the future is not old");
    }
}
