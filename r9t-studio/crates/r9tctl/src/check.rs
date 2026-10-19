//! `r9tctl check [--json]`: read-only health check of the installation (plan Faz 5,
//! `ileride.md` "kurulum ön kontrolü"). Device (DMI), packages, every installed
//! kernel's headers and DKMS modules (so a kernel update's autoinstall is visible),
//! loaded modules and the r9t_fan version, modules-load files, unit layout (masked,
//! linked or `/etc` overrides are reported), r9td authority, the user units and the
//! legacy leftovers. No EC access, no root needed. Exit code 1 when anything fails.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, ExitCode};

use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub level: Level,
    pub name: String,
    pub detail: String,
}

fn item(level: Level, name: impl Into<String>, detail: impl Into<String>) -> Item {
    Item { level, name: name.into(), detail: detail.into() }
}

fn out(cmd: &str, args: &[&str]) -> Option<String> {
    let o = Command::new(cmd).args(args).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_owned())
}

/// `dkms status` → (module, kernel) → (version, state).
pub fn parse_dkms(text: &str) -> BTreeMap<(String, String), (String, String)> {
    let mut m = BTreeMap::new();
    for line in text.lines() {
        // "slayer-r9t-fan/0.3.0, 7.2.9-2-cachyos, x86_64: installed (Original modules exist)"
        let Some((head, state)) = line.split_once(": ") else { continue };
        let parts: Vec<&str> = head.split(", ").collect();
        let (Some(modver), Some(kernel)) = (parts.first(), parts.get(1)) else { continue };
        let Some((module, version)) = modver.split_once('/') else { continue };
        let state = state.split_whitespace().next().unwrap_or("").to_owned();
        m.insert((module.to_owned(), (*kernel).to_owned()), (version.to_owned(), state));
    }
    m
}

/// Per installed kernel: headers and both DKMS modules `installed`.
pub fn kernel_items(kernels: &[(String, bool)], dkms: &BTreeMap<(String, String), (String, String)>, fan_version: Option<&str>) -> Vec<Item> {
    let mut items = Vec::new();
    for (k, headers) in kernels {
        let mut missing = Vec::new();
        if !headers {
            missing.push("başlıklar (build/Makefile)".to_owned());
        }
        for module in ["tuxedo-drivers", "slayer-r9t-fan"] {
            match dkms.get(&(module.to_owned(), k.clone())) {
                Some((v, s)) if s == "installed" => {
                    if module == "slayer-r9t-fan" && fan_version.is_some_and(|f| f != v) {
                        missing.push(format!("{module} {v} (paket {})", fan_version.unwrap_or("?")));
                    }
                }
                Some((v, s)) => missing.push(format!("{module} {v}: {s}")),
                None => missing.push(format!("{module} yok")),
            }
        }
        items.push(if missing.is_empty() {
            item(Level::Ok, format!("kernel {k}"), "başlıklar ve DKMS modülleri kurulu")
        } else {
            item(Level::Fail, format!("kernel {k}"), missing.join(", "))
        });
    }
    items
}

/// `systemctl show` key=value lines.
pub fn parse_show(text: &str) -> BTreeMap<String, String> {
    text.lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.to_owned(), v.to_owned())).collect()
}

/// A unit that should be enabled and active, shipped in `/usr/lib`.
pub fn wanted_unit(name: &str, show: &BTreeMap<String, String>, must_run: bool) -> Item {
    let g = |k: &str| show.get(k).map(String::as_str).unwrap_or("");
    if g("LoadState") == "masked" || g("UnitFileState").starts_with("masked") {
        return item(Level::Fail, name, "maskeli");
    }
    if g("LoadState") != "loaded" {
        return item(Level::Fail, name, format!("yüklenmedi ({})", g("LoadState")));
    }
    if g("UnitFileState") == "linked" || g("UnitFileState") == "linked-runtime" {
        return item(Level::Fail, name, "bağlı (linked) birim; paket birimi kullanılmıyor");
    }
    if g("UnitFileState") != "enabled" {
        return item(Level::Fail, name, format!("etkin değil ({})", g("UnitFileState")));
    }
    if must_run && g("ActiveState") != "active" {
        return item(Level::Fail, name, format!("çalışmıyor ({})", g("ActiveState")));
    }
    let frag = g("FragmentPath");
    if frag.starts_with("/etc/") {
        return item(Level::Warn, name, format!("/etc kopyası paket birimini gölgeliyor: {frag}"));
    }
    item(Level::Ok, name, format!("{} · {}", g("UnitFileState"), g("ActiveState")))
}

