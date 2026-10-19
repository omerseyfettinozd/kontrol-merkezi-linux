//! R9T Studio: Tauri v2 shell. The webview never touches sockets or files; every
//! call goes through the commands below (docs/04 Faz 4, docs/08 spike decisions).

mod local;
mod rpc;
mod tray;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

use rpc::{CallError, Conn, Persistent};

/// Methods that only need r9t group membership; they use their own connection so a
/// pending polkit prompt on the control connection never blocks reads.
const READ_ONLY: &[&str] = &["hello", "status", "policy", "rgb_state", "history", "capabilities", "events"];

struct AppState {
    control: Mutex<Persistent>,
    reads: Mutex<Persistent>,
    procs: Mutex<local::ProcSampler>,
    /// Window has keyboard focus (WindowEvent::Focused); drives snapshot thinning.
    focused: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

async fn blocking<F>(f: F) -> Result<Value, Value>
where
    F: FnOnce() -> Result<Value, Value> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await.unwrap_or_else(|e| Err(json!({"code": -3, "message": e.to_string()})))
}

#[tauri::command]
async fn daemon_call(app: AppHandle, method: String, params: Option<Value>) -> Result<Value, Value> {
    blocking(move || {
        let st = app.state::<AppState>();
        let params = params.unwrap_or_else(|| json!({}));
        let conn = if READ_ONLY.contains(&method.as_str()) { &st.reads } else { &st.control };
        let r = lock(conn).call(&method, &params).map_err(|e| e.to_value());
        r
    })
    .await
}

#[tauri::command]
async fn session_call(method: String, params: Option<Value>) -> Result<Value, Value> {
    blocking(move || {
        let r = Conn::connect(&rpc::session_socket(), Duration::from_secs(15))
            .and_then(|mut c| c.call(&method, &params.unwrap_or_else(|| json!({}))));
        match &r {
            Err(e) => eprintln!("r9t-session {method}: {e:?}"),
            Ok(_) if std::env::var_os("R9T_UI_DEBUG").is_some() => eprintln!("r9t-session {method}: ok"),
            Ok(_) => {}
        }
        r.map_err(|e| e.to_value())
    })
    .await
}

#[tauri::command]
async fn processes(app: AppHandle, limit: Option<usize>) -> Result<Value, Value> {
    blocking(move || Ok(lock(&app.state::<AppState>().procs).sample(limit.unwrap_or(12)))).await
}

#[tauri::command]
fn sleep_snapshot() -> Value {
    local::sleep_snapshot()
}

#[tauri::command]
fn sleep_baseline(save: Option<Value>) -> Result<Value, String> {
    if let Some(s) = save {
        local::sleep_baseline_save(&s)?;
        return Ok(s);
    }
    Ok(local::sleep_baseline_load())
}

#[tauri::command]
fn export_file(name: String, content: String) -> Result<String, String> {
    local::export_file(&name, &content)
}

#[tauri::command]
fn keymap() -> Value {
    local::keymap()
}

/// `--view <id>`: module to open (also forwarded from a second launch).
fn view_arg(args: &[String]) -> Option<String> {
    args.iter().position(|a| a == "--view").and_then(|i| args.get(i + 1)).cloned()
}

#[tauri::command]
fn app_info() -> Value {
    let args: Vec<String> = std::env::args().collect();
    json!({
        "view": view_arg(&args),
        "version": env!("CARGO_PKG_VERSION"),
        "daemon_socket": rpc::daemon_socket(),
        "session_socket": rpc::session_socket(),
        "explicit_sync_disabled": std::env::var("__NV_DISABLE_EXPLICIT_SYNC").ok(),
        "software_rendering": std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some(),
    })
}

fn field_f64(v: &Value, path: &[&str]) -> Option<f64> {
    let mut cur = v;
    for p in path {
        cur = cur.get(p)?;
    }
    cur.get("value")?.as_f64()
}

/// Thermal alarm: user-space guard or kernel override. Always shown at once.
fn thermal_alarm(s: &Value) -> bool {
    s.pointer("/authority/guard_active").and_then(Value::as_bool).unwrap_or(false)
        || s.pointer("/fans/thermal_override/value").and_then(Value::as_bool).unwrap_or(false)
}

fn tray_state(snap: Option<&Value>) -> ([u8; 3], String) {
    let Some(s) = snap else {
        return (tray::IDLE, "R9T Studio\nr9td bağlantısı yok".into());
    };
    let profile = s.pointer("/power/profile/value").and_then(Value::as_str).unwrap_or("?");
    let alarm = thermal_alarm(s);
    let color = if alarm {
        [230, 57, 70]
    } else {
        match profile {
            "power-saver" => [63, 185, 80],
            "balanced" => [78, 161, 255],
            "performance" => [240, 136, 62],
            _ => tray::IDLE,
        }
    };
    let t = |p: &[&str]| field_f64(s, p).map(|v| format!("{v:.0} °C")).unwrap_or_else(|| "—".into());
    let active = s.pointer("/authority/active_profile").and_then(Value::as_str).map(|p| format!(" · {p}")).unwrap_or_default();
    let tip = format!("R9T Studio · {profile}{active}\nCPU {} · GPU {}{}", t(&["cpu", "ec_temp_c"]), t(&["gpu", "ec_temp_c"]), if alarm { "\nTermal koruma etkin" } else { "" });
    (color, tip)
}

