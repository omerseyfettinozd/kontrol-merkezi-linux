//! `r9tctl keymap-calibrate` (Faz 0.6): lights the 126 RGB channels one by one and
//! records which key each belongs to, into `data/keymap-tr-q.json` (compiled into the UI).
//!
//! Goes through r9td (`rgb_state`, `rgb_map`), so it needs control mode but not root,
//! and never races the RGB engine. The current colours and brightness are restored on
//! every exit, Ctrl-C included. Answers: key label (e.g. "Ş", "F5", "Sol Shift"),
//! empty = no LED, "g" = back, "q" = save and quit. Progress is saved after every
//! answer; `--start` resumes elsewhere (default: first uncalibrated channel).

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{json, Value};

const ROWS: usize = 6;
const COLS: usize = 21;
const N: usize = ROWS * COLS;

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: nix::libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

/// SIGINT/SIGTERM without SA_RESTART: a blocked `read_line` returns `Interrupted`, so
/// the restore path always runs.
fn trap_signals() {
    use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
    let act = SigAction::new(SigHandler::Handler(on_signal), SaFlags::empty(), SigSet::empty());
    for s in [Signal::SIGINT, Signal::SIGTERM] {
        // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
        let _ = unsafe { sigaction(s, &act) };
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    pub label: Option<String>,
    pub led: Option<bool>,
}

pub fn document(keys: &[Key]) -> Value {
    let done = keys.iter().filter(|k| k.led.is_some()).count();
    json!({
        "version": 1,
        "layout": "tr-q",
        "grid": {"rows": ROWS, "cols": COLS, "channel": "row*21+col"},
        "verified": done == N,
        "calibrated_count": done,
        "keys": keys.iter().enumerate().map(|(i, k)| json!({
            "index": i, "row": i / COLS, "col": i % COLS, "label": k.label, "led": k.led,
        })).collect::<Vec<_>>(),
    })
}

pub fn parse(doc: &Value) -> Vec<Key> {
    let mut keys = vec![Key { label: None, led: None }; N];
    for k in doc["keys"].as_array().into_iter().flatten() {
        if let Some(i) = k["index"].as_u64().map(|i| i as usize).filter(|i| *i < N) {
            keys[i] = Key { label: k["label"].as_str().map(str::to_owned), led: k["led"].as_bool() };
        }
    }
    keys
}

fn save(path: &Path, keys: &[Key]) -> std::io::Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(&document(keys)).unwrap_or_default() + "\n")
}

fn default_out() -> Option<PathBuf> {
    ["data/keymap-tr-q.json", "r9t-studio/data/keymap-tr-q.json"].iter().map(PathBuf::from).find(|p| p.exists())
}

fn frame(lit: usize) -> Vec<[u8; 3]> {
    (0..N).map(|i| if i == lit { [255, 255, 255] } else { [0, 0, 0] }).collect()
}

pub fn run(args: &[String]) -> ExitCode {
    let mut start = None;
    let mut out = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--start" => start = it.next().and_then(|v| v.parse::<usize>().ok()).filter(|v| *v < N),
            "--out" => out = it.next().map(PathBuf::from),
            other => {
                eprintln!("keymap-calibrate: bilinmeyen seçenek {other}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(out) = out.or_else(default_out) else {
        eprintln!("data/keymap-tr-q.json bulunamadı; depo kökünden çalıştırın ya da --out verin");
        return ExitCode::from(2);
    };
    let mut c = match crate::connect() {
        Ok(c) => c,
        Err(code) => return code,
    };
    let state = match crate::rpc(&mut c, "rgb_state", json!({})) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("rgb_state: {e}");
            return ExitCode::FAILURE;
        }
    };
    let saved: Vec<[u8; 3]> = serde_json::from_value(state["colors"].clone()).unwrap_or_default();
    let saved_pct = match (state["brightness"].as_f64(), state["max"].as_f64()) {
        (Some(b), Some(m)) if m > 0.0 => (b / m * 100.0).round() as u8,
        _ => 40,
    };
    if saved.len() != N {
        eprintln!("rgb_state {} renk döndürdü, {N} bekleniyor", saved.len());
        return ExitCode::FAILURE;
    }
    let mut keys = std::fs::read_to_string(&out).ok().and_then(|t| serde_json::from_str(&t).ok()).map(|d| parse(&d)).unwrap_or_else(|| vec![Key { label: None, led: None }; N]);
    let mut i = start.unwrap_or_else(|| keys.iter().position(|k| k.led.is_none()).unwrap_or(N));
    trap_signals();
    let stdin = std::io::stdin();
    let mut failed = None;
    while i < N && !STOP.load(Ordering::SeqCst) {
        if let Err(e) = crate::rpc(&mut c, "rgb_map", json!({"colors": frame(i), "brightness": 100})) {
            failed = Some(format!("rgb_map: {e}"));
            break;
        }
        print!("[{i:3}] satır {} sütun {} — yanan tuş (boş=LED yok, g=geri, q=çık): ", i / COLS, i % COLS);
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) | Err(_) => break, // EOF, or Ctrl-C (Interrupted)
            Ok(_) => {}
        }
        match line.trim() {
            "q" => break,
            "g" => i = i.saturating_sub(1),
            answer => {
                keys[i] = Key { label: (!answer.is_empty()).then(|| answer.to_owned()), led: Some(!answer.is_empty()) };
                if let Err(e) = save(&out, &keys) {
                    failed = Some(format!("{}: {e}", out.display()));
                    break;
                }
                i += 1;
            }
        }
    }
    println!();
    let restored = crate::rpc(&mut c, "rgb_map", json!({"colors": saved, "brightness": saved_pct}));
    let saved_ok = save(&out, &keys);
    match &restored {
        Ok(_) => println!("Renkler ve parlaklık geri yüklendi (%{saved_pct}); kayıt: {}", out.display()),
        Err(e) => eprintln!("UYARI: renkler geri yüklenemedi: {e}"),
    }
    let done = keys.iter().filter(|k| k.led.is_some()).count();
    println!("{done}/{N} kanal kalibre edildi{}", if done == N { "; arayüzü yeniden derleyin (harita ikiliye gömülü)" } else { "" });
    if let Some(e) = failed.or(saved_ok.err().map(|e| e.to_string())).or(restored.err()) {
        eprintln!("hata: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_round_trips_and_counts() {
        let mut keys = vec![Key { label: None, led: None }; N];
        keys[0] = Key { label: Some("Esc".into()), led: Some(true) };
        keys[1] = Key { label: None, led: Some(false) };
        let doc = document(&keys);
        assert_eq!(doc["calibrated_count"], 2);
        assert_eq!(doc["verified"], false);
        assert_eq!(doc["keys"][22]["row"], 1);
        assert_eq!(parse(&doc), keys);
    }

    #[test]
    fn repository_keymap_parses() {
        let t = include_str!("../../../data/keymap-tr-q.json");
        assert_eq!(parse(&serde_json::from_str(t).unwrap()).len(), N);
    }

    #[test]
    fn frame_lights_exactly_one_channel() {
        let f = frame(5);
        assert_eq!(f.iter().filter(|c| **c != [0, 0, 0]).count(), 1);
        assert_eq!(f[5], [255, 255, 255]);
    }
}
