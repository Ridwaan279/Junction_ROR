//! Rothermel (1972) surface fire spread with the Albini (1976) multi-class
//! extension, as documented in Andrews (2018, RMRS-GTR-371). Internally uses the
//! model's native US customary units; the public interface is SI.
use crate::fuel::{FuelModel, SAV_10H_PER_FT, SAV_100H_PER_FT};
use crate::moisture::Moisture;
use serde::Serialize;

const TPA_TO_LB_FT2: f64 = 2000.0 / 43_560.0;
const PARTICLE_DENSITY: f64 = 32.0; // lb/ft³
const TOTAL_MINERAL: f64 = 0.0555;
const EFFECTIVE_MINERAL: f64 = 0.010;
pub const MPS_TO_FTMIN: f64 = 196.850_394;
const FTMIN_TO_MPS: f64 = 1.0 / MPS_TO_FTMIN;
const BTU_FT_S_TO_KW_M: f64 = 3.461_469;

/// Fuel-bed properties that depend only on fuel model and moisture.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct FuelBed {
    /// Reaction intensity, BTU/ft²/min.
    pub reaction_intensity: f64,
    /// No-wind, no-slope rate of spread, ft/min.
    pub r0_ft_min: f64,
    /// Heat per unit area, BTU/ft² (Anderson 1969 residence time 384/σ).
    pub heat_per_area: f64,
    pub beta_ratio: f64,
    pub packing_ratio: f64,
    pub sigma: f64,
    c: f64,
    b: f64,
    e: f64,
    /// Fuel load available to the flaming front, kg/m² (for smoke emission).
    pub consumed_kg_m2: f64,
}

/// Directional spread at a point.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Spread {
    /// Head-fire rate of spread, m/s.
    pub head_ros_mps: f64,
    /// Compass bearing (radians) the head fire travels toward.
    pub head_bearing: f64,
    /// Ellipse eccentricity from Anderson (1983) length-to-breadth ratio.
    pub eccentricity: f64,
    pub length_to_breadth: f64,
    /// Effective midflame wind speed (wind + slope), m/s.
    pub effective_wind_mps: f64,
    /// Byram fireline intensity at the head, kW/m.
    pub intensity_kw_m: f64,
    /// Byram (1959) flame length at the head, m.
    pub flame_length_m: f64,
    /// True when Rothermel's wind limit (U ≤ 0.9 I_R) capped the effective wind.
    pub wind_limited: bool,
    /// Crown fraction burned (Scott & Reinhardt 2001): 0 surface, 1 active crown fire.
    pub crown_fraction: f64,
}

impl Spread {
    /// Rate of spread in a direction `theta` radians from the head (Richards 1990
    /// elliptical growth from a focus): R(θ) = R_head (1 − e) / (1 − e cos θ).
    pub fn ros_at(&self, theta: f64) -> f64 {
        self.head_ros_mps * (1.0 - self.eccentricity) / (1.0 - self.eccentricity * theta.cos())
    }
}

fn eta_moisture(m: f64, mx: f64) -> f64 {
    if mx <= 0.0 {
        return 0.0;
    }
    let r = (m / mx).min(1.0);
    (1.0 - 2.59 * r + 5.11 * r * r - 3.52 * r * r * r).max(0.0)
}

