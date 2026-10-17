//! 300 s ring buffer, persisted on stop and restored on start when still recent.

use std::collections::VecDeque;
use std::path::Path;

use r9t_protocol::snapshot::HistoryPoint;
use serde::{Deserialize, Serialize};

pub const WINDOW_MS: u64 = 300_000;

#[derive(Debug, Default)]
pub struct History {
    points: VecDeque<HistoryPoint>,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    saved_unix_ms: u64,
    points: Vec<HistoryPoint>,
}

impl History {
    pub fn push(&mut self, p: HistoryPoint) {
        self.points.push_back(p);
        while self.points.front().is_some_and(|f| p.t_ms.saturating_sub(f.t_ms) > WINDOW_MS) {
            self.points.pop_front();
        }
    }

    /// Points newer than `now_ms - seconds`.
    pub fn since(&self, now_ms: u64, seconds: u32) -> Vec<HistoryPoint> {
        let from = now_ms.saturating_sub(u64::from(seconds.min(300)) * 1000);
        self.points.iter().filter(|p| p.t_ms >= from).copied().collect()
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn save(&self, path: &Path, now_ms: u64) -> std::io::Result<()> {
        let saved = Saved { saved_unix_ms: now_ms, points: self.points.iter().copied().collect() };
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec(&saved).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, path)
    }

    /// Restores points from a file written less than 300 s ago; anything else is ignored.
    pub fn load(path: &Path, now_ms: u64) -> Self {
        let mut h = Self::default();
        let Ok(bytes) = std::fs::read(path) else { return h };
        let Ok(saved) = serde_json::from_slice::<Saved>(&bytes) else { return h };
        if now_ms.saturating_sub(saved.saved_unix_ms) > WINDOW_MS {
            return h;
        }
        for p in saved.points {
            if now_ms.saturating_sub(p.t_ms) <= WINDOW_MS && p.t_ms <= now_ms {
                h.push(p);
            }
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(t_ms: u64) -> HistoryPoint {
        HistoryPoint { t_ms, cpu_c: Some(60.0), gpu_c: None, cpu_w: None, gpu_w: None, cpu_rpm: None, gpu_rpm: None, battery_w: None }
    }

    #[test]
    fn window_and_query() {
        let mut h = History::default();
        for s in 0..400u64 {
            h.push(pt(s * 1000));
        }
        assert_eq!(h.len(), 301);
        assert_eq!(h.since(399_000, 10).len(), 11);
        assert_eq!(h.since(399_000, 9999).len(), 301, "capped at 300 s");
    }

    #[test]
    fn persistence_only_when_recent() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("history.bin");
        let mut h = History::default();
        for s in 0..10u64 {
            h.push(pt(1_000_000 + s * 1000));
        }
        h.save(&p, 1_010_000).unwrap();
        assert_eq!(History::load(&p, 1_100_000).len(), 10);
        assert_eq!(History::load(&p, 1_400_000).len(), 0, "older than 300 s");
        std::fs::write(&p, b"garbage").unwrap();
        assert!(History::load(&p, 1_010_000).is_empty());
    }
}
