//! Ground-level smoke (PM2.5) from a steady-state Gaussian plume.
//!
//! * Emission: area growth × fuel consumed × emission factor (17 g PM2.5 / kg, within
//!   the 12–24 g/kg range reported for temperate forest fires).
//! * Heat release: heat per unit area × area growth (Byram intensity integrated over
//!   the perimeter); a convective fraction of 0.65 drives Briggs (1975) plume rise.
//! * Stability: Pasquill–Turner classes from 10 m wind and insolation (solar
//!   elevation, cloud, shortwave radiation); dispersion by Briggs (1973) open-country
//!   coefficients; ground reflection and mixing-layer trapping (Turner 1970).
//! * Wildfire smoke fills the convection column, so emissions are spread uniformly
//!   between the surface and the effective plume height (a vertical line source of
//!   five sub-sources) instead of being released at a single elevated point.
use serde::{Deserialize, Serialize};

pub const PM25_EMISSION_FACTOR_G_PER_KG: f64 = 17.0;
pub const CONVECTIVE_FRACTION: f64 = 0.65;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stability {
    A,
    B,
    C,
    D,
    E,
    F,
}

impl Stability {
    fn index(self) -> usize {
        self as usize
    }
    /// Typical mixing-layer depth used when no boundary-layer height is supplied (m).
    pub fn default_mixing_height_m(self) -> f64 {
        [1600.0, 1200.0, 1000.0, 800.0, 400.0, 250.0][self.index()]
    }
    /// Wind-profile power-law exponent (rural; EPA ISC3).
    pub fn wind_exponent(self) -> f64 {
        [0.07, 0.07, 0.10, 0.15, 0.35, 0.55][self.index()]
    }
    pub fn sigma_y(self, x: f64) -> f64 {
        let c = [0.22, 0.16, 0.11, 0.08, 0.06, 0.04][self.index()];
        c * x / (1.0 + 1e-4 * x).sqrt()
    }
    pub fn sigma_z(self, x: f64) -> f64 {
        let s = match self {
            Stability::A => 0.20 * x,
            Stability::B => 0.12 * x,
            Stability::C => 0.08 * x / (1.0 + 2e-4 * x).sqrt(),
            Stability::D => 0.06 * x / (1.0 + 1.5e-3 * x).sqrt(),
            Stability::E => 0.03 * x / (1.0 + 3e-4 * x),
            Stability::F => 0.016 * x / (1.0 + 3e-4 * x),
        };
        s.min(5000.0)
    }
    pub fn label(self) -> &'static str {
        [
            "A (very unstable)",
            "B (unstable)",
            "C (slightly unstable)",
            "D (neutral)",
            "E (slightly stable)",
            "F (stable)",
        ][self.index()]
    }
}