/// Fuel-bed properties, or `None` when the fuel cannot carry fire.
pub fn fuel_bed(fuel: &FuelModel, moisture: &Moisture) -> Option<FuelBed> {
    let w0 = [
        fuel.load_tpa[0] * TPA_TO_LB_FT2,
        fuel.load_tpa[1] * TPA_TO_LB_FT2,
        fuel.load_tpa[2] * TPA_TO_LB_FT2,
        fuel.load_tpa[3] * TPA_TO_LB_FT2,
    ];
    let sav = [
        fuel.sav_1h_per_ft,
        SAV_10H_PER_FT,
        SAV_100H_PER_FT,
        fuel.sav_live_per_ft,
    ];
    let m = [
        moisture.dead_1h,
        moisture.dead_10h,
        moisture.dead_100h,
        moisture.live,
    ];
    let total: f64 = w0.iter().sum();
    if total <= 0.0 || fuel.depth_ft <= 0.0 {
        return None;
    }
    let area: Vec<f64> = (0..4).map(|i| sav[i] * w0[i] / PARTICLE_DENSITY).collect();
    let a_dead: f64 = area[..3].iter().sum();
    let a_live = area[3];
    let a_total = a_dead + a_live;
    let f_dead_j: Vec<f64> = (0..3)
        .map(|i| if a_dead > 0.0 { area[i] / a_dead } else { 0.0 })
        .collect();
    let f_dead = a_dead / a_total;
    let f_live = a_live / a_total;
    let sigma_dead: f64 = (0..3).map(|i| f_dead_j[i] * sav[i]).sum();
    let sigma = f_dead * sigma_dead + f_live * sav[3];

    let bulk_density = total / fuel.depth_ft;
    let beta = bulk_density / PARTICLE_DENSITY;
    let beta_op = 3.348 * sigma.powf(-0.8189);
    let ratio = beta / beta_op;
    let gamma_max = sigma.powf(1.5) / (495.0 + 0.0594 * sigma.powf(1.5));
    let a = 133.0 * sigma.powf(-0.7913);
    let gamma = gamma_max * ratio.powf(a) * (a * (1.0 - ratio)).exp();

    let wn_dead: f64 = (0..3)
        .map(|i| f_dead_j[i] * w0[i] * (1.0 - TOTAL_MINERAL))
        .sum();
    let wn_live = w0[3] * (1.0 - TOTAL_MINERAL);
    let mf_dead: f64 = (0..3).map(|i| f_dead_j[i] * m[i]).sum();
    let mx_dead = fuel.mx_dead_pct / 100.0;
    let eta_s = (0.174 * EFFECTIVE_MINERAL.powf(-0.19)).min(1.0);
    let eta_dead = eta_moisture(mf_dead, mx_dead);
    let mut eta_live = 0.0;
    if w0[3] > 0.0 {
        let fine_dead: f64 = (0..3).map(|i| w0[i] * (-138.0 / sav[i]).exp()).sum();
        let fine_live = w0[3] * (-500.0 / sav[3]).exp();
        let fine_dead_m: f64 = (0..3)
            .map(|i| w0[i] * (-138.0 / sav[i]).exp() * m[i])
            .sum::<f64>()
            / fine_dead.max(1e-12);
        let w_ratio = fine_dead / fine_live.max(1e-12);
        let mx_live = (2.9 * w_ratio * (1.0 - fine_dead_m / mx_dead) - 0.226).max(mx_dead);
        eta_live = eta_moisture(m[3], mx_live);
    }
    let reaction_intensity =
        gamma * fuel.heat_btu_per_lb * (wn_dead * eta_dead * eta_s + wn_live * eta_live * eta_s);
    let xi = ((0.792 + 0.681 * sigma.sqrt()) * (beta + 0.1)).exp() / (192.0 + 0.2595 * sigma);
    let q = |i: usize| (-138.0 / sav[i]).exp() * (250.0 + 1116.0 * m[i]);
    let heat_sink =
        bulk_density * (f_dead * (0..3).map(|i| f_dead_j[i] * q(i)).sum::<f64>() + f_live * q(3));
    if reaction_intensity <= 0.0 || heat_sink <= 0.0 {
        return None;
    }
    let r0 = reaction_intensity * xi / heat_sink;
    let residence_min = 384.0 / sigma;
    // Flaming-front consumption: fine and 10-h dead fully, 100-h and live half (approximation).
    let consumed_tpa =
        fuel.load_tpa[0] + fuel.load_tpa[1] + 0.5 * (fuel.load_tpa[2] + fuel.load_tpa[3]);
    Some(FuelBed {
        reaction_intensity,
        r0_ft_min: r0,
        heat_per_area: reaction_intensity * residence_min,
        beta_ratio: ratio,
        packing_ratio: beta,
        sigma,
        c: 7.47 * (-0.133 * sigma.powf(0.55)).exp(),
        b: 0.02526 * sigma.powf(0.54),
        e: 0.715 * (-3.59e-4 * sigma).exp(),
        consumed_kg_m2: consumed_tpa * 0.224_17,
    })
}

