// Faz 0.4 atılabilir deneme. Ölçüm için ön yüz FPS'i ve kare süresi dağılımını
// stdout'a yazar; SPIKE_ANIMATE=0 animasyonu kapatır (boşta ölçümü).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::time::Instant;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, WindowEvent,
};

struct Started(Instant);

#[tauri::command]
fn spike_config() -> serde_json::Value {
    let animate = std::env::var("SPIKE_ANIMATE").map(|v| v != "0").unwrap_or(true);
    serde_json::json!({
        "animate": animate,
        "dmabuf_disabled": std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_ok(),
        "compositing_disabled": std::env::var("WEBKIT_DISABLE_COMPOSITING_MODE").is_ok(),
    })
}

#[tauri::command]
fn report(state: tauri::State<Started>, fps: f64, p50_ms: f64, p99_ms: f64, long_frames: u32) {
    println!(
        "SPIKE t={:.1}s fps={:.1} p50_ms={:.2} p99_ms={:.2} long_frames={}",
        state.0.elapsed().as_secs_f64(),
        fps,
        p50_ms,
        p99_ms,
        long_frames
    );
}

fn main() {
    tauri::Builder::default()
        .manage(Started(Instant::now()))
        .invoke_handler(tauri::generate_handler![spike_config, report])
        .setup(|app| {
            let show = MenuItem::with_id(app, "show", "Göster", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Çık", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            TrayIconBuilder::with_id("r9t-spike")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("R9T spike · CPU 52 °C · GPU 48 °C")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            println!("SPIKE ready pid={}", std::process::id());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Pencere kapanınca uygulama tepside kalır (plan Faz 4).
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("tauri çalıştırılamadı");
}
