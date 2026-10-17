//! Profiles + automation document (legacy schema 3 compatible, docs/05 §2.8).

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::profile::{ProfileSettings, ValidationError};

pub const MAX_PROFILES: usize = 100;
pub const MAX_APP_RULES: usize = 32;
pub const MAX_DOCUMENT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppRule {
    /// Absolute executable path (`/proc/<pid>/exe`) or a window-class glob prefixed `class:`.
    pub executable: String,
    pub profile: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Automation {
    pub startup: Option<String>,
    pub ac: Option<String>,
    pub battery: Option<String>,
    pub apps: Vec<AppRule>,
    pub low_hz_on_battery: bool,
    /// Keyboard light idle timeout in seconds, 0 = off.
    pub lighting_timeout: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyDocument {
    pub schema: u32,
    pub profiles: BTreeMap<String, ProfileSettings>,
    #[serde(default)]
    pub automation: Automation,
}

impl Default for PolicyDocument {
    fn default() -> Self {
        Self { schema: 3, profiles: BTreeMap::new(), automation: Automation::default() }
    }
}

fn invalid<T>(m: impl Into<String>) -> Result<T, ValidationError> {
    Err(ValidationError(m.into()))
}

impl PolicyDocument {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema != 3 {
            return invalid("Politika şeması 3 olmalı.");
        }
        if self.profiles.len() > MAX_PROFILES {
            return invalid("En fazla 100 profil desteklenir.");
        }
        for (name, settings) in &self.profiles {
            let len = name.chars().count();
            if name.trim().is_empty() || len > 80 {
                return invalid("Profil adı 1–80 karakter olmalı.");
            }
            settings.validate().map_err(|e| ValidationError(format!("{name}: {e}")))?;
        }
        let a = &self.automation;
        for p in [&a.startup, &a.ac, &a.battery].into_iter().flatten() {
            if !self.profiles.contains_key(p) {
                return invalid("Otomasyon profili bulunamadı.");
            }
        }
        if a.lighting_timeout > 3600 {
            return invalid("Işık zaman aşımı 0–3600 saniye olmalı.");
        }
        if a.apps.len() > MAX_APP_RULES {
            return invalid("En fazla 32 uygulama kuralı.");
        }
        let mut seen = HashSet::new();
        for rule in &a.apps {
            let e = &rule.executable;
            let well_formed = e.len() <= 4096
                && !e.contains('\0')
                && (e.starts_with('/') || e.strip_prefix("class:").is_some_and(|c| !c.is_empty()));
            if !well_formed || !seen.insert(e.as_str()) {
                return invalid("Tekil tam uygulama yolu ya da class: kuralı gerekli.");
            }
            if !self.profiles.contains_key(&rule.profile) {
                return invalid("Uygulama profili bulunamadı.");
            }
        }
        if serde_json::to_vec(self).map(|v| v.len()).unwrap_or(usize::MAX) > MAX_DOCUMENT_BYTES {
            return invalid("Profil ve kural belgesi en fazla 64 KiB olmalı.");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY: &str = r#"{"schema":3,"profiles":{
        "Oyun - CPU Serin 3500":{"power_profile":"balanced","boost_enabled":true,"cpu_max_mhz":3500,
            "gpu_max_mhz":2475,"cpu_epp":"balance_performance","fan":{"mode":"preset","preset":"cool"}}},
        "automation":{"startup":null,"ac":null,"battery":null,"apps":[],"low_hz_on_battery":false,"lighting_timeout":0}}"#;

    #[test]
    fn legacy_policy_parses_and_validates() {
        let d: PolicyDocument = serde_json::from_str(LEGACY).unwrap();
        d.validate().unwrap();
    }

    #[test]
    fn referential_and_rule_checks() {
        let mut d: PolicyDocument = serde_json::from_str(LEGACY).unwrap();
        d.automation.ac = Some("yok".into());
        assert!(d.validate().is_err());
        d.automation.ac = None;
        let rule = |e: &str| AppRule { executable: e.into(), profile: "Oyun - CPU Serin 3500".into() };
        d.automation.apps = vec![rule("/usr/bin/game"), rule("class:steam_app_*")];
        d.validate().unwrap();
        d.automation.apps.push(rule("/usr/bin/game"));
        assert!(d.validate().is_err(), "duplicate");
        d.automation.apps = vec![rule("game")];
        assert!(d.validate().is_err(), "relative");
        d.automation.apps = vec![rule("class:")];
        assert!(d.validate().is_err());
    }
}