/// Anderson (1983) length-to-breadth ratio with the FARSITE −0.397 adjustment so a
/// no-wind fire is circular. `u_mph` is effective midflame wind.
pub fn length_to_breadth(u_mph: f64) -> f64 {
    (0.936 * (0.2566 * u_mph).exp() + 0.461 * (-0.1548 * u_mph).exp() - 0.397).clamp(1.0, 8.0)
}

/// Directional spread for a fuel bed.
/// * `midflame_wind_mps` – wind speed at midflame height.
/// * `wind_toward` – compass bearing (radians) the wind blows toward.
/// * `slope_tan` – terrain slope (rise/run), `upslope` its compass bearing.
pub fn spread(
    bed: &FuelBed,
    midflame_wind_mps: f64,
    wind_toward: f64,
    slope_tan: f64,
    upslope: f64,
) -> Spread {
    let u = midflame_wind_mps.max(0.0) * MPS_TO_FTMIN;
    let phi_w = if u > 0.0 {
        bed.c * u.powf(bed.b) * bed.beta_ratio.powf(-bed.e)
    } else {
        0.0
    };
    let phi_s = 5.275 * bed.packing_ratio.powf(-0.3) * slope_tan * slope_tan;
    let x = phi_w * wind_toward.sin() + phi_s * upslope.sin();
    let y = phi_w * wind_toward.cos() + phi_s * upslope.cos();
    let mut phi_e = x.hypot(y);
    let head_bearing = if phi_e > 0.0 {
        x.atan2(y).rem_euclid(std::f64::consts::TAU)
    } else {
        wind_toward
    };
    let mut u_eff = if phi_e > 0.0 {
        (phi_e * bed.beta_ratio.powf(bed.e) / bed.c).powf(1.0 / bed.b)
    } else {
        0.0
    };
    let limit = 0.9 * bed.reaction_intensity;
    let wind_limited = u_eff > limit;
    if wind_limited {
        u_eff = limit;
        phi_e = bed.c * u_eff.powf(bed.b) * bed.beta_ratio.powf(-bed.e);
    }
    let ros_ft_min = bed.r0_ft_min * (1.0 + phi_e);
    let lb = length_to_breadth(u_eff / 88.0);
    let eccentricity = (lb * lb - 1.0).max(0.0).sqrt() / lb;
    let intensity_btu = bed.heat_per_area * ros_ft_min / 60.0;
    let flame_ft = 0.45 * intensity_btu.max(0.0).powf(0.46);
    Spread {
        head_ros_mps: ros_ft_min * FTMIN_TO_MPS,
        head_bearing,
        eccentricity,
        length_to_breadth: lb,
        effective_wind_mps: u_eff * FTMIN_TO_MPS,
        intensity_kw_m: intensity_btu * BTU_FT_S_TO_KW_M,
        flame_length_m: flame_ft * 0.3048,
        wind_limited,
        crown_fraction: 0.0,
    }
}

/// Canopy fuel properties for crown-fire assessment.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Canopy {
    /// Canopy base height, m.
    pub base_height_m: f64,
    /// Canopy bulk density, kg/m³.
    pub bulk_density_kg_m3: f64,
    /// Available canopy fuel load, kg/m².
    pub fuel_load_kg_m2: f64,
    /// Foliar moisture content, percent.
    pub foliar_moisture_pct: f64,
}

/// Van Wagner (1977) critical surface intensity for crown initiation, kW/m.
pub fn crown_initiation_kw_m(canopy: &Canopy) -> f64 {
    (0.010 * canopy.base_height_m * (460.0 + 25.9 * canopy.foliar_moisture_pct)).powf(1.5)
}

pub fn fire_type(crown_fraction: f64) -> &'static str {
    if crown_fraction <= 0.0 {
        "surface"
    } else if crown_fraction < 0.9 {
        "passive crown (torching)"
    } else {
        "active crown"
    }
}

