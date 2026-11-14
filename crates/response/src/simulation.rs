use crate::grid::{CellPosition, ComputeError, Grid};
use crate::planner::{Mission, MissionRequest, Route};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DroneState {
    Idle,
    Enroute,
    OnTask,
    Returning,
    Charging,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Telemetry {
    pub state: DroneState,
    pub position_m: [f64; 2],
    pub battery_pct: f64,
    pub simulated: bool,
}

/// Samples absolute run-clock seconds; reconstruct on replay epoch changes.
/// No wall clock or mutable PRNG state participates in the simulation.
pub fn sample_drone(
    grid: &Grid,
    mission: &Mission,
    request: &MissionRequest,
    dispatched_at_seconds: f64,
    at_seconds: f64,
    recharge_pct_per_second: f64,
) -> Result<Telemetry, ComputeError> {
    if !at_seconds.is_finite()
        || !dispatched_at_seconds.is_finite()
        || at_seconds < 0.0
        || dispatched_at_seconds < 0.0
        || !recharge_pct_per_second.is_finite()
        || !(0.0..=10.0).contains(&recharge_pct_per_second)
        || mission.outbound.cells.is_empty()
        || mission.returning.cells.is_empty()
    {
        return Err(ComputeError::InvalidInput(
            "invalid simulation clock, mission or recharge rate".into(),
        ));
    }
    let elapsed = at_seconds - dispatched_at_seconds;
    let outbound_end = mission.outbound.duration_seconds;
    let task_end = outbound_end + request.task_seconds;
    let (state, position_m) = if elapsed < 0.0 {
        (DroneState::Idle, center(grid, request.start))
    } else if elapsed < outbound_end {
        (
            DroneState::Enroute,
            interpolate(grid, &mission.outbound, elapsed / outbound_end),
        )
    } else if elapsed < task_end {
        (DroneState::OnTask, center(grid, request.target))
    } else if elapsed < mission.total_seconds {
        (
            DroneState::Returning,
            interpolate(
                grid,
                &mission.returning,
                (elapsed - task_end) / mission.returning.duration_seconds,
            ),
        )
    } else {
        (DroneState::Charging, center(grid, request.start))
    };
    let used_seconds = elapsed.clamp(0.0, mission.total_seconds);
    let charged = (elapsed - mission.total_seconds).max(0.0) * recharge_pct_per_second;
    let battery_pct =
        (request.battery_pct - used_seconds / request.battery_capacity_seconds * 100.0 + charged)
            .clamp(0.0, 100.0);
    Ok(Telemetry {
        state,
        position_m,
        battery_pct,
        simulated: true,
    })
}

fn center(grid: &Grid, p: CellPosition) -> [f64; 2] {
    [
        (p.x as f64 + 0.5) * grid.cell_size_m,
        (p.y as f64 + 0.5) * grid.cell_size_m,
    ]
}

fn interpolate(grid: &Grid, route: &Route, fraction: f64) -> [f64; 2] {
    let mut remaining_m = route.distance_m * fraction.clamp(0.0, 1.0);
    for edge in route.cells.windows(2) {
        let from = center(grid, edge[0]);
        let to = center(grid, edge[1]);
        let distance_m = (to[0] - from[0]).hypot(to[1] - from[1]);
        if remaining_m <= distance_m {
            let ratio = remaining_m / distance_m;
            return [
                from[0] + (to[0] - from[0]) * ratio,
                from[1] + (to[1] - from[1]) * ratio,
            ];
        }
        remaining_m -= distance_m;
    }
    center(
        grid,
        *route.cells.last().expect("validated nonempty mission"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cell, WindVector, plan_mission};
    #[test]
    fn run_clock_advances_all_mission_states_and_seek_rebuilds_exactly() {
        let grid = Grid::new(2, 1, 500.0, vec![Cell::default(); 2]).unwrap();
        let req = MissionRequest {
            start: CellPosition { x: 0, y: 0 },
            target: CellPosition { x: 1, y: 0 },
            wind: WindVector::default(),
            cruise_mps: 10.0,
            max_gust_mps: 20.0,
            gust_mps: None,
            battery_capacity_seconds: 1000.0,
            battery_pct: 100.0,
            reserve_pct: 20.0,
            task_seconds: 10.0,
            climb_mps: 3.0,
        };
        let mission = plan_mission(&grid, &req).unwrap();
        let sample = |at| sample_drone(&grid, &mission, &req, 100.0, at, 0.1).unwrap();
        assert_eq!(sample(99.0).state, DroneState::Idle);
        assert_eq!(sample(125.0).state, DroneState::Enroute);
        assert_eq!(sample(125.0).position_m, [500.0, 250.0]);
        assert_eq!(sample(150.0).state, DroneState::OnTask);
        assert_eq!(sample(160.0).state, DroneState::Returning);
        assert_eq!(sample(210.0).state, DroneState::Charging);
        assert_eq!(sample(210.0).battery_pct, 89.0);
        assert_eq!(sample(320.0).battery_pct, 100.0);
        let before_seek = sample(125.0);
        let _after = sample(250.0);
        assert_eq!(sample(125.0), before_seek);
    }
}
