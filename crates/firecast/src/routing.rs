//! Regional terrain and conservative hazard sampling around a routed mission.
//! Flight A* and wind/climb pricing remain in the shared response planner.
use crate::{
    FirecastError,
    fleet::{Airframe, Leg, Route, Waypoint},
    geo::{LocalFrame, haversine_m},
    terrain::Dem,
};
use emberwatch_response::{Cell, CellPosition, Grid, MissionRequest, WindVector, plan_mission};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegionalTerrain {
    pub origin_lon_lat: [f64; 2],
    pub width: usize,
    pub height: usize,
    pub cell_m: f64,
    pub elevation_m: Vec<f32>,
    pub source: String,
}
impl RegionalTerrain {
    pub fn dem(&self) -> Result<Dem, FirecastError> {
        if self.width.checked_mul(self.height) != Some(self.elevation_m.len())
            || self.width == 0
            || self.height == 0
            || self.elevation_m.len() > 250_000
            || !self.cell_m.is_finite()
            || self.cell_m <= 0.
            || self.elevation_m.iter().any(|z| !z.is_finite())
        {
            return Err(FirecastError::Parse("regional terrain shape".into()));
        }
        Ok(Dem {
            width: self.width,
            height: self.height,
            cell_m: self.cell_m,
            elevation_m: self.elevation_m.clone(),
            frame: LocalFrame {
                origin_lon_lat: self.origin_lon_lat,
            },
        })
    }
}
pub fn terrain_at(dem: &Dem, p: [f64; 2]) -> Option<f64> {
    let (x, y) = dem.frame.to_local(p);
    if !x.is_finite() || !y.is_finite() || x < 0. || y < 0. {
        return None;
    }
    let (x, y) = (
        (x / dem.cell_m).floor() as usize,
        (y / dem.cell_m).floor() as usize,
    );
    if x >= dem.width || y >= dem.height {
        return None;
    }
    let z = dem.at(x, y);
    z.is_finite().then_some(z)
}

#[derive(Clone, Copy, Debug)]
pub struct Hazard {
    pub burn_probability: Option<f64>,
    pub smoke_ugm3: Option<f64>,
}
impl Hazard {
    pub fn blocked(self) -> bool {
        self.burn_probability
            .is_none_or(|p| !p.is_finite() || !(0.0..0.3).contains(&p))
            || self
                .smoke_ugm3
                .is_none_or(|c| !c.is_finite() || !(0.0..150.0).contains(&c))
    }
}
pub struct RoutedMission {
    pub route: Route,
    pub transit_s: f64,
}

