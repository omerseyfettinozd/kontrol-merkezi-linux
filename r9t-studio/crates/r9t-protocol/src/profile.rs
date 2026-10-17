//! Profiles and fan curves. Validation rules are ported from the legacy
//! `hardware.validate_profile` / `fans.validate_curve` (docs/05 §2.8).

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Lowest duty the v1/0.2.0 kernel bridge accepts (safety invariant 5).
pub const DUTY_FLOOR: u8 = 50;
pub const RGB_CHANNELS: usize = 126;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("{0}")]
pub struct ValidationError(pub String);

fn invalid<T>(msg: impl Into<String>) -> Result<T, ValidationError> {
    Err(ValidationError(msg.into()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PowerProfile {
    PowerSaver,
    Balanced,
    Performance,
}

impl PowerProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PowerSaver => "power-saver",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Epp {
    Performance,
    BalancePerformance,
    BalancePower,
    Power,
}

impl Epp {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Performance => "performance",
            Self::BalancePerformance => "balance_performance",
            Self::BalancePower => "balance_power",
            Self::Power => "power",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "performance" => Self::Performance,
            "balance_performance" => Self::BalancePerformance,
            "balance_power" => Self::BalancePower,
            "power" => Self::Power,
            _ => return None,
        })
    }
}

/// Curve interpolation between points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    /// Legacy behaviour: straight segments.
    #[default]
    Linear,
    /// Fritsch–Carlson monotone cubic; never overshoots or decreases.
    MonotoneCubic,
}

/// `(temperature °C, duty %)` points; see [`FanCurve::validate`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FanCurve {
    pub points: Vec<[u8; 2]>,
    #[serde(default)]
    pub interpolation: Interpolation,
}

impl FanCurve {
    pub fn linear(points: &[[u8; 2]]) -> Self {
        Self { points: points.to_vec(), interpolation: Interpolation::Linear }
    }

    /// 2–10 points; 40–95 °C; duty 50–100 %; strictly rising temperature,
    /// non-decreasing duty; must reach 100 % at or below 90 °C.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let p = &self.points;
        if !(2..=10).contains(&p.len()) {
            return invalid("Eğri 2–10 nokta içermeli.");
        }
        let mut prev: Option<[u8; 2]> = None;
        for &[t, d] in p {
            if !(40..=95).contains(&t) || !(DUTY_FLOOR..=100).contains(&d) {
                return invalid("Eğride sıcaklık 40–95 °C, fan %50–100 olmalı.");
            }
            if let Some([pt, pd]) = prev {
                if t <= pt || d < pd {
                    return invalid("Sıcaklıklar artmalı, fan yüzdesi azalmamalı.");
                }
            }
            prev = Some([t, d]);
        }
        let [t_last, d_last] = *p.last().expect("len checked");
        if d_last != 100 || t_last > 90 {
            return invalid("Eğri en geç 90 °C'de %100 fanla bitmeli.");
        }
        Ok(())
    }

    /// Duty for `temp`, clamped to the first/last point. Assumes a valid curve.
    pub fn duty_at(&self, temp: f32) -> u8 {
        let p = &self.points;
        let first = p[0];
        let last = p[p.len() - 1];
        if temp <= first[0] as f32 {
            return first[1];
        }
        if temp >= last[0] as f32 {
            return 100;
        }
        let duty = match self.interpolation {
            Interpolation::Linear => linear(p, temp),
            Interpolation::MonotoneCubic => monotone_cubic(p, temp),
        };
        duty.round().clamp(DUTY_FLOOR as f32, 100.0) as u8
    }
}

fn segment(p: &[[u8; 2]], temp: f32) -> usize {
    p.windows(2).position(|w| temp <= w[1][0] as f32).unwrap_or(p.len() - 2)
}

fn linear(p: &[[u8; 2]], temp: f32) -> f32 {
    let i = segment(p, temp);
    let (t0, d0) = (p[i][0] as f32, p[i][1] as f32);
    let (t1, d1) = (p[i + 1][0] as f32, p[i + 1][1] as f32);
    d0 + (d1 - d0) * (temp - t0) / (t1 - t0)
}

