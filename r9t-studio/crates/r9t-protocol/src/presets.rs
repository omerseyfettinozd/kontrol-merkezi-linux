//! Measured legacy presets (docs/05 §2.4). Values are kept exactly.

use crate::profile::{Epp, FanCurve, FanSetting, PowerProfile, ProfileSettings};

pub struct ThermalPreset {
    pub key: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

pub const THERMAL_PRESETS: [ThermalPreset; 5] = [
    ThermalPreset { key: "quiet", name: "Sessiz / Ofis", description: "Daha düşük frekanslar; ofis ve hafif işler için." },
    ThermalPreset { key: "balanced_cool", name: "Serin Dengeli", description: "CPU boost kapalı; GPU üst frekansı sınırlı." },
    ThermalPreset {
        key: "sweet_spot",
        name: "Sweet Spot · Ölçülmüş dengeli performans",
        description: "5 kısa yük döngüsü: GPU tepe 84→79 °C, CPU tepe 80,5 °C; CPU+GPU gücü ~%9,7 düşük. \
                      CS2 uzun oyunda CPU 90–91 °C çıktı; uzun oyun için 'Oyun - CPU Serin 3500' önerilir.",
    },
    ThermalPreset { key: "gaming_cool", name: "Serin Oyun", description: "CPU boost açık, üst frekans sınırlı; FPS etkisi oyuna göre değişir." },
    ThermalPreset { key: "stock", name: "Sınırları kaldır", description: "Frekans sınırları otomatik, CPU boost açık, güç profili dengeli." },
];

/// Settings applied by a thermal preset.
pub fn thermal_preset(key: &str) -> Option<ProfileSettings> {
    let base = |power, boost, cpu, gpu| ProfileSettings {
        power_profile: Some(power),
        boost_enabled: Some(boost),
        cpu_max_mhz: Some(cpu),
        gpu_max_mhz: Some(gpu),
        ..Default::default()
    };
    Some(match key {
        "quiet" => base(PowerProfile::PowerSaver, false, 1800, 1200),
        "balanced_cool" => base(PowerProfile::Balanced, false, 2400, 1800),
        "sweet_spot" => ProfileSettings {
            cpu_epp: Some(Epp::BalancePerformance),
            fan: Some(FanSetting::Preset { preset: "cool".into() }),
            ..base(PowerProfile::Balanced, true, 4900, 2475)
        },
        "gaming_cool" => base(PowerProfile::Balanced, true, 3500, 2100),
        "stock" => base(PowerProfile::Balanced, true, 0, 0),
        _ => return None,
    })
}

pub enum FanPreset {
    Curve(FanCurve),
    Boost,
}

pub const FAN_PRESET_KEYS: [&str; 3] = ["balanced", "cool", "maximum"];

pub fn fan_preset(key: &str) -> Option<FanPreset> {
    Some(match key {
        "balanced" => FanPreset::Curve(FanCurve::linear(&[[45, 50], [60, 60], [75, 80], [85, 100]])),
        "cool" => FanPreset::Curve(FanCurve::linear(&[[45, 60], [60, 75], [75, 90], [85, 100]])),
        "maximum" => FanPreset::Boost,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_is_valid() {
        for p in &THERMAL_PRESETS {
            thermal_preset(p.key).unwrap().validate().unwrap();
        }
        for k in FAN_PRESET_KEYS {
            if let FanPreset::Curve(c) = fan_preset(k).unwrap() {
                c.validate().unwrap();
            }
        }
        assert!(thermal_preset("extreme").is_none());
    }

    #[test]
    fn sweet_spot_values() {
        let s = thermal_preset("sweet_spot").unwrap();
        assert_eq!((s.cpu_max_mhz, s.gpu_max_mhz), (Some(4900), Some(2475)));
        assert_eq!(s.cpu_epp, Some(Epp::BalancePerformance));
    }
}