/// Raster is about 500 m, padded around station and task. Unknown hazard/DEM
/// cells stay blocked. A far station is handled by a separate ground transfer.
pub fn flight(
    dem: &Dem,
    start: [f64; 2],
    task: [f64; 2],
    wind: WindVector,
    gust: Option<f64>,
    cfg: &Airframe,
    hazard: &impl Fn([f64; 2]) -> Hazard,
) -> Result<RoutedMission, FirecastError> {
    cfg.validate()?;
    let anchor = LocalFrame {
        origin_lon_lat: [start[0].min(task[0]), start[1].min(task[1])],
    };
    let (sx, sy) = anchor.to_local(start);
    let (tx, ty) = anchor.to_local(task);
    let cell = 500.;
    let origin = anchor.to_lon_lat(sx.min(tx) - 5000., sy.min(ty) - 5000.);
    let frame = LocalFrame {
        origin_lon_lat: origin,
    };
    let (sx, sy) = frame.to_local(start);
    let (tx, ty) = frame.to_local(task);
    let width = ((sx.max(tx) + 5000.) / cell).ceil() as usize;
    let height = ((sy.max(ty) + 5000.) / cell).ceil() as usize;
    if width > 120 || height > 120 {
        return Err(FirecastError::Compute(
            "station outside bounded flight window".into(),
        ));
    }
    let mut cells = Vec::with_capacity(width * height);
    let mut smokes = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            // Max over centre and corners: don't hide a narrow hazardous cell behind
            // a benign centre sample. No geometric path smoothing cuts blocked cells.
            let mut blocked = false;
            let (mut z, mut smoke) = (f64::NEG_INFINITY, 0.0_f64);
            for (ox, oy) in [
                (0.5, 0.5),
                (0.01, 0.01),
                (0.99, 0.01),
                (0.01, 0.99),
                (0.99, 0.99),
            ] {
                let p = frame.to_lon_lat((x as f64 + ox) * cell, (y as f64 + oy) * cell);
                let h = hazard(p);
                blocked |= h.blocked();
                if let Some(v) = terrain_at(dem, p) {
                    z = z.max(v);
                } else {
                    blocked = true;
                }
                if let Some(v) = h.smoke_ugm3 {
                    smoke = smoke.max(v);
                }
            }
            // An unavailable elevation is only a storage placeholder in a blocked
            // cell; it can never become a flight waypoint.
            cells.push(Cell {
                elevation_m: if z.is_finite() { z } else { 0. },
                blocked,
                smoke_penalty: (smoke / 150.).clamp(0., 100.),
                ..Default::default()
            });
            smokes.push(smoke);
        }
    }
    let grid =
        Grid::new(width, height, cell, cells).map_err(|e| FirecastError::Compute(e.to_string()))?;
    let pos = |x: f64, y: f64| CellPosition {
        x: (x / cell).floor() as usize,
        y: (y / cell).floor() as usize,
    };
    let (start_cell, target) = (pos(sx, sy), pos(tx, ty));
    let overhead = 2. * cfg.flight_agl_m / cfg.climb_mps;
    let request = MissionRequest {
        start: start_cell,
        target,
        wind,
        cruise_mps: cfg.cruise_mps,
        max_gust_mps: 22.,
        gust_mps: gust,
        battery_capacity_seconds: cfg.endurance_s,
        battery_pct: 100.,
        reserve_pct: cfg.reserve_fraction * 100.,
        task_seconds: 120. + overhead,
        climb_mps: cfg.climb_mps,
    };
    let mission =
        plan_mission(&grid, &request).map_err(|e| FirecastError::Compute(e.to_string()))?;
    let mut waypoints = Vec::new();
    let mut t = cfg.flight_agl_m / cfg.climb_mps;
    let mut max_smoke = 0.0_f64;
    for (leg_no, path) in [&mission.outbound, &mission.returning]
        .into_iter()
        .enumerate()
    {
        if leg_no == 1 {
            t += 120.;
        }
        for (i, p) in path.cells.iter().enumerate() {
            if i > 0 {
                let a = path.cells[i - 1];
                let from = grid.cell(a).unwrap();
                let to = grid.cell(*p).unwrap();
                let (dx, dy) = (p.x as f64 - a.x as f64, p.y as f64 - a.y as f64);
                let norm = dx.hypot(dy);
                let along = (wind.east_mps * dx + wind.north_mps * dy) / norm;
                let cross = (wind.east_mps * dy - wind.north_mps * dx) / norm;
                let speed = (cfg.cruise_mps.powi(2) - cross.powi(2)).sqrt() + along;
                t += norm * cell / speed * (1. + to.smoke_penalty)
                    + (to.elevation_m - from.elevation_m).max(0.) / cfg.climb_mps;
            }
            let ll = frame.to_lon_lat((p.x as f64 + 0.5) * cell, (p.y as f64 + 0.5) * cell);
            max_smoke = max_smoke.max(smokes[p.y * width + p.x]);
            waypoints.push(Waypoint {
                position: [
                    ll[0],
                    ll[1],
                    grid.cell(*p).unwrap().elevation_m + cfg.flight_agl_m,
                ],
                t_s: t,
            });
        }
    }
    let straight_line_m = 2. * haversine_m(start, task);
    let blocked_direct = (0..=100).any(|i| {
        let f = i as f64 / 100.;
        hazard([
            start[0] + (task[0] - start[0]) * f,
            start[1] + (task[1] - start[1]) * f,
        ])
        .blocked()
    });
    let distance = mission.outbound.distance_m + mission.returning.distance_m;
    let last = waypoints.len() - 1;
    Ok(RoutedMission {
        transit_s: mission
            .outbound
            .duration_seconds
            .max(mission.returning.duration_seconds),
        route: Route {
            waypoints,
            legs: vec![Leg {
                kind: "flight".into(),
                from: 0,
                to: last,
            }],
            duration_s: mission.total_seconds,
            distance_m: distance,
            straight_line_m,
            detour_reason: blocked_direct
                .then(|| "Detour around blocked fire/smoke or unknown hazard cells".into()),
            max_smoke_ugm3: max_smoke,
            reserve_pct_at_return: mission.battery_after_pct,
            flight_duration_s: mission.total_seconds,
            ground_duration_s: 0.,
        },
    })
}