fn monotone_cubic(p: &[[u8; 2]], temp: f32) -> f32 {
    let n = p.len();
    let x: Vec<f32> = p.iter().map(|q| q[0] as f32).collect();
    let y: Vec<f32> = p.iter().map(|q| q[1] as f32).collect();
    let delta: Vec<f32> = (0..n - 1).map(|i| (y[i + 1] - y[i]) / (x[i + 1] - x[i])).collect();
    let mut m = vec![0.0f32; n];
    m[0] = delta[0];
    m[n - 1] = delta[n - 2];
    for i in 1..n - 1 {
        m[i] = if delta[i - 1] * delta[i] <= 0.0 { 0.0 } else { (delta[i - 1] + delta[i]) / 2.0 };
    }
    for i in 0..n - 1 {
        if delta[i] == 0.0 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
            continue;
        }
        let a = m[i] / delta[i];
        let b = m[i + 1] / delta[i];
        let s = a * a + b * b;
        if s > 9.0 {
            let tau = 3.0 / s.sqrt();
            m[i] = tau * a * delta[i];
            m[i + 1] = tau * b * delta[i];
        }
    }
    let i = segment(p, temp);
    let h = x[i + 1] - x[i];
    let t = (temp - x[i]) / h;
    let (t2, t3) = (t * t, t * t * t);
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    h00 * y[i] + h10 * h * m[i] + h01 * y[i + 1] + h11 * h * m[i + 1]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum FanSetting {
    Auto,
    Boost,
    Manual { cpu: u8, gpu: u8 },
    Curve { points: Vec<[u8; 2]>, #[serde(default)] interpolation: Interpolation },
    Preset { preset: String },
}

impl FanSetting {
    pub fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Manual { cpu, gpu } => {
                if ![cpu, gpu].iter().all(|v| (DUTY_FLOOR..=100).contains(*v)) {
                    return invalid("Fan hedefleri %50–100 olmalı.");
                }
                Ok(())
            }
            Self::Curve { points, interpolation } => {
                FanCurve { points: points.clone(), interpolation: *interpolation }.validate()
            }
            Self::Preset { preset } => match crate::presets::fan_preset(preset) {
                Some(_) => Ok(()),
                None => invalid("Bilinmeyen fan profili."),
            },
            Self::Auto | Self::Boost => Ok(()),
        }
    }
}

/// One saved profile; every field optional, only set fields are applied.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_profile: Option<PowerProfile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boost_enabled: Option<bool>,
    /// 0 = automatic (driver ceiling).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_max_mhz: Option<u32>,
    /// 0 = reset locked clocks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_max_mhz: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_epp: Option<Epp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rgb: Option<[u8; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rgb_map: Option<Vec<[u8; 3]>>,
    /// Percent 0–100, required together with `rgb` or `rgb_map`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fan: Option<FanSetting>,
}

