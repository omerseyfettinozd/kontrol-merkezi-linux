//! StatusNotifierItem tray (ksni). Tauri's tray backend cannot do this on Linux:
//! tray-icon 0.25 makes `set_tooltip` a no-op there, and plan Faz 4 wants the
//! temperatures in the tooltip. ksni speaks SNI over D-Bus directly (Waybar, KDE).

use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::StandardItem;
use ksni::{Category, Icon, MenuItem, ToolTip};
use serde_json::json;
use tauri::AppHandle;

/// Grey until the first snapshot arrives.
pub const IDLE: [u8; 3] = [110, 118, 129];

pub struct R9tTray {
    app: AppHandle,
    pub color: [u8; 3],
    /// First line is the tooltip title, the rest its description.
    pub tip: String,
}

/// Solid disc, ARGB32 in network byte order; the colour encodes the power profile
/// or a thermal alarm, the white centre keeps it visible on any bar.
pub fn disc(rgb: [u8; 3]) -> Icon {
    const N: i32 = 32;
    let mut data = Vec::with_capacity((N * N * 4) as usize);
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            let r = (dx * dx + dy * dy).sqrt();
            let a = ((14.5 - r).clamp(0.0, 1.0) * 255.0) as u8;
            let c = if r < 5.0 { [245, 245, 245] } else { rgb };
            data.extend_from_slice(&[a, c[0], c[1], c[2]]);
        }
    }
    Icon { width: N, height: N, data }
}

fn split_tip(tip: &str) -> (String, String) {
    let (title, rest) = tip.split_once('\n').unwrap_or((tip, ""));
    (title.to_owned(), rest.to_owned())
}

impl ksni::Tray for R9tTray {
    fn id(&self) -> String {
        "r9t-studio".into()
    }
    fn title(&self) -> String {
        "R9T Studio".into()
    }
    fn category(&self) -> Category {
        Category::Hardware
    }
    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![disc(self.color)]
    }
    fn tool_tip(&self) -> ToolTip {
        let (title, description) = split_tip(&self.tip);
        ToolTip { title, description, ..Default::default() }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        crate::show_main(&self.app);
    }
    fn menu(&self) -> Vec<MenuItem<Self>> {
        fn item(label: &str, f: impl Fn(&AppHandle) + Send + 'static) -> MenuItem<R9tTray> {
            StandardItem { label: label.into(), activate: Box::new(move |t: &mut R9tTray| f(&t.app)), ..Default::default() }.into()
        }
        let scenario = |label: &str, name: &'static str| item(label, move |app| crate::tray_call(app, "scenario", json!({"name": name})));
        vec![
            item("R9T Studio'yu göster", crate::show_main),
            MenuItem::Separator,
            scenario("Senaryo: Sessiz", "quiet"),
            scenario("Senaryo: Dengeli", "balanced"),
            scenario("Senaryo: Oyun", "game"),
            scenario("Senaryo: Turbo", "turbo"),
            item("Fanlar otomatik", |app| crate::tray_call(app, "auto", json!({}))),
            MenuItem::Separator,
            item("Çık", |app| app.exit(0)),
        ]
    }
}

/// Registers the tray. Waits for a StatusNotifierWatcher that is not up yet (login
/// autostart before Waybar); `None` only if the session bus itself is unusable.
pub fn spawn(app: AppHandle) -> Option<Handle<R9tTray>> {
    let tray = R9tTray { app, color: IDLE, tip: "R9T Studio\nr9td bağlantısı bekleniyor".into() };
    match tray.assume_sni_available(true).spawn() {
        Ok(h) => Some(h),
        Err(e) => {
            eprintln!("tepsi kaydedilemedi: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disc_is_argb_with_white_centre() {
        let icon = disc([63, 185, 80]);
        assert_eq!(icon.data.len(), 32 * 32 * 4);
        let px = |x: usize, y: usize| &icon.data[(y * 32 + x) * 4..][..4];
        assert_eq!(px(16, 16), &[255, 245, 245, 245]);
        assert_eq!(px(16, 4), &[255, 63, 185, 80]);
        assert_eq!(px(0, 0)[0], 0, "corner is transparent");
    }

    #[test]
    fn tip_splits_into_title_and_description() {
        assert_eq!(split_tip("R9T Studio · balanced\nCPU 60 °C · GPU 55 °C"), ("R9T Studio · balanced".into(), "CPU 60 °C · GPU 55 °C".into()));
        assert_eq!(split_tip("yalnız başlık"), ("yalnız başlık".into(), String::new()));
    }
}