/// Explicit schematic transfer, not a computed road route. Ground speed and
/// distance multiplier are demonstration assumptions, never observed road data.
pub fn with_ground_transfer(
    mut flight: RoutedMission,
    dem: &Dem,
    station: [f64; 2],
) -> Result<RoutedMission, FirecastError> {
    let first = flight
        .route
        .waypoints
        .first()
        .ok_or_else(|| FirecastError::Compute("empty flight".into()))?
        .position;
    let launch = [first[0], first[1]];
    let z = terrain_at(dem, station)
        .ok_or_else(|| FirecastError::Compute("station terrain unavailable".into()))?;
    let distance = haversine_m(station, launch) * 1.5;
    let seconds = distance / 12.;
    for point in &mut flight.route.waypoints {
        point.t_s += seconds;
    }
    flight.route.waypoints.insert(
        0,
        Waypoint {
            position: [station[0], station[1], z],
            t_s: 0.,
        },
    );
    for leg in &mut flight.route.legs {
        leg.from += 1;
        leg.to += 1;
    }
    let last = flight.route.waypoints.len();
    flight.route.waypoints.push(Waypoint {
        position: [station[0], station[1], z],
        t_s: flight.route.flight_duration_s + 2. * seconds,
    });
    flight.route.legs.insert(
        0,
        Leg {
            kind: "ground_transfer".into(),
            from: 0,
            to: 1,
        },
    );
    flight.route.legs.push(Leg {
        kind: "ground_transfer".into(),
        from: last - 1,
        to: last,
    });
    flight.route.ground_duration_s = 2. * seconds;
    flight.route.duration_s += 2. * seconds;
    flight.route.distance_m += 2. * distance;
    flight.route.straight_line_m += 2. * haversine_m(station, launch);
    flight.route.detour_reason=Some("Schematic ground transfer to a forward launch point; 12 m/s and road-distance factor 1.5 are demonstration assumptions. Flight avoids blocked smoke/fire; unknown hazards stay blocked.".into());
    Ok(flight)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocked_smoke_is_detoured_with_clearance_and_return_reserve() {
        let frame = LocalFrame::centred([-8., 40.], 6000.);
        let dem = Dem {
            width: 24,
            height: 24,
            cell_m: 500.,
            elevation_m: vec![100.; 576],
            frame,
        };
        let hazard = |ll| {
            let (x, y) = frame.to_local(ll);
            Hazard {
                burn_probability: Some(0.),
                smoke_ugm3: Some(
                    if (5500.0..6500.).contains(&x) && (3000.0..9000.).contains(&y) {
                        200.
                    } else {
                        0.
                    },
                ),
            }
        };
        let cfg = Airframe::default();
        let result = flight(
            &dem,
            frame.to_lon_lat(2500., 6000.),
            frame.to_lon_lat(9500., 6000.),
            WindVector {
                east_mps: 0.,
                north_mps: 0.,
            },
            Some(0.),
            &cfg,
            &hazard,
        )
        .unwrap();
        assert!(result.route.detour_reason.is_some());
        assert!(result.route.distance_m > result.route.straight_line_m);
        assert!(result.route.flight_duration_s <= cfg.endurance_s * (1. - cfg.reserve_fraction));
        assert!(result.route.reserve_pct_at_return >= 20.);
        for p in &result.route.waypoints {
            assert!(!hazard([p.position[0], p.position[1]]).blocked());
            assert!(
                p.position[2]
                    >= terrain_at(&dem, [p.position[0], p.position[1]]).unwrap() + 120. - 0.01
            );
        }
    }
    #[test]
    fn unknown_or_blocked_target_is_never_forced_open() {
        let frame = LocalFrame::centred([-8., 40.], 6000.);
        let dem = Dem {
            width: 24,
            height: 24,
            cell_m: 500.,
            elevation_m: vec![100.; 576],
            frame,
        };
        let unknown = |_| Hazard {
            burn_probability: None,
            smoke_ugm3: None,
        };
        assert!(
            flight(
                &dem,
                [-8., 40.],
                [-7.99, 40.01],
                WindVector {
                    east_mps: 0.,
                    north_mps: 0.
                },
                None,
                &Airframe::default(),
                &unknown
            )
            .is_err()
        );
        assert!(terrain_at(&dem, [-9., 40.]).is_none());
    }
}
