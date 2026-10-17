//! Automation decisions (port of legacy `automation.py`, docs/05 §2.9). Pure logic:
//! the daemon supplies observations and executes the returned action.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use r9t_hal::power::Source;
use r9t_protocol::policy::PolicyDocument;
use r9t_protocol::profile::ProfileSettings;

/// A power source change must be stable this long before it is acted on.
pub const SOURCE_DEBOUNCE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    None,
    /// Apply `settings` (profile `name`) because of `reason`. `snapshot_first`
    /// asks the daemon to save current settings for a later restore.
    Apply { name: String, settings: ProfileSettings, reason: String, snapshot_first: bool },
    /// The app that triggered a profile exited: restore what was there before.
    Restore { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Trigger {
    App(String),
    Ac,
    Battery,
    Startup,
}

impl Trigger {
    fn label(&self) -> String {
        match self {
            Self::App(e) => format!("app:{e}"),
            Self::Ac => "ac".into(),
            Self::Battery => "battery".into(),
            Self::Startup => "startup".into(),
        }
    }
}

#[derive(Debug)]
pub struct Automation {
    pub doc: PolicyDocument,
    pub paused: bool,
    source: Option<Source>,
    candidate: Option<Source>,
    changed_at: Option<Instant>,
    startup_done: bool,
    active: Option<(Trigger, String)>,
    applied: Option<ProfileSettings>,
    has_snapshot: bool,
}

/// What the daemon observed this tick.
pub struct Observation<'a> {
    pub source: Source,
    /// `/proc/<pid>/exe` of the desktop user's processes.
    pub executables: &'a HashSet<String>,
    /// Window classes reported by r9t-session (game detection).
    pub classes: &'a HashSet<String>,
}

fn class_matches(pattern: &str, class: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => class.starts_with(prefix),
        None => class == pattern,
    }
}

impl Automation {
    pub fn new(doc: PolicyDocument, paused: bool) -> Self {
        Self {
            doc,
            paused,
            source: None,
            candidate: None,
            changed_at: None,
            startup_done: false,
            active: None,
            applied: None,
            has_snapshot: false,
        }
    }

    pub fn configure(&mut self, doc: PolicyDocument) {
        if doc.automation != self.doc.automation {
            self.startup_done = false;
        }
        self.doc = doc;
    }

    /// A manual user action pauses automation (legacy behaviour, now explicit).
    pub fn pause(&mut self) {
        self.paused = true;
        self.active = None;
        self.has_snapshot = false;
    }

    pub fn resume(&mut self) {
        self.paused = false;
        self.active = None;
        self.startup_done = false;
    }

    pub fn active_label(&self) -> Option<String> {
        self.active.as_ref().map(|(t, p)| format!("{} → {p}", t.label()))
    }

    fn decide(&self, obs: &Observation) -> Option<(Trigger, String)> {
        let rules = &self.doc.automation;
        for rule in &rules.apps {
            let hit = match rule.executable.strip_prefix("class:") {
                Some(pattern) => obs.classes.iter().any(|c| class_matches(pattern, c)),
                None => obs.executables.contains(&rule.executable),
            };
            if hit {
                return Some((Trigger::App(rule.executable.clone()), rule.profile.clone()));
            }
        }
        match self.source {
            Some(Source::Ac) if rules.ac.is_some() => return Some((Trigger::Ac, rules.ac.clone().unwrap())),
            Some(Source::Battery) if rules.battery.is_some() => return Some((Trigger::Battery, rules.battery.clone().unwrap())),
            _ => {}
        }
        if !self.startup_done {
            if let Some(p) = &rules.startup {
                return Some((Trigger::Startup, p.clone()));
            }
        }
        None
    }

