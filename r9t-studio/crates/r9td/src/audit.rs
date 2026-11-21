//! `actions.jsonl` audit log: 256 KiB × 3 rotation, last 100 events kept in memory
//! for the `events` RPC.

use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const MAX_BYTES: u64 = 256 * 1024;
pub const KEEP: usize = 3;
pub const MEMORY: usize = 100;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub time_ms: u64,
    pub method: String,
    pub ok: bool,
    pub uid: Option<u32>,
    pub pid: Option<i32>,
    pub duration_ms: u64,
    pub message: String,
}

#[derive(Debug)]
pub struct Audit {
    path: Option<PathBuf>,
    recent: VecDeque<Event>,
}

impl Audit {
    /// `path = None` keeps events in memory only (tests, missing state dir).
    pub fn new(path: Option<PathBuf>) -> Self {
        let mut recent = VecDeque::new();
        if let Some(p) = &path {
            if let Ok(text) = std::fs::read_to_string(p) {
                for line in text.lines().rev().take(MEMORY) {
                    if let Ok(e) = serde_json::from_str::<Event>(line) {
                        recent.push_front(e);
                    }
                }
            }
        }
        Self { path, recent }
    }

    pub fn record(&mut self, mut event: Event) {
        // truncate panics when 600 is not a char boundary (Turkish multi-byte text):
        // back up to the nearest UTF-8 boundary.
        if event.message.len() > 600 {
            let mut end = 600;
            while !event.message.is_char_boundary(end) {
                end -= 1;
            }
            event.message.truncate(end);
        }
        if self.path.is_some() {
            // The file is root-only (0600); the journal keeps the same line readable and
            // across boots (stop/start/stall history, 2026-11-21).
            let who = event.uid.map(|u| format!(" uid={u}")).unwrap_or_default();
            eprintln!("r9td: olay {} {}{who}: {}", event.method, if event.ok { "tamam" } else { "HATA" }, event.message);
        }
        if let Some(p) = &self.path {
            if std::fs::metadata(p).is_ok_and(|m| m.len() >= MAX_BYTES) {
                for i in (1..KEEP).rev() {
                    let from = if i == 1 { p.clone() } else { p.with_extension(format!("jsonl.{}", i - 1)) };
                    let _ = std::fs::rename(from, p.with_extension(format!("jsonl.{i}")));
                }
            }
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(p) {
                if let Ok(line) = serde_json::to_string(&event) {
                    let _ = writeln!(f, "{line}");
                }
            }
        }
        self.recent.push_back(event);
        while self.recent.len() > MEMORY {
            self.recent.pop_front();
        }
    }

    pub fn latest(&self, limit: usize) -> Vec<Event> {
        self.recent.iter().rev().take(limit.min(MEMORY)).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(i: usize) -> Event {
        Event { time_ms: i as u64, method: "status".into(), ok: true, uid: Some(0), pid: None, duration_ms: 1, message: "x".repeat(1000) }
    }

    #[test]
    fn rotates_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("actions.jsonl");
        let mut a = Audit::new(Some(p.clone()));
        for i in 0..800 {
            a.record(ev(i));
        }
        assert!(std::fs::metadata(&p).unwrap().len() <= MAX_BYTES + 1024);
        assert!(p.with_extension("jsonl.1").exists());
        assert!(!p.with_extension("jsonl.3").exists());
        assert_eq!(a.latest(5)[0].time_ms, 799);
        assert_eq!(a.latest(1000).len(), MEMORY);
        assert_eq!(a.latest(1)[0].message.len(), 600);
        let b = Audit::new(Some(p));
        assert_eq!(b.latest(1)[0].time_ms, 799);
    }

    #[test]
    fn truncation_respects_utf8_boundaries() {
        let mut a = Audit::new(None);
        let mut e = ev(0);
        e.message = format!("{}{}", "a".repeat(599), "ş".repeat(10));
        a.record(e);
        assert_eq!(a.latest(1)[0].message, "a".repeat(599));
    }
}
