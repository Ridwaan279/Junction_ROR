//! Deterministic demonstration fleet sizing. No real aircraft specifications.
use crate::FirecastError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub position: [f64; 2],
    pub simulated: bool,
    pub drones_total: usize,
    pub drones_available: usize,
    pub coordinate_source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Airframe {
    pub label: String,
    pub cruise_mps: f64,
    pub endurance_s: f64,
    pub recharge_s: f64,
    pub reserve_fraction: f64,
    pub revisit_s: f64,
    pub flight_agl_m: f64,
    pub climb_mps: f64,
    pub exposure_cap: usize,
}
impl Default for Airframe {
    fn default() -> Self {
        Self {
            label: "demonstration values; simulated aircraft".into(),
            cruise_mps: 18.,
            endurance_s: 5400.,
            recharge_s: 1800.,
            reserve_fraction: 0.2,
            revisit_s: 600.,
            flight_agl_m: 120.,
            climb_mps: 3.,
            exposure_cap: 6,
        }
    }
}
impl Airframe {
    pub fn validate(&self) -> Result<(), FirecastError> {
        if [
            self.cruise_mps,
            self.endurance_s,
            self.revisit_s,
            self.flight_agl_m,
            self.climb_mps,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v <= 0.)
            || !self.recharge_s.is_finite()
            || self.recharge_s < 0.
            || !self.reserve_fraction.is_finite()
            || !(0.0..1.0).contains(&self.reserve_fraction)
        {
            return Err(FirecastError::Config(
                "invalid demonstration airframe".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Term {
    pub term: String,
    pub value: f64,
    pub unit: String,
    pub basis: String,
    pub source: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sizing {
    pub count: usize,
    pub airborne_perimeter: usize,
    pub airborne_head: usize,
    pub airborne_exposure: usize,
    pub duty: f64,
    pub breakdown: Vec<Term>,
}

/// The workplan formula, without a hidden cap. A station with nonpositive duty
/// is out of range. `head_sectors` and exposure clusters come from this cursor.
pub fn size(
    front_m: f64,
    head_sectors: usize,
    exposure_clusters: usize,
    transit_s: f64,
    cfg: &Airframe,
) -> Result<Sizing, FirecastError> {
    cfg.validate()?;
    if !front_m.is_finite() || front_m < 0. || !transit_s.is_finite() || transit_s < 0. {
        return Err(FirecastError::Config("invalid fleet front/transit".into()));
    }
    let duty = (cfg.endurance_s - 2. * transit_s) / (cfg.endurance_s + cfg.recharge_s);
    if duty <= 0. {
        return Err(FirecastError::Compute(
            "station out of range: nonpositive duty".into(),
        ));
    }
    let perimeter = (front_m / (cfg.cruise_mps * cfg.revisit_s / 2.)).ceil() as usize;
    let head = head_sectors.max(1);
    let exposure = exposure_clusters.min(cfg.exposure_cap);
    let count_f =
        ((perimeter + head + exposure) as f64 / duty * (1. + cfg.reserve_fraction)).ceil();
    if count_f > 100_000. {
        return Err(FirecastError::Compute(
            "fleet demand exceeds demonstration limit; no allocation made".into(),
        ));
    }
    let terms = [
        ("P_front", front_m, "m", "model: predicted one-hour front"),
        (
            "v",
            cfg.cruise_mps,
            "m/s",
            "configured: demonstration values",
        ),
        (
            "T_revisit",
            cfg.revisit_s,
            "s",
            "configured: demonstration values",
        ),
        (
            "E",
            cfg.endurance_s,
            "s",
            "configured: demonstration values",
        ),
        ("R", cfg.recharge_s, "s", "configured: demonstration values"),
        (
            "t_transit",
            transit_s,
            "s",
            "model: conservative outbound/return route transit",
        ),
        (
            "reserve",
            cfg.reserve_fraction,
            "fraction",
            "configured: demonstration values",
        ),
        (
            "airborne_perimeter",
            perimeter as f64,
            "drones",
            "ceil(P_front / (v * T_revisit / 2))",
        ),
        (
            "airborne_head",
            head as f64,
            "drones",
            "model: distinct fast head sectors; at least one",
        ),
        (
            "airborne_exposure",
            exposure as f64,
            "drones",
            "model: eligible exposure clusters, capped",
        ),
        ("duty", duty, "fraction", "(E - 2 * t_transit) / (E + R)"),
        (
            "fleet",
            count_f,
            "drones",
            "ceil((perimeter + head + exposure) / duty * (1 + reserve))",
        ),
    ];
    Ok(Sizing {
        count: count_f as usize,
        airborne_perimeter: perimeter,
        airborne_head: head,
        airborne_exposure: exposure,
        duty,
        breakdown: terms
            .into_iter()
            .map(|(term, value, unit, basis)| Term {
                term: term.into(),
                value,
                unit: unit.into(),
                basis: basis.into(),
                source: None,
            })
            .collect(),
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Waypoint {
    pub position: [f64; 3],
    pub t_s: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Leg {
    pub kind: String,
    pub from: usize,
    pub to: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Route {
    pub waypoints: Vec<Waypoint>,
    pub legs: Vec<Leg>,
    pub duration_s: f64,
    pub distance_m: f64,
    pub straight_line_m: f64,
    pub detour_reason: Option<String>,
    pub max_smoke_ugm3: f64,
    pub reserve_pct_at_return: f64,
    /// Ground transfer consumes wall time, not aircraft endurance.
    pub flight_duration_s: f64,
    pub ground_duration_s: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Drone {
    pub id: String,
    pub station_id: String,
    pub role: String,
    pub route: Option<Route>,
    pub task_point: [f64; 2],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Held {
    pub station_id: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FleetPlan {
    pub at: DateTime<Utc>,
    pub count: usize,
    pub previous_count: Option<usize>,
    pub change_reason_pt: Option<String>,
    pub change_reason_en: Option<String>,
    pub breakdown: Vec<Term>,
    pub drones: Vec<Drone>,
    pub held: Vec<Held>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_hand_computed_case() {
        let cfg = Airframe {
            cruise_mps: 20.,
            revisit_s: 600.,
            endurance_s: 3600.,
            recharge_s: 1200.,
            reserve_fraction: 0.2,
            ..Default::default()
        };
        // 12 km / 6 km = 2 perimeter; +1 head +2 exposure =5.
        // Duty=(3600-2*600)/(3600+1200)=0.5; ceil(5/.5*1.2)=12.
        let s = size(12000., 1, 2, 600., &cfg).unwrap();
        assert_eq!(s.count, 12);
        assert_eq!(s.airborne_perimeter, 2);
        assert_eq!(s.duty, 0.5);
        assert!(size(12000., 1, 2, 1800., &cfg).is_err());
    }
    #[test]
    fn monotonic_and_revisit_direction() {
        let cfg = Airframe::default();
        let initial = size(12000., 1, 0, 300., &cfg).unwrap().count;
        assert!(size(30000., 1, 0, 300., &cfg).unwrap().count >= initial);
        assert!(size(12000., 1, 0, 1200., &cfg).unwrap().count >= initial);
        let slower = Airframe {
            revisit_s: 1200.,
            ..cfg.clone()
        };
        assert!(size(12000., 1, 0, 300., &slower).unwrap().count <= initial);
        assert!(size(f64::NAN, 1, 0, 0., &cfg).is_err());
    }
}