/// Pasquill–Turner class. Half classes (A–B …) resolve to the more stable letter,
/// which is conservative for ground-level concentration.
pub fn stability_class(
    wind10_mps: f64,
    solar_elevation_deg: f64,
    cloud_pct: Option<f64>,
    shortwave_wm2: Option<f64>,
) -> Stability {
    use Stability::*;
    let u = wind10_mps;
    let cloud = cloud_pct.unwrap_or(30.0);
    if solar_elevation_deg <= 0.0 {
        let overcast = cloud >= 50.0;
        return if u < 2.0 {
            F
        } else if u < 3.0 {
            if overcast { E } else { F }
        } else if u < 5.0 {
            if overcast { D } else { E }
        } else {
            D
        };
    }
    // Insolation: 0 = slight, 1 = moderate, 2 = strong.
    let insolation = match shortwave_wm2 {
        Some(sw) if sw > 600.0 => 2,
        Some(sw) if sw > 300.0 => 1,
        Some(_) => 0,
        None => {
            let base = if solar_elevation_deg > 60.0 {
                2
            } else if solar_elevation_deg > 35.0 {
                1
            } else {
                0
            };
            if cloud > 70.0 {
                0
            } else if cloud > 40.0 {
                (base as i32 - 1).max(0) as usize
            } else {
                base
            }
        }
    };
    let row: [Stability; 3] = if u < 2.0 {
        [B, B, A]
    } else if u < 3.0 {
        [C, B, B]
    } else if u < 5.0 {
        [C, C, B]
    } else {
        // 5–6 m/s and above 6 m/s share a row once half classes resolve to the more stable letter.
        [D, D, C]
    };
    row[insolation]
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlumeSource {
    /// Local metres.
    pub x: f64,
    pub y: f64,
    pub radius_m: f64,
    /// PM2.5 emission, g/s.
    pub emission_g_s: f64,
    /// Total heat release rate, W.
    pub heat_release_w: f64,
    /// Seconds since ignition (limits plume length to distance travelled).
    pub age_s: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlumeMet {
    pub wind10_mps: f64,
    /// Transport wind at plume level, m/s, and the bearing (radians) it blows toward.
    pub transport_mps: f64,
    pub transport_toward: f64,
    pub stability: Stability,
    pub mixing_height_m: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct PlumeSummary {
    pub buoyancy_flux_m4_s3: f64,
    pub plume_rise_m: f64,
    pub effective_height_m: f64,
    pub trapped_in_mixing_layer: bool,
    pub max_length_m: f64,
}

/// Briggs buoyancy flux, m⁴/s³ (g Q / (π c_p ρ T) with standard air).
pub fn buoyancy_flux(convective_w: f64) -> f64 {
    9.81 * convective_w / (std::f64::consts::PI * 1005.0 * 1.2 * 293.0)
}

/// Briggs final plume rise (m).
pub fn plume_rise(f: f64, u: f64, stability: Stability) -> f64 {
    let u = u.max(1.0);
    match stability {
        Stability::E | Stability::F => {
            let dtheta = if stability == Stability::E {
                0.020
            } else {
                0.035
            };
            let s = 9.81 / 293.0 * dtheta;
            2.6 * (f / (u * s)).cbrt()
        }
        _ if f < 55.0 => 21.425 * f.powf(0.75) / u,
        _ => 38.71 * f.powf(0.6) / u,
    }
}

pub fn summarise(source: &PlumeSource, met: &PlumeMet) -> PlumeSummary {
    let f = buoyancy_flux(source.heat_release_w * CONVECTIVE_FRACTION);
    let rise = plume_rise(f, met.transport_mps, met.stability);
    PlumeSummary {
        buoyancy_flux_m4_s3: f,
        plume_rise_m: rise,
        effective_height_m: rise.min(met.mixing_height_m),
        trapped_in_mixing_layer: rise >= met.mixing_height_m,
        max_length_m: met.transport_mps.max(1.0) * source.age_s.max(600.0),
    }
}

/// Ground-level PM2.5 (µg/m³) at local point (px, py).
pub fn concentration(
    source: &PlumeSource,
    met: &PlumeMet,
    summary: &PlumeSummary,
    px: f64,
    py: f64,
) -> f64 {
    let (dx, dy) = (px - source.x, py - source.y);
    let (s, c) = (met.transport_toward.sin(), met.transport_toward.cos());
    let along = dx * s + dy * c;
    let cross = -dx * c + dy * s;
    if along > summary.max_length_m || along < -source.radius_m {
        return 0.0;
    }
    let x = along.max(source.radius_m.max(100.0) * 0.5);
    let sy0 = source.radius_m / 2.15;
    let sigma_y = (met.stability.sigma_y(x).powi(2) + sy0 * sy0).sqrt();
    let sigma_z = met.stability.sigma_z(x).max(1.0);
    let u = met.transport_mps.max(1.0);
    let lateral = (-cross * cross / (2.0 * sigma_y * sigma_y)).exp();
    let l = met.mixing_height_m;
    if sigma_z > 1.6 * l {
        return source.emission_g_s / ((std::f64::consts::TAU).sqrt() * u * sigma_y * l)
            * lateral
            * 1e6;
    }
    const LAYERS: usize = 5;
    let q = source.emission_g_s / LAYERS as f64;
    let grams: f64 = (0..LAYERS)
        .map(|k| {
            let h = (k as f64 + 0.5) / LAYERS as f64 * summary.effective_height_m;
            q / (std::f64::consts::PI * u * sigma_y * sigma_z)
                * (-h * h / (2.0 * sigma_z * sigma_z)).exp()
        })
        .sum();
    grams * lateral * 1e6
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src() -> PlumeSource {
        PlumeSource {
            x: 0.0,
            y: 0.0,
            radius_m: 300.0,
            emission_g_s: 2000.0,
            heat_release_w: 2e9,
            age_s: 7200.0,
        }
    }

    #[test]
    fn stability_table_follows_pasquill_turner() {
        assert_eq!(stability_class(1.0, 65.0, Some(0.0), None), Stability::A);
        assert_eq!(stability_class(7.0, 65.0, Some(0.0), None), Stability::C);
        assert_eq!(
            stability_class(4.0, 20.0, Some(0.0), Some(150.0)),
            Stability::C
        );
        assert_eq!(stability_class(1.5, -10.0, Some(0.0), None), Stability::F);
        assert_eq!(stability_class(4.0, -10.0, Some(80.0), None), Stability::D);
    }

    #[test]
    fn briggs_rise_matches_hand_values() {
        // F = 100 m⁴/s³, u = 5 m/s, neutral: 38.71 * 100^0.6 / 5 ≈ 122.6 m
        assert!((plume_rise(100.0, 5.0, Stability::D) - 122.6).abs() < 0.5);
        assert!(
            (plume_rise(10.0, 5.0, Stability::C) - 21.425 * 10f64.powf(0.75) / 5.0).abs() < 1e-9
        );
        assert!(plume_rise(100.0, 5.0, Stability::F) < plume_rise(100.0, 5.0, Stability::D));
    }

    #[test]
    fn plume_goes_downwind_and_decays() {
        let met = PlumeMet {
            wind10_mps: 5.0,
            transport_mps: 7.0,
            transport_toward: std::f64::consts::FRAC_PI_2,
            stability: Stability::D,
            mixing_height_m: 800.0,
        };
        let s = summarise(&src(), &met);
        assert!(s.trapped_in_mixing_layer);
        let near = concentration(&src(), &met, &s, 5_000.0, 0.0);
        let far = concentration(&src(), &met, &s, 30_000.0, 0.0);
        let side = concentration(&src(), &met, &s, 5_000.0, 4_000.0);
        let upwind = concentration(&src(), &met, &s, -5_000.0, 0.0);
        assert!(near > far && far > 0.0, "{near} {far}");
        assert!(near > side);
        assert_eq!(upwind, 0.0);
        // Plume cannot be longer than distance travelled since ignition.
        assert_eq!(concentration(&src(), &met, &s, 60_000.0, 0.0), 0.0);
    }
}