/// A unit that must not start at boot (legacy service, shadow).
pub fn unwanted_unit(name: &str, show: &BTreeMap<String, String>) -> Item {
    let g = |k: &str| show.get(k).map(String::as_str).unwrap_or("");
    match (g("LoadState"), g("UnitFileState"), g("ActiveState")) {
        ("not-found", _, _) => item(Level::Ok, name, "yok"),
        (_, "enabled", _) => item(Level::Fail, name, "açılışta etkin; r9td ile çakışır"),
        (_, _, "active") => item(Level::Fail, name, "çalışıyor; r9td ile çakışır"),
        (_, s, a) => item(Level::Ok, name, format!("{s} · {a}")),
    }
}

fn show(user: bool, unit: &str) -> BTreeMap<String, String> {
    let mut args = vec!["show", "-p", "LoadState", "-p", "UnitFileState", "-p", "ActiveState", "-p", "FragmentPath", unit];
    if user {
        args.insert(0, "--user");
    }
    parse_show(&out("systemctl", &args).unwrap_or_default())
}

fn gather() -> Vec<Item> {
    let mut v = Vec::new();
    let dmi = |f: &str| std::fs::read_to_string(format!("/sys/class/dmi/id/{f}")).unwrap_or_default().trim().to_owned();
    let (vendor, product) = (dmi("sys_vendor"), dmi("product_name"));
    v.push(if vendor == "GAME GARAJ" && product == "SLAYER R9T" {
        item(Level::Ok, "cihaz", format!("{vendor} {product}, BIOS {}", dmi("bios_version")))
    } else {
        item(Level::Fail, "cihaz", format!("beklenmeyen model: {vendor} {product}"))
    });

    let mut fan_pkg = None;
    for p in ["r9t-studio", "slayer-r9t-fan-dkms", "tuxedo-drivers-r9t-dkms"] {
        match out("pacman", &["-Q", p]) {
            Some(q) => {
                let ver = q.split_whitespace().nth(1).unwrap_or("").to_owned();
                if p == "slayer-r9t-fan-dkms" {
                    fan_pkg = ver.split('-').next().map(str::to_owned);
                }
                v.push(item(Level::Ok, format!("paket {p}"), ver));
            }
            None => v.push(item(Level::Fail, format!("paket {p}"), "kurulu değil")),
        }
    }

    // Installed kernels: /usr/lib/modules/<k>/vmlinuz owned by a package.
    let mut kernels = Vec::new();
    let mut dirs: Vec<_> = std::fs::read_dir("/usr/lib/modules").into_iter().flatten().flatten().map(|e| e.path()).collect();
    dirs.sort();
    for d in dirs {
        let k = d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let vmlinuz = d.join("vmlinuz");
        if !vmlinuz.exists() {
            continue;
        }
        if out("pacman", &["-Qoq", &vmlinuz.to_string_lossy()]).is_none() {
            v.push(item(Level::Warn, format!("kernel {k}"), "sahipsiz kalıntı (paketi kaldırılmış); DKMS denetlenmedi"));
            continue;
        }
        kernels.push((k, d.join("build/Makefile").exists()));
    }
    let dkms = parse_dkms(&out("dkms", &["status"]).unwrap_or_default());
    v.extend(kernel_items(&kernels, &dkms, fan_pkg.as_deref()));

    let fan = r9t_hal::fan::SysfsFan::new(r9t_hal::Sysfs::from_env());
    use r9t_hal::fan::FanBridge;
    let version = fan.version();
    v.push(match (fan.loaded(), r9t_hal::fan::supported_version(version.as_deref())) {
        (false, _) => item(Level::Fail, "modül r9t_fan", "yüklü değil"),
        (true, false) => item(Level::Fail, "modül r9t_fan", r9t_hal::fan::version_refusal(version.as_deref())),
        (true, true) if fan_pkg.is_some() && fan_pkg != version => {
            item(Level::Warn, "modül r9t_fan", format!("yüklü {} ≠ paket {}: yeniden başlatınca geçer", version.unwrap_or_default(), fan_pkg.unwrap_or_default()))
        }
        (true, true) => item(Level::Ok, "modül r9t_fan", version.unwrap_or_default()),
    });
    v.push(if Path::new("/dev/tuxedo_io").exists() {
        item(Level::Ok, "modül tuxedo_io", "/dev/tuxedo_io")
    } else {
        item(Level::Fail, "modül tuxedo_io", "/dev/tuxedo_io yok")
    });
    for f in ["slayer-r9t-fan.conf", "tuxedo-drivers-r9t-dkms.conf"] {
        let p = format!("/usr/lib/modules-load.d/{f}");
        v.push(if Path::new(&p).exists() { item(Level::Ok, "modules-load", p) } else { item(Level::Fail, "modules-load", format!("{p} yok")) });
    }
    if Path::new("/etc/modules-load.d/slayer-r9t.conf").exists() {
        v.push(item(Level::Warn, "modules-load", "eski /etc/modules-load.d/slayer-r9t.conf duruyor (paket dosyaları yeterli)"));
    }

    v.push(wanted_unit("r9td.service", &show(false, "r9td.service"), true));
    v.push(unwanted_unit("r9td-shadow.service", &show(false, "r9td-shadow.service")));
    v.push(unwanted_unit("slayer-r9t.service", &show(false, "slayer-r9t.service")));
    let fb = show(false, "r9t-fallback.service");
    v.push(if fb.get("LoadState").map(String::as_str) == Some("loaded") {
        item(Level::Ok, "r9t-fallback.service", "yüklü (r9td OnFailure)")
    } else {
        item(Level::Fail, "r9t-fallback.service", "yok")
    });

    v.push(match crate::client::Client::connect(&crate::socket(), Some(std::time::Duration::from_secs(5))) {
        Ok(mut c) => match c.call(r9t_protocol::rpc::Call::Status(r9t_protocol::rpc::Empty {})) {
            Ok(s) if s["authority"]["state"] == "control" => item(Level::Ok, "r9td yetkisi", "control"),
            Ok(s) => item(Level::Fail, "r9td yetkisi", format!("{} — {}", s["authority"]["state"], s["authority"]["reason"])),
            Err(e) => item(Level::Fail, "r9td yetkisi", format!("{e:?}")),
        },
        Err(e) => item(Level::Warn, "r9td yetkisi", format!("sokete bağlanılamadı ({e:?}); r9t grubunda mısın?")),
    });

    if crate::switch::euid() != 0 {
        v.push(wanted_unit("r9t-session.service (kullanıcı)", &show(true, "r9t-session.service"), true));
        v.push(wanted_unit("r9t-studio-tray.service (kullanıcı)", &show(true, "r9t-studio-tray.service"), false));
        v.push(unwanted_unit("slayer-r9t-session.service (kullanıcı)", &show(true, "slayer-r9t-session.service")));
    }

    v.push(match std::fs::read_to_string(crate::legacy::REMOVED) {
        Ok(t) => item(Level::Ok, "eski sistem", format!("kaldırıldı: {}", t.trim())),
        Err(_) if Path::new("/usr/local/lib/slayer-r9t").exists() => item(Level::Ok, "eski sistem", "duruyor (yedek); 1 hafta sonra: sudo r9tctl legacy-remove"),
        Err(_) => item(Level::Ok, "eski sistem", "yok"),
    });
    v
}