    pub fn tick(&mut self, obs: &Observation, now: Instant) -> Action {
        if self.candidate != Some(obs.source) {
            self.candidate = Some(obs.source);
            self.changed_at = Some(now);
        }
        if self.changed_at.is_some_and(|t| now.duration_since(t) >= SOURCE_DEBOUNCE) {
            self.source = self.candidate.filter(|s| *s != Source::Unknown).or(self.source);
        }
        if self.paused {
            return Action::None;
        }
        let target = self.decide(obs);
        let settings = target.as_ref().and_then(|(_, p)| self.doc.profiles.get(p).cloned());
        if target == self.active && settings == self.applied {
            return Action::None;
        }
        match (target, settings) {
            (Some((trigger, name)), Some(settings)) => {
                let entering_app = matches!(trigger, Trigger::App(_)) && !matches!(self.active, Some((Trigger::App(_), _)));
                if !matches!(trigger, Trigger::App(_)) {
                    self.has_snapshot = false;
                }
                if entering_app {
                    self.has_snapshot = true;
                }
                if trigger == Trigger::Startup {
                    self.startup_done = true;
                }
                let reason = format!("{} → {name}", trigger.label());
                self.active = Some((trigger, name.clone()));
                self.applied = Some(settings.clone());
                Action::Apply { name, settings, reason, snapshot_first: entering_app }
            }
            _ => {
                let was_app = matches!(self.active, Some((Trigger::App(_), _)));
                let restore = was_app && self.has_snapshot;
                self.active = None;
                self.applied = None;
                self.has_snapshot = false;
                if restore { Action::Restore { reason: "Uygulama kapandı; önceki ayarlar geri yüklendi.".into() } } else { Action::None }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use r9t_protocol::policy::{AppRule, Automation as Rules};
    use r9t_protocol::profile::PowerProfile;

    fn doc() -> PolicyDocument {
        let mut d = PolicyDocument::default();
        let p = |pp| ProfileSettings { power_profile: Some(pp), ..Default::default() };
        d.profiles.insert("Oyun".into(), p(PowerProfile::Performance));
        d.profiles.insert("Pil".into(), p(PowerProfile::PowerSaver));
        d.profiles.insert("Priz".into(), p(PowerProfile::Balanced));
        d.automation = Rules {
            startup: Some("Priz".into()),
            ac: Some("Priz".into()),
            battery: Some("Pil".into()),
            apps: vec![
                AppRule { executable: "/usr/bin/game".into(), profile: "Oyun".into() },
                AppRule { executable: "class:steam_app_*".into(), profile: "Oyun".into() },
            ],
            ..Default::default()
        };
        d.validate().unwrap();
        d
    }

    fn name(a: &Action) -> Option<&str> {
        match a {
            Action::Apply { name, .. } => Some(name),
            _ => None,
        }
    }

    #[test]
    fn startup_then_debounced_power_source() {
        let t0 = Instant::now();
        let s = |n| t0 + Duration::from_secs(n);
        let none = HashSet::new();
        let mut a = Automation::new(doc(), false);
        let obs = |src| Observation { source: src, executables: &none, classes: &none };
        assert_eq!(name(&a.tick(&obs(Source::Battery), s(0))), Some("Priz"), "startup rule first");
        assert_eq!(a.tick(&obs(Source::Battery), s(2)), Action::None, "debounce");
        assert_eq!(name(&a.tick(&obs(Source::Battery), s(3))), Some("Pil"));
        assert_eq!(a.tick(&obs(Source::Ac), s(4)), Action::None);
        assert_eq!(name(&a.tick(&obs(Source::Ac), s(7))), Some("Priz"));
        // Unknown never switches to battery.
        a.tick(&obs(Source::Unknown), s(8));
        assert_eq!(a.tick(&obs(Source::Unknown), s(20)), Action::None);
    }

    #[test]
    fn app_rule_snapshots_and_restores() {
        let t0 = Instant::now();
        let s = |n| t0 + Duration::from_secs(n);
        let none = HashSet::new();
        let mut d = doc();
        d.automation.startup = None;
        d.automation.ac = None;
        let mut a = Automation::new(d, false);
        let game: HashSet<String> = ["/usr/bin/game".to_string()].into();
        let r = a.tick(&Observation { source: Source::Ac, executables: &game, classes: &none }, s(0));
        assert!(matches!(r, Action::Apply { snapshot_first: true, .. }));
        assert_eq!(a.tick(&Observation { source: Source::Ac, executables: &game, classes: &none }, s(1)), Action::None);
        let r = a.tick(&Observation { source: Source::Ac, executables: &none, classes: &none }, s(2));
        assert!(matches!(r, Action::Restore { .. }));
    }

    #[test]
    fn window_class_glob_and_pause() {
        let t0 = Instant::now();
        let none = HashSet::new();
        let classes: HashSet<String> = ["steam_app_730".to_string()].into();
        let mut a = Automation::new(doc(), true);
        let obs = Observation { source: Source::Ac, executables: &none, classes: &classes };
        assert_eq!(a.tick(&obs, t0), Action::None, "paused");
        a.resume();
        assert_eq!(name(&a.tick(&obs, t0 + Duration::from_secs(1))), Some("Oyun"));
        assert_eq!(a.active_label().as_deref(), Some("app:class:steam_app_* → Oyun"));
        assert!(class_matches("steam_app_*", "steam_app_1") && !class_matches("steam_app_*", "steam"));
    }
}