/// Crown-fire adjustment as in FARSITE/FlamMap (Finney 1998):
/// * initiation when surface intensity ≥ Van Wagner's I₀;
/// * crown rate R_c = 3.34 · R₁₀ (Rothermel 1991), `r10_mps` being fuel model 10 with
///   a 0.4 wind reduction of the open 20-ft wind;
/// * crown fraction burned CFB = 1 − exp(−a (R_s − R_init)) with a chosen so CFB = 0.9 at
///   Van Wagner's active-crown threshold 3/CBD (Scott & Reinhardt 2001);
/// * spread R = R_s + CFB (R_c − R_s), intensity includes burning canopy fuel (18 MJ/kg),
///   crown flame length after Thomas (1963), ellipse at least 1 + 0.125·U₂₀ (mi/h).
pub fn crown_adjust(
    surface: Spread,
    bed: &FuelBed,
    canopy: &Canopy,
    r10_mps: f64,
    open_wind20_mps: f64,
) -> Spread {
    let i0 = crown_initiation_kw_m(canopy);
    if surface.intensity_kw_m < i0 || canopy.bulk_density_kg_m3 <= 0.0 {
        return surface;
    }
    let hpa_kj_m2 = bed.heat_per_area * 11.3565;
    let r_init = i0 / hpa_kj_m2.max(1.0); // m/s
    let r_active = 3.0 / canopy.bulk_density_kg_m3 / 60.0; // m/s
    let r_crown = 3.34 * r10_mps;
    let a = if r_active > r_init {
        -(0.1f64.ln()) / (0.9 * (r_active - r_init))
    } else {
        f64::INFINITY
    };
    let cfb = (1.0 - (-a * (surface.head_ros_mps - r_init).max(0.0)).exp()).clamp(0.0, 1.0);
    let cfb = if r_crown >= r_active && surface.head_ros_mps >= r_init {
        cfb.max(0.9)
    } else {
        cfb
    };
    let ros = surface.head_ros_mps + cfb * (r_crown - surface.head_ros_mps).max(0.0);
    let intensity = (hpa_kj_m2 + cfb * canopy.fuel_load_kg_m2 * 18_000.0) * ros;
    let lb = surface
        .length_to_breadth
        .max((1.0 + 0.125 * open_wind20_mps / 0.447_04).min(8.0));
    Spread {
        head_ros_mps: ros,
        intensity_kw_m: intensity,
        flame_length_m: if cfb >= 0.5 {
            0.0266 * intensity.powf(2.0 / 3.0)
        } else {
            surface.flame_length_m
        },
        length_to_breadth: lb,
        eccentricity: (lb * lb - 1.0).max(0.0).sqrt() / lb,
        crown_fraction: cfb,
        ..surface
    }
}

