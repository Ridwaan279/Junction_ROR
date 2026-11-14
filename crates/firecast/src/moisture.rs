//! Fuel moisture estimates from weather.
//!
//! Dead fuel: equilibrium moisture content of Simard (1968), the formulation used by
//! the US NFDRS, with Rothermel's (1983) guidance that 10-h ≈ 1-h + 1 % and
//! 100-h ≈ 1-h + 2 %. Live fuel: explicit seasonal defaults for mainland Portugal
//! that are adjusted by vapour-pressure deficit and reported drought, and labelled.
use chrono::{Datelike, Timelike};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Moisture {
    /// Fractions (0.06 = 6 %).
    pub dead_1h: f64,
    pub dead_10h: f64,
    pub dead_100h: f64,
    pub live: f64,
}

impl Moisture {
    pub fn offset(self, dead_pct: f64, live_pct: f64) -> Self {
        let d = dead_pct / 100.0;
        Self {
            dead_1h: (self.dead_1h + d).clamp(0.02, 0.6),
            dead_10h: (self.dead_10h + d).clamp(0.02, 0.6),
            dead_100h: (self.dead_100h + d).clamp(0.02, 0.6),
            live: (self.live + live_pct / 100.0).clamp(0.3, 3.0),
        }
    }
}

/// Simard (1968) equilibrium moisture content, percent. `temp_c` air temperature,
/// `rh_pct` relative humidity.
pub fn simard_emc_pct(temp_c: f64, rh_pct: f64) -> f64 {
    let t = temp_c * 9.0 / 5.0 + 32.0;
    let h = rh_pct.clamp(0.0, 100.0);
    let emc = if h < 10.0 {
        0.03229 + 0.281073 * h - 0.000578 * h * t
    } else if h < 50.0 {
        2.22749 + 0.160107 * h - 0.01478 * t
    } else {
        21.0606 + 0.005565 * h * h - 0.00035 * h * t - 0.483199 * h
    };
    emc.max(1.0)
}

/// Seasonal live fuel moisture default (percent) for mainland Portugal shrubs and
/// understory. Coarse climatological prior; replaced when better data is available.
pub fn seasonal_live_pct(month: u32) -> f64 {
    match month {
        7..=9 => 85.0,
        6 | 10 => 105.0,
        5 | 11 => 125.0,
        _ => 150.0,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MoistureEstimate {
    pub moisture: Moisture,
    pub basis: Vec<String>,
}

pub struct MoistureInputs<'a> {
    pub time: chrono::DateTime<chrono::Utc>,
    pub temp_c: f64,
    pub rh_pct: f64,
    pub precip_last_3h_mm: Option<f64>,
    pub vpd_kpa: Option<f64>,
    /// Points subtracted from live moisture because of reported drought (positive = drier).
    pub drought_live_reduction_pct: f64,
    pub drought_label: Option<&'a str>,
}

pub fn estimate(inputs: &MoistureInputs<'_>) -> MoistureEstimate {
    let mut basis = Vec::new();
    let mut one_h = simard_emc_pct(inputs.temp_c, inputs.rh_pct);
    basis.push(format!(
        "1-h dead moisture {:.1} % from Simard (1968) EMC at {:.1} °C / {:.0} % RH",
        one_h, inputs.temp_c, inputs.rh_pct
    ));
    if let Some(rain) = inputs.precip_last_3h_mm
        && rain >= 1.0
    {
        one_h = one_h.max(20.0);
        basis.push(format!(
            "{rain:.1} mm rain in the previous 3 h: fine dead fuel set to at least 20 %"
        ));
    }
    let mut live = seasonal_live_pct(inputs.time.month());
    basis.push(format!(
        "Live moisture prior {live:.0} % for month {} (Portugal seasonal default)",
        inputs.time.month()
    ));
    if let Some(vpd) = inputs.vpd_kpa {
        let adj = if vpd >= 3.0 {
            -10.0
        } else if vpd >= 2.0 {
            -5.0
        } else if vpd < 0.5 {
            5.0
        } else {
            0.0
        };
        if adj != 0.0 {
            live += adj;
            basis.push(format!(
                "Vapour-pressure deficit {vpd:.1} kPa: live moisture {adj:+.0} points"
            ));
        }
    }
    if inputs.drought_live_reduction_pct > 0.0 {
        live -= inputs.drought_live_reduction_pct;
        basis.push(format!(
            "Reported drought ({}): live moisture -{:.0} points",
            inputs.drought_label.unwrap_or("unspecified"),
            inputs.drought_live_reduction_pct
        ));
    }
    let _ = inputs.time.hour();
    MoistureEstimate {
        moisture: Moisture {
            dead_1h: one_h / 100.0,
            dead_10h: (one_h + 1.0) / 100.0,
            dead_100h: (one_h + 2.0) / 100.0,
            live: live.clamp(30.0, 300.0) / 100.0,
        },
        basis,
    }
}

/// Drought wording (Portuguese IPMA categories or English) to a live-moisture reduction.
pub fn drought_reduction(text: &str) -> Option<(f64, &'static str)> {
    let t = text.to_lowercase();
    if t.contains("extrem") {
        Some((25.0, "extreme"))
    } else if t.contains("sever") {
        Some((15.0, "severe"))
    } else if t.contains("moderad") || t.contains("moderate") {
        Some((8.0, "moderate"))
    } else if t.contains("fraca") || t.contains("mild") || t.contains("weak") {
        Some((3.0, "weak"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn simard_emc_matches_hand_calculation() {
        // 30 °C (86 °F), 20 % RH: 2.22749 + 0.160107*20 - 0.01478*86 = 4.1593
        assert!((simard_emc_pct(30.0, 20.0) - 4.1593).abs() < 1e-3);
        // drier air gives drier fuel
        assert!(simard_emc_pct(35.0, 12.0) < simard_emc_pct(20.0, 60.0));
    }

    #[test]
    fn estimate_applies_rain_vpd_and_drought_with_reasons() {
        let time = chrono::Utc.with_ymd_and_hms(2026, 8, 10, 14, 0, 0).unwrap();
        let dry = estimate(&MoistureInputs {
            time,
            temp_c: 32.0,
            rh_pct: 18.0,
            precip_last_3h_mm: None,
            vpd_kpa: Some(3.5),
            drought_live_reduction_pct: 15.0,
            drought_label: Some("severe"),
        });
        assert!(dry.moisture.dead_1h < 0.06);
        assert!((dry.moisture.dead_10h - dry.moisture.dead_1h - 0.01).abs() < 1e-9);
        assert!((dry.moisture.live - 0.60).abs() < 1e-9); // 85 - 10 - 15
        assert_eq!(dry.basis.len(), 4);
        let wet = estimate(&MoistureInputs {
            precip_last_3h_mm: Some(4.0),
            ..MoistureInputs {
                time,
                temp_c: 32.0,
                rh_pct: 18.0,
                precip_last_3h_mm: None,
                vpd_kpa: None,
                drought_live_reduction_pct: 0.0,
                drought_label: None,
            }
        });
        assert!(wet.moisture.dead_1h >= 0.2);
        assert_eq!(drought_reduction("Seca severa"), Some((15.0, "severe")));
    }
}