impl ProfileSettings {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.is_empty() {
            return invalid("Profil alanları geçersiz.");
        }
        if self.cpu_max_mhz.is_some_and(|v| v > 6000) || self.gpu_max_mhz.is_some_and(|v| v > 4000) {
            return invalid("Frekans sınırı geçersiz.");
        }
        if self.rgb.is_some() && self.rgb_map.is_some() {
            return invalid("Tek renk ve renk haritası aynı profilde olamaz.");
        }
        if (self.rgb.is_some() || self.rgb_map.is_some()) != self.brightness.is_some() {
            return invalid("Renk ve parlaklık birlikte kaydedilmeli.");
        }
        if self.brightness.is_some_and(|b| b > 100) {
            return invalid("Profil parlaklığı geçersiz.");
        }
        if self.rgb_map.as_ref().is_some_and(|m| m.len() != RGB_CHANNELS) {
            return invalid("Klavye haritası 126 adet RGB üçlüsü olmalı.");
        }
        if let Some(fan) = &self.fan {
            fan.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cool() -> FanCurve {
        FanCurve::linear(&[[45, 60], [60, 75], [75, 90], [85, 100]])
    }

    #[test]
    fn legacy_curve_rules() {
        assert!(cool().validate().is_ok());
        assert!(FanCurve::linear(&[[45, 50]]).validate().is_err());
        assert!(FanCurve::linear(&[[45, 49], [80, 100]]).validate().is_err());
        assert!(FanCurve::linear(&[[45, 60], [45, 100]]).validate().is_err());
        assert!(FanCurve::linear(&[[45, 80], [60, 70], [80, 100]]).validate().is_err());
        assert!(FanCurve::linear(&[[45, 60], [92, 100]]).validate().is_err());
        assert!(FanCurve::linear(&[[45, 60], [85, 90]]).validate().is_err());
        assert!(FanCurve::linear(&[[39, 60], [85, 100]]).validate().is_err());
    }

    #[test]
    fn linear_matches_legacy_curve_target() {
        // Legacy: round(p0+(p1-p0)*(t-t0)/(t1-t0)), clamp to first point / 100.
        let c = cool();
        assert_eq!(c.duty_at(30.0), 60);
        assert_eq!(c.duty_at(45.0), 60);
        assert_eq!(c.duty_at(52.0), 67);
        assert_eq!(c.duty_at(80.0), 95);
        assert_eq!(c.duty_at(99.0), 100);
    }

    #[test]
    fn monotone_cubic_never_decreases_or_overshoots() {
        let mut c = FanCurve::linear(&[[40, 50], [55, 52], [60, 80], [70, 81], [85, 100]]);
        c.interpolation = Interpolation::MonotoneCubic;
        c.validate().unwrap();
        let mut prev = 0;
        for tenth in 300..1000 {
            let d = c.duty_at(tenth as f32 / 10.0);
            assert!(d >= prev, "decreased at {tenth}");
            assert!((50..=100).contains(&d));
            prev = d;
        }
        // Passes through the points.
        assert_eq!(c.duty_at(60.0), 80);
    }

    #[test]
    fn fan_setting_wire_format_is_legacy_compatible() {
        let v: FanSetting = serde_json::from_str(r#"{"mode":"preset","preset":"cool"}"#).unwrap();
        assert_eq!(v, FanSetting::Preset { preset: "cool".into() });
        let v: FanSetting = serde_json::from_str(r#"{"mode":"manual","cpu":60,"gpu":70}"#).unwrap();
        assert!(v.validate().is_ok());
        assert!(serde_json::from_str::<FanSetting>(r#"{"mode":"manual","cpu":60,"gpu":70,"x":1}"#).is_err());
        assert!(FanSetting::Manual { cpu: 40, gpu: 60 }.validate().is_err());
        assert!(FanSetting::Preset { preset: "turbo".into() }.validate().is_err());
    }

    #[test]
    fn profile_rules() {
        let ok: ProfileSettings = serde_json::from_str(
            r#"{"power_profile":"balanced","boost_enabled":true,"cpu_max_mhz":3500,"gpu_max_mhz":2475,
                "cpu_epp":"balance_performance","fan":{"mode":"preset","preset":"cool"}}"#,
        )
        .unwrap();
        assert!(ok.validate().is_ok());
        assert!(ProfileSettings::default().validate().is_err());
        let p = ProfileSettings { rgb: Some([1, 2, 3]), ..Default::default() };
        assert!(p.validate().is_err(), "colour without brightness");
        let p = ProfileSettings { rgb: Some([1, 2, 3]), brightness: Some(50), ..Default::default() };
        assert!(p.validate().is_ok());
        let p = ProfileSettings { rgb_map: Some(vec![[0; 3]; 125]), brightness: Some(5), ..Default::default() };
        assert!(p.validate().is_err());
        let p = ProfileSettings { cpu_max_mhz: Some(7000), ..Default::default() };
        assert!(p.validate().is_err());
        assert!(serde_json::from_str::<ProfileSettings>(r#"{"turbo":true}"#).is_err());
        assert!(serde_json::from_str::<ProfileSettings>(r#"{"power_profile":"quiet"}"#).is_err());
    }
}
