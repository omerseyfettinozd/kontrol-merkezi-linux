//! Measured legacy presets (docs/05 §2.4). Values are kept exactly.

use crate::profile::{Epp, FanCurve, FanSetting, PowerProfile, ProfileSettings};

pub struct ThermalPreset {
    pub key: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

pub const THERMAL_PRESETS: [ThermalPreset; 6] = [
    ThermalPreset { key: "quiet", name: "Sessiz / Ofis", description: "Daha düşük frekanslar, fanlar EC otomatiğinde; ofis ve hafif işler için." },
    ThermalPreset {
        key: "library",
        name: "Kütüphane",
        description: "Sessiz / Ofis frekansları + en düşük fan eğrisi. 60 Hz ve duvar kağıdı kapalıyken fan %36 (~2300 RPM), \
                      300 Hz + duvar kağıdıyla EC otomatiği %55 (~3500 RPM). Fan tabanını r9t_fan sıcaklıkla yükseltir.",
    },
    ThermalPreset { key: "balanced_cool", name: "Serin Dengeli", description: "CPU boost kapalı; GPU üst frekansı sınırlı." },
    ThermalPreset {
        key: "sweet_spot",
        name: "Sweet Spot · Ölçülmüş dengeli performans",
        description: "5 kısa yük döngüsü: GPU tepe 84→79 °C, CPU tepe 80,5 °C; CPU+GPU gücü ~%9,7 düşük. \
                      CS2 uzun oyunda CPU 90–91 °C çıktı; uzun oyun için 'Serin Oyun' önerilir.",
    },
    ThermalPreset {
        key: "gaming_cool",
        name: "Serin Oyun",
        description: "CPU boost kapalı (2400 MHz), GPU 2250 MHz, serin fan eğrisi. GPU ağırlıklı oyunlar için: \
                      Bannerlord 384 asker benchmark'ında boost açığa göre ort. 70,9→75,0 FPS, CPU 90→81 °C. \
                      CPU ağırlıklı oyunlarda (CS2) 'Sweet Spot' seçin.",
    },
    ThermalPreset { key: "stock", name: "Sınırları kaldır", description: "Frekans sınırları otomatik, CPU boost açık, güç profili dengeli." },
];

/// Settings applied by a thermal preset.
pub fn thermal_preset(key: &str) -> Option<ProfileSettings> {
    // EPP is always set: power-profiles-daemon rewrites it only when the profile
    // changes, so a preset without it kept the previous preset's EPP.
    let base = |power, boost, cpu, gpu| ProfileSettings {
        power_profile: Some(power),
        boost_enabled: Some(boost),
        cpu_max_mhz: Some(cpu),
        gpu_max_mhz: Some(gpu),
        cpu_epp: Some(if power == PowerProfile::PowerSaver { Epp::Power } else { Epp::BalancePerformance }),
        ..Default::default()
    };
    Some(match key {
        "quiet" => ProfileSettings { fan: Some(FanSetting::Auto), ..base(PowerProfile::PowerSaver, false, 1800, 1200) },
        "library" => ProfileSettings {
            fan: Some(FanSetting::Preset { preset: "quiet".into() }),
            ..base(PowerProfile::PowerSaver, false, 1800, 1200)
        },
        "balanced_cool" => base(PowerProfile::Balanced, false, 2400, 1800),
        "sweet_spot" => ProfileSettings {
            fan: Some(FanSetting::Preset { preset: "cool".into() }),
            ..base(PowerProfile::Balanced, true, 4900, 2475)
        },
        // docs/16: with the GPU the bottleneck, a cooler CPU leaves the shared budget to it.
        "gaming_cool" => ProfileSettings {
            fan: Some(FanSetting::Preset { preset: "cool".into() }),
            ..base(PowerProfile::Balanced, false, 0, 2250)
        },
        "stock" => base(PowerProfile::Balanced, true, 0, 0),
        _ => return None,
    })
}

pub enum FanPreset {
    Curve(FanCurve),
    Boost,
}

pub const FAN_PRESET_KEYS: [&str; 4] = ["quiet", "balanced", "cool", "maximum"];

pub fn fan_preset(key: &str) -> Option<FanPreset> {
    Some(match key {
        // Below ~75 °C the r9t_fan floor (25 → 50 % over CPU 60–75 °C) sets the duty.
        "quiet" => FanPreset::Curve(FanCurve::linear(&[[60, 25], [70, 30], [78, 50], [85, 100]])),
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

    #[test]
    fn every_preset_sets_epp() {
        for p in &THERMAL_PRESETS {
            assert!(thermal_preset(p.key).unwrap().cpu_epp.is_some(), "{}", p.key);
        }
        assert_eq!(thermal_preset("quiet").unwrap().cpu_epp, Some(Epp::Power));
    }
}