/// Suppression interpretation of fireline intensity (Andrews & Rothermel 1982 "hauling chart").
pub fn suppression_class(intensity_kw_m: f64) -> &'static str {
    if intensity_kw_m < 346.0 {
        "hand crews can attack the head"
    } else if intensity_kw_m < 1730.0 {
        "too intense for hand crews; machinery and aircraft needed"
    } else if intensity_kw_m < 3460.0 {
        "control efforts at the head likely ineffective"
    } else {
        "crowning, spotting and major runs likely; head attack ineffective"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuel::anderson;

    fn dry() -> Moisture {
        Moisture {
            dead_1h: 0.06,
            dead_10h: 0.07,
            dead_100h: 0.08,
            live: 0.6,
        }
    }

    #[test]
    fn short_grass_matches_published_behaveplus_magnitude() {
        // FM1, 6 % 1-h, 5 mi/h midflame, flat: BehavePlus reports ~78 ch/h (~26 m/min).
        let bed = fuel_bed(anderson(1).unwrap(), &dry()).unwrap();
        let s = spread(&bed, 5.0 * 0.447_04, 0.0, 0.0, 0.0);
        let m_per_min = s.head_ros_mps * 60.0;
        assert!(
            (18.0..34.0).contains(&m_per_min),
            "FM1 ROS {m_per_min} m/min"
        );
    }

    #[test]
    fn wind_and_slope_increase_spread_and_wet_fuel_stops_it() {
        let bed = fuel_bed(anderson(10).unwrap(), &dry()).unwrap();
        let calm = spread(&bed, 0.0, 0.0, 0.0, 0.0);
        let windy = spread(&bed, 3.0, 0.0, 0.0, 0.0);
        let uphill = spread(&bed, 0.0, 0.0, 0.5, std::f64::consts::FRAC_PI_2);
        assert!(windy.head_ros_mps > 3.0 * calm.head_ros_mps);
        assert!(uphill.head_ros_mps > calm.head_ros_mps);
        assert!((uphill.head_bearing - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert!((calm.eccentricity).abs() < 1e-9 && windy.eccentricity > 0.5);
        assert!(windy.ros_at(std::f64::consts::PI) < windy.ros_at(0.0) / 5.0);
        let soaked = Moisture {
            dead_1h: 0.35,
            dead_10h: 0.36,
            dead_100h: 0.37,
            live: 2.0,
        };
        assert!(fuel_bed(anderson(1).unwrap(), &soaked).is_none_or(|b| b.r0_ft_min < 1e-9));
    }

    #[test]
    fn wind_and_slope_vectors_combine() {
        let bed = fuel_bed(anderson(4).unwrap(), &dry()).unwrap();
        // Wind toward east, slope upslope toward north: head lies between.
        let s = spread(&bed, 2.0, std::f64::consts::FRAC_PI_2, 0.6, 0.0);
        let deg = s.head_bearing.to_degrees();
        assert!(deg > 0.0 && deg < 90.0, "{deg}");
        assert!(s.intensity_kw_m > 0.0 && s.flame_length_m > 0.0);
        assert_eq!(length_to_breadth(0.0), 1.0);
    }
}

#[cfg(test)]
mod crown_tests {
    use super::*;
    use crate::fuel::anderson;

    #[test]
    fn van_wagner_initiation_matches_hand_value() {
        // CBH 3 m, FMC 110 %: (0.01·3·(460 + 2849))^1.5 ≈ 989 kW/m
        let c = Canopy {
            base_height_m: 3.0,
            bulk_density_kg_m3: 0.15,
            fuel_load_kg_m2: 1.0,
            foliar_moisture_pct: 110.0,
        };
        assert!((crown_initiation_kw_m(&c) - 989.5).abs() < 2.0);
        assert_eq!(fire_type(0.0), "surface");
        assert_eq!(fire_type(1.0), "active crown");
    }

    #[test]
    fn strong_wind_makes_dry_forest_crown_and_weak_wind_stays_surface() {
        let m = Moisture {
            dead_1h: 0.05,
            dead_10h: 0.06,
            dead_100h: 0.07,
            live: 0.8,
        };
        let bed = fuel_bed(anderson(10).unwrap(), &m).unwrap();
        let canopy = Canopy {
            base_height_m: 3.0,
            bulk_density_kg_m3: 0.15,
            fuel_load_kg_m2: 1.0,
            foliar_moisture_pct: 110.0,
        };
        let strong_open = 12.0 / 1.15;
        let strong = spread(&bed, strong_open * 0.36, 0.0, 0.0, 0.0);
        let r10 = spread(&bed, strong_open * 0.4, 0.0, 0.0, 0.0).head_ros_mps;
        let crown = crown_adjust(strong, &bed, &canopy, r10, strong_open);
        assert!(crown.crown_fraction > 0.0, "{crown:?}");
        assert!(crown.head_ros_mps > strong.head_ros_mps);
        assert!(crown.intensity_kw_m > strong.intensity_kw_m);
        let calm = spread(&bed, 0.3, 0.0, 0.0, 0.0);
        let calm_r10 = spread(&bed, 0.4, 0.0, 0.0, 0.0).head_ros_mps;
        assert_eq!(crown_adjust(calm, &bed, &canopy, calm_r10, 1.0), calm);
    }
}
