//! Turn a fire forecast into drone-planning hazards and plan a simulated mission
//! with the shared response planner: predicted fire is a no-fly area, predicted
//! smoke is a travel-time penalty, the depot sits upwind, and the task point is a
//! flank of the 1-hour front where intensity is lowest.
use crate::forecast::FirecastResult;
use emberwatch_domain::{DronePlan, Incident, RunContext};
use emberwatch_response::{
    Cell, CellPosition, Grid, LocalProjection, MissionRequest, WindVector, build_drone_plan,
    plan_mission,
};

pub const PLAN_CELL_M: f64 = 400.0;
pub const NO_FLY_PROBABILITY: f64 = 0.3;
pub const MISSION_HORIZON_S: f64 = 3600.0;

pub fn plan_drone(
    context: &RunContext,
    incident: &Incident,
    result: &FirecastResult,
) -> Result<DronePlan, String> {
    let fine = result.ensemble.width;
    let span = fine as f64 * result.cell_m;
    let n = (span / PLAN_CELL_M).floor() as usize;
    let ratio = PLAN_CELL_M / result.cell_m;
    let burn = result.ensemble.burn_probability(MISSION_HORIZON_S);
    let smoke = result
        .smoke
        .iter()
        .min_by_key(|s| s.hour)
        .ok_or("no smoke field")?;
    let mut cells = Vec::with_capacity(n * n);
    let mut p_coarse = vec![0.0; n * n];
    for y in 0..n {
        for x in 0..n {
            let (mut elev, mut p, mut count) = (0.0f64, 0.0f64, 0.0f64);
            let (fx0, fy0) = ((x as f64 * ratio) as usize, (y as f64 * ratio) as usize);
            let (fx1, fy1) = (
                (((x + 1) as f64) * ratio) as usize,
                (((y + 1) as f64) * ratio) as usize,
            );
            for fy in fy0..fy1.min(fine) {
                for fx in fx0..fx1.min(fine) {
                    elev += f64::from(result.elevation_m[fy * fine + fx]);
                    p = p.max(burn[fy * fine + fx]);
                    count += 1.0;
                }
            }
            let centre = result.frame.to_lon_lat(
                (x as f64 + 0.5) * PLAN_CELL_M,
                (y as f64 + 0.5) * PLAN_CELL_M,
            );
            let c = smoke.at(centre);
            p_coarse[y * n + x] = p;
            cells.push(Cell {
                fuel: 0.5,
                humidity: 0.0,
                elevation_m: (elev / count.max(1.0)).clamp(-400.0, 8000.0),
                blocked: p >= NO_FLY_PROBABILITY,
                // Planner multiplies travel time by (1 + penalty): 35 µg/m³ ≈ +23 %, ≥ 300 µg/m³ = ×3.
                smoke_penalty: (c / 150.0).clamp(0.0, 2.0),
            });
        }
    }
    let mut grid = Grid::new(n, n, PLAN_CELL_M, cells).map_err(|e| e.to_string())?;
    let at = |x: f64, y: f64| CellPosition {
        x: ((x / PLAN_CELL_M) as usize).min(n - 1),
        y: ((y / PLAN_CELL_M) as usize).min(n - 1),
    };
    let (ix, iy) = result.ignition_local;
    // Task point: unblocked cell just outside the 1-h front, closest to a flank (head ± 90°),
    // preferring the side with less smoke.
    let head = result.head_bearing_deg.to_radians();
    let mut best: Option<(f64, CellPosition, &'static str)> = None;
    for y in 1..n - 1 {
        for x in 1..n - 1 {
            let k = y * n + x;
            if p_coarse[k] >= NO_FLY_PROBABILITY {
                continue;
            }
            let touches_front =
                [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(dx, dy)| {
                        p_coarse[(y as isize + dy) as usize * n + (x as isize + dx) as usize] >= 0.5
                    });
            if !touches_front {
                continue;
            }
            let (cx, cy) = (
                (x as f64 + 0.5) * PLAN_CELL_M - ix,
                (y as f64 + 0.5) * PLAN_CELL_M - iy,
            );
            let bearing = cx.atan2(cy);
            let rel = (bearing - head).sin(); // +1 right flank, −1 left flank
            let flank_error = 1.0 - rel.abs();
            let score = flank_error * 4.0 + grid.cells()[k].smoke_penalty / 20.0;
            if best.is_none_or(|b| score < b.0) {
                best = Some((
                    score,
                    CellPosition { x, y },
                    if rel >= 0.0 { "right" } else { "left" },
                ));
            }
        }
    }
    let (target, side) = match best {
        Some((_, t, s)) => (t, s),
        None => (
            at(
                ix + 1200.0 * (head + std::f64::consts::FRAC_PI_2).sin(),
                iy + 1200.0 * (head + std::f64::consts::FRAC_PI_2).cos(),
            ),
            "right",
        ),
    };
    grid.set_blocked(target, false).map_err(|e| e.to_string())?;
    let wind = WindVector {
        east_mps: result.transport_mps * result.transport_toward_deg.to_radians().sin(),
        north_mps: result.transport_mps * result.transport_toward_deg.to_radians().cos(),
    };
    // Staging candidates 7 km from the ignition: upwind (smoke-free), then ±45° and
    // crosswind (cheaper return leg in strong wind). Keep the fastest feasible mission.
    let mut chosen = None;
    let mut rejected = Vec::new();
    for (offset, label) in [
        (180.0, "upwind"),
        (135.0, "upwind-right"),
        (225.0, "upwind-left"),
        (90.0, "crosswind-right"),
        (270.0, "crosswind-left"),
    ] {
        let bearing = (result.transport_toward_deg + offset).to_radians();
        let depot_xy = (
            (ix + 7000.0 * bearing.sin()).clamp(PLAN_CELL_M, span - PLAN_CELL_M),
            (iy + 7000.0 * bearing.cos()).clamp(PLAN_CELL_M, span - PLAN_CELL_M),
        );
        let depot = at(depot_xy.0, depot_xy.1);
        if grid.cell(depot).is_none_or(|c| c.blocked) || depot == target {
            continue;
        }
        let request = MissionRequest {
            start: depot,
            target,
            wind,
            cruise_mps: 15.0,
            max_gust_mps: 22.0,
            gust_mps: result.gust_mps,
            battery_capacity_seconds: 3600.0,
            battery_pct: 100.0,
            reserve_pct: 20.0,
            task_seconds: 300.0,
            climb_mps: 3.0,
        };
        match plan_mission(&grid, &request) {
            Ok(mission) => {
                if chosen
                    .as_ref()
                    .is_none_or(|(m, ..): &(emberwatch_response::Mission, _, _)| {
                        mission.total_seconds < m.total_seconds
                    })
                {
                    chosen = Some((mission, request, label));
                }
            }
            Err(e) => rejected.push(format!("{label}: {e}")),
        }
    }
    let (mission, request, depot_label) =
        chosen.ok_or_else(|| format!("no feasible staging point ({})", rejected.join("; ")))?;
    let projection = LocalProjection {
        origin_lon_lat: result.frame.origin_lon_lat,
    };
    let blocked = grid.cells().iter().filter(|c| c.blocked).count();
    let smoky = grid
        .cells()
        .iter()
        .filter(|c| c.smoke_penalty >= 1.4)
        .count();
    build_drone_plan(
        context,
        incident,
        &grid,
        projection,
        &mission,
        request.reserve_pct,
        vec![
            format!("Task point on the {side} flank of the predicted 1-hour front, where fireline intensity is lower than at the head."),
            format!("{blocked} cells with ≥{:.0}% predicted burn probability within 60 min are no-fly.", NO_FLY_PROBABILITY * 100.0),
            format!("{smoky} cells with predicted smoke ≥ 35 µg/m³ carry a travel-time penalty."),
            format!("Synthetic staging point 7 km {depot_label} of the ignition, fastest feasible of five candidates; route priced with {:.1} m/s wind toward {:.0}°.", result.transport_mps, result.transport_toward_deg),
            "Configured 15 m/s cruise, 5-minute task and 20% reserve are demonstration values.".into(),
        ],
    )
    .map_err(|e| e.to_string())
}