pub fn run(args: &[String]) -> ExitCode {
    let items = gather();
    if args.iter().any(|a| a == "--json") {
        let doc: Vec<Value> = items
            .iter()
            .map(|i| json!({"level": format!("{:?}", i.level).to_lowercase(), "name": i.name, "detail": i.detail}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
    } else {
        for i in &items {
            let mark = match i.level {
                Level::Ok => "✓",
                Level::Warn => "!",
                Level::Fail => "✗",
            };
            println!("{mark} {:<40} {}", i.name, i.detail);
        }
    }
    let worst = items.iter().map(|i| i.level).max().unwrap_or(Level::Ok);
    if worst == Level::Fail { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DKMS: &str = "slayer-r9t-fan/0.3.0, 7.2.9-2-cachyos, x86_64: installed\n\
                        slayer-r9t-fan/0.3.0, 6.18.55-2-cachyos-lts, x86_64: built\n\
                        tuxedo-drivers/4.24.0, 7.2.9-2-cachyos, x86_64: installed (Original modules exist)\n\
                        tuxedo-drivers/4.24.0, 6.18.55-2-cachyos-lts, x86_64: installed (Original modules exist)\n";

    #[test]
    fn parses_dkms_status() {
        let m = parse_dkms(DKMS);
        assert_eq!(m[&("tuxedo-drivers".into(), "7.2.9-2-cachyos".into())], ("4.24.0".into(), "installed".into()));
        assert_eq!(m[&("slayer-r9t-fan".into(), "6.18.55-2-cachyos-lts".into())].1, "built");
        assert_eq!(m.len(), 4);
    }

    #[test]
    fn flags_kernels_missing_autoinstall_or_headers() {
        let m = parse_dkms(DKMS);
        let ks = vec![("7.2.9-2-cachyos".to_owned(), true), ("6.18.55-2-cachyos-lts".to_owned(), true), ("7.3.0-1-cachyos".to_owned(), false)];
        let items = kernel_items(&ks, &m, Some("0.3.0"));
        assert_eq!(items[0].level, Level::Ok);
        assert!(items[1].level == Level::Fail && items[1].detail.contains("built"));
        assert!(items[2].level == Level::Fail && items[2].detail.contains("başlıklar") && items[2].detail.contains("tuxedo-drivers yok"));
        let stale = kernel_items(&ks[..1], &m, Some("0.4.0"));
        assert!(stale[0].level == Level::Fail && stale[0].detail.contains("paket 0.4.0"));
    }

    #[test]
    fn judges_unit_layout() {
        let s = |t: &str| parse_show(t);
        let ok = "LoadState=loaded\nUnitFileState=enabled\nActiveState=active\nFragmentPath=/usr/lib/systemd/system/r9td.service";
        assert_eq!(wanted_unit("r9td", &s(ok), true).level, Level::Ok);
        assert_eq!(wanted_unit("r9td", &s(&ok.replace("UnitFileState=enabled", "UnitFileState=masked")), true).level, Level::Fail);
        assert_eq!(wanted_unit("r9td", &s(&ok.replace("UnitFileState=enabled", "UnitFileState=linked")), true).level, Level::Fail);
        assert_eq!(wanted_unit("r9td", &s(&ok.replace("ActiveState=active", "ActiveState=failed")), true).level, Level::Fail);
        assert_eq!(wanted_unit("tray", &s(&ok.replace("ActiveState=active", "ActiveState=inactive")), false).level, Level::Ok);
        assert_eq!(wanted_unit("r9td", &s(&ok.replace("/usr/lib/", "/etc/")), true).level, Level::Warn);
        assert_eq!(unwanted_unit("slayer", &s("LoadState=not-found\nUnitFileState=\nActiveState=inactive")).level, Level::Ok);
        assert_eq!(unwanted_unit("slayer", &s("LoadState=loaded\nUnitFileState=enabled\nActiveState=inactive")).level, Level::Fail);
        assert_eq!(unwanted_unit("slayer", &s("LoadState=loaded\nUnitFileState=disabled\nActiveState=active")).level, Level::Fail);
        assert_eq!(unwanted_unit("slayer", &s("LoadState=loaded\nUnitFileState=disabled\nActiveState=inactive")).level, Level::Ok);
    }
}