/// Unfocused windows get a snapshot this often. Each `emit` costs the WebKit process
/// about as much as a repaint (docs/14: 1 Hz events alone ≈ 0.3 % CPU), and the
/// webview refills its chart history from r9td when focus returns.
const UNFOCUSED_EVERY: Duration = Duration::from_millis(4900);

/// Snapshot subscription: forwards to the webview while it is visible (every sample
/// when focused, every 5 s otherwise, at once on a thermal alarm or its change) and
/// keeps the tray up to date. Reconnects every 2 s while r9td is away.
fn subscriber(app: AppHandle, tray: Option<ksni::blocking::Handle<tray::R9tTray>>) {
    let mut last_tray = (None::<[u8; 3]>, String::new(), Instant::now() - Duration::from_secs(10));
    let mut update_tray = |snap: Option<&Value>, force: bool| {
        let (color, tip) = tray_state(snap);
        let Some(tray) = &tray else { return };
        if last_tray.0 != Some(color) {
            tray.update(|t| t.color = color);
            last_tray.0 = Some(color);
        }
        if (force || last_tray.2.elapsed() >= Duration::from_secs(2)) && last_tray.1 != tip {
            tray.update(|t| t.tip = tip.clone());
            last_tray.1 = tip;
            last_tray.2 = Instant::now();
        }
    };
    loop {
        let result = (|| -> Result<(), CallError> {
            let mut c = Conn::connect(&rpc::daemon_socket(), Duration::from_secs(10))?;
            let hello = c.call("hello", &json!({"protocol_version": 1, "client": "r9t-studio-ui"}))?;
            c.call("subscribe", &json!({"topics": ["snapshot"]}))?;
            let _ = app.emit("daemon", json!({"connected": true, "hello": hello}));
            let (mut last_emit, mut last_alarm) = (None::<Instant>, false);
            loop {
                let msg = c.read()?;
                if msg.get("method").and_then(Value::as_str) != Some("snapshot") {
                    continue;
                }
                let snap = msg.get("params").cloned().unwrap_or(Value::Null);
                update_tray(Some(&snap), false);
                let visible = app.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(false);
                let alarm = thermal_alarm(&snap);
                let due = app.state::<AppState>().focused.load(Ordering::Relaxed)
                    || alarm
                    || alarm != last_alarm
                    || last_emit.is_none_or(|t| t.elapsed() >= UNFOCUSED_EVERY);
                last_alarm = alarm;
                if visible && due {
                    let _ = app.emit("snapshot", snap);
                    last_emit = Some(Instant::now());
                }
            }
        })();
        let err = result.err().map(|e| e.to_value()).unwrap_or(Value::Null);
        let _ = app.emit("daemon", json!({"connected": false, "error": err}));
        update_tray(None, true);
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        let _ = app.emit("shown", ());
    }
}

fn tray_call(app: &AppHandle, method: &'static str, params: Value) {
    let app = app.clone();
    std::thread::spawn(move || {
        let r = lock(&app.state::<AppState>().control).call(method, &params);
        let _ = app.emit("tray-result", json!({"method": method, "ok": r.is_ok(), "error": r.err().map(|e| e.to_value())}));
    });
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // docs/08: without this WebKitGTK dies with Wayland "Error 71" on the NVIDIA driver.
    // Must happen before GTK/EGL load; a user-provided value is left alone.
    if std::env::var_os("__NV_DISABLE_EXPLICIT_SYNC").is_none() {
        std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
    }
    if args.iter().any(|a| a == "--software") {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    let hidden = args.iter().any(|a| a == "--hidden");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(v) = view_arg(&argv) {
                let _ = app.emit("navigate", v);
            }
            show_main(app)
        }))
        .manage(AppState {
            control: Mutex::new(Persistent::new(rpc::daemon_socket(), Duration::from_secs(180))),
            reads: Mutex::new(Persistent::new(rpc::daemon_socket(), Duration::from_secs(10))),
            procs: Mutex::new(local::ProcSampler::default()),
            focused: AtomicBool::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            daemon_call,
            session_call,
            processes,
            sleep_snapshot,
            sleep_baseline,
            export_file,
            keymap,
            app_info
        ])
        .setup(move |app| {
            let tray = tray::spawn(app.handle().clone());
            let handle = app.handle().clone();
            std::thread::Builder::new().name("r9t-subscribe".into()).spawn(move || subscriber(handle, tray))?;
            if !hidden {
                show_main(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing keeps the app in the tray (plan Faz 4); quit from the tray menu.
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                WindowEvent::Focused(f) => window.state::<AppState>().focused.store(*f, Ordering::Relaxed),
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("R9T Studio başlatılamadı");
}
