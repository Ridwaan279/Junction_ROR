use crate::grid::{CellPosition, ComputeError, Grid};
use crate::wind::WindVector;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(Clone, Debug)]
pub struct MissionRequest {
    pub start: CellPosition,
    pub target: CellPosition,
    pub wind: WindVector,
    pub cruise_mps: f64,
    pub max_gust_mps: f64,
    pub gust_mps: Option<f64>,
    pub battery_capacity_seconds: f64,
    pub battery_pct: f64,
    pub reserve_pct: f64,
    pub task_seconds: f64,
    pub climb_mps: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub cells: Vec<CellPosition>,
    pub distance_m: f64,
    pub duration_seconds: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mission {
    pub outbound: Route,
    pub returning: Route,
    pub total_seconds: f64,
    pub battery_after_pct: f64,
}

pub fn plan_mission(grid: &Grid, request: &MissionRequest) -> Result<Mission, ComputeError> {
    validate_request(request)?;
    let outbound = route(grid, request.start, request.target, request)?;
    let returning = route(grid, request.target, request.start, request)?;
    let total_seconds =
        outbound.duration_seconds + request.task_seconds + returning.duration_seconds;
    let battery_after_pct =
        request.battery_pct - 100.0 * total_seconds / request.battery_capacity_seconds;
    if battery_after_pct < request.reserve_pct {
        return Err(ComputeError::InsufficientBattery);
    }
    Ok(Mission {
        outbound,
        returning,
        total_seconds,
        battery_after_pct,
    })
}

fn validate_request(request: &MissionRequest) -> Result<(), ComputeError> {
    request.wind.validate()?;
    if !request.cruise_mps.is_finite()
        || !(1.0..=100.0).contains(&request.cruise_mps)
        || !request.battery_capacity_seconds.is_finite()
        || request.battery_capacity_seconds <= 0.0
        || !request.battery_pct.is_finite()
        || !(0.0..=100.0).contains(&request.battery_pct)
        || !request.reserve_pct.is_finite()
        || !(0.0..=100.0).contains(&request.reserve_pct)
        || !request.task_seconds.is_finite()
        || !(0.0..=86400.0).contains(&request.task_seconds)
        || !request.climb_mps.is_finite()
        || !(0.1..=30.0).contains(&request.climb_mps)
        || !request.max_gust_mps.is_finite()
        || !(0.0..=100.0).contains(&request.max_gust_mps)
        || request
            .gust_mps
            .is_some_and(|g| !g.is_finite() || g < 0.0 || g > request.max_gust_mps)
    {
        return Err(ComputeError::InvalidInput(
            "invalid mission or gust exceeds configured limit".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Frontier {
    index: usize,
    score: f64,
}
impl PartialEq for Frontier {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.score == other.score
    }
}
impl Eq for Frontier {}
impl PartialOrd for Frontier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Frontier {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .total_cmp(&self.score)
            .then_with(|| other.index.cmp(&self.index))
    }
}

fn edge(
    grid: &Grid,
    a: CellPosition,
    b: CellPosition,
    request: &MissionRequest,
) -> Option<(f64, f64)> {
    let from = grid.cell(a)?;
    let to = grid.cell(b)?;
    if from.blocked || to.blocked {
        return None;
    }
    let dx = b.x as f64 - a.x as f64;
    let dy = b.y as f64 - a.y as f64;
    if dx.abs() > 1.0 || dy.abs() > 1.0 || (dx == 0.0 && dy == 0.0) {
        return None;
    }
    if dx != 0.0
        && dy != 0.0
        && (grid.cell(CellPosition { x: a.x, y: b.y })?.blocked
            || grid.cell(CellPosition { x: b.x, y: a.y })?.blocked)
    {
        return None;
    }
    let length = dx.hypot(dy);
    let along = (request.wind.east_mps * dx + request.wind.north_mps * dy) / length;
    let cross = (request.wind.east_mps * dy - request.wind.north_mps * dx) / length;
    // A drone must cancel crosswind to follow the edge. Insufficient airspeed
    // makes this directed edge unreachable; no minimum-speed clamp hides it.
    if cross.abs() >= request.cruise_mps {
        return None;
    }
    let ground_speed = (request.cruise_mps.powi(2) - cross.powi(2)).sqrt() + along;
    if ground_speed <= 0.0 {
        return None;
    }
    let distance = length * grid.cell_size_m;
    let climb_seconds = (to.elevation_m - from.elevation_m).max(0.0) / request.climb_mps;
    let duration = distance / ground_speed * (1.0 + to.smoke_penalty) + climb_seconds;
    Some((distance, duration))
}

fn route(
    grid: &Grid,
    start: CellPosition,
    target: CellPosition,
    request: &MissionRequest,
) -> Result<Route, ComputeError> {
    let start_index = grid
        .index(start)
        .ok_or_else(|| ComputeError::InvalidInput("start outside grid".into()))?;
    let target_index = grid
        .index(target)
        .ok_or_else(|| ComputeError::InvalidInput("target outside grid".into()))?;
    if grid.cells[start_index].blocked || grid.cells[target_index].blocked {
        return Err(ComputeError::Unreachable);
    }
    let mut cost = vec![f64::INFINITY; grid.cells.len()];
    let mut parent = vec![None; grid.cells.len()];
    let mut frontier = BinaryHeap::new();
    cost[start_index] = 0.0;
    frontier.push(Frontier {
        index: start_index,
        score: 0.0,
    });
    while let Some(current) = frontier.pop() {
        if current.index == target_index {
            let mut path = vec![target];
            let mut index = target_index;
            while index != start_index {
                index = parent[index].ok_or(ComputeError::Unreachable)?;
                path.push(grid.position(index));
            }
            path.reverse();
            let distance_m = path
                .windows(2)
                .map(|p| edge(grid, p[0], p[1], request).expect("validated path").0)
                .sum();
            return Ok(Route {
                cells: path,
                distance_m,
                duration_seconds: cost[target_index],
            });
        }
        let position = grid.position(current.index);
        for neighbor in grid.neighbors(position, true) {
            let Some((_, seconds)) = edge(grid, position, neighbor, request) else {
                continue;
            };
            let index = grid.index(neighbor).expect("bounded neighbor");
            let candidate = cost[current.index] + seconds;
            if candidate < cost[index] {
                cost[index] = candidate;
                parent[index] = Some(current.index);
                let dx = neighbor.x as f64 - target.x as f64;
                let dy = neighbor.y as f64 - target.y as f64;
                let optimistic = dx.hypot(dy) * grid.cell_size_m
                    / (request.cruise_mps + request.wind.speed_mps());
                frontier.push(Frontier {
                    index,
                    score: candidate + optimistic,
                });
            }
        }
    }
    Err(ComputeError::Unreachable)
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReplanDecision {
    Keep,
    Replace {
        mission: Mission,
        reasons: Vec<String>,
    },
}

pub fn replan(
    grid: &Grid,
    request: &MissionRequest,
    previous: &Mission,
    minimum_improvement_pct: f64,
) -> Result<ReplanDecision, ComputeError> {
    if !minimum_improvement_pct.is_finite() || !(0.0..=100.0).contains(&minimum_improvement_pct) {
        return Err(ComputeError::InvalidInput(
            "invalid replan threshold".into(),
        ));
    }
    let hazard = [&previous.outbound, &previous.returning]
        .iter()
        .any(|route| {
            route
                .cells
                .iter()
                .any(|&p| grid.cell(p).is_none_or(|c| c.blocked))
                || route
                    .cells
                    .windows(2)
                    .any(|pair| edge(grid, pair[0], pair[1], request).is_none())
        });
    let mission = plan_mission(grid, request)?;
    let improvement = if previous.total_seconds > 0.0 {
        100.0 * (previous.total_seconds - mission.total_seconds) / previous.total_seconds
    } else {
        0.0
    };
    if hazard || improvement >= minimum_improvement_pct && mission != *previous {
        let reason = if hazard {
            "New hard hazard invalidates the previous route".into()
        } else {
            format!("Wind/smoke travel-time improvement {improvement:.1}% exceeds hysteresis")
        };
        Ok(ReplanDecision::Replace {
            mission,
            reasons: vec![reason],
        })
    } else {
        Ok(ReplanDecision::Keep)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Cell;
    fn request() -> MissionRequest {
        MissionRequest {
            start: CellPosition { x: 0, y: 1 },
            target: CellPosition { x: 4, y: 1 },
            wind: WindVector::default(),
            cruise_mps: 10.0,
            max_gust_mps: 25.0,
            gust_mps: Some(10.0),
            battery_capacity_seconds: 2000.0,
            battery_pct: 100.0,
            reserve_pct: 20.0,
            task_seconds: 60.0,
            climb_mps: 3.0,
        }
    }
    #[test]
    fn a_star_routes_around_obstacles_without_crossing_corners() {
        let mut grid = Grid::new(5, 3, 500.0, vec![Cell::default(); 15]).unwrap();
        grid.set_blocked(CellPosition { x: 2, y: 1 }, true).unwrap();
        let mission = plan_mission(&grid, &request()).unwrap();
        for route in [&mission.outbound, &mission.returning] {
            assert!(!route.cells.contains(&CellPosition { x: 2, y: 1 }));
            for edge in route.cells.windows(2) {
                if edge[0].x != edge[1].x && edge[0].y != edge[1].y {
                    assert!(
                        !grid
                            .cell(CellPosition {
                                x: edge[0].x,
                                y: edge[1].y
                            })
                            .unwrap()
                            .blocked
                    );
                    assert!(
                        !grid
                            .cell(CellPosition {
                                x: edge[1].x,
                                y: edge[0].y
                            })
                            .unwrap()
                            .blocked
                    );
                }
            }
        }
        assert!(mission.outbound.distance_m > 2000.0);
        assert!(mission.battery_after_pct >= 20.0);
    }
    #[test]
    fn checks_return_leg_and_reserve_not_only_outbound() {
        let grid = Grid::new(5, 3, 500.0, vec![Cell::default(); 15]).unwrap();
        let req = MissionRequest {
            battery_capacity_seconds: 400.0,
            task_seconds: 0.0,
            ..request()
        };
        assert_eq!(
            plan_mission(&grid, &req),
            Err(ComputeError::InsufficientBattery)
        );
    }
    #[test]
    fn blocked_mission_and_diagonal_corner_escape_are_unreachable() {
        let mut grid = Grid::new(2, 2, 500.0, vec![Cell::default(); 4]).unwrap();
        grid.set_blocked(CellPosition { x: 1, y: 0 }, true).unwrap();
        grid.set_blocked(CellPosition { x: 0, y: 1 }, true).unwrap();
        let req = MissionRequest {
            start: CellPosition { x: 0, y: 0 },
            target: CellPosition { x: 1, y: 1 },
            ..request()
        };
        assert_eq!(plan_mission(&grid, &req), Err(ComputeError::Unreachable));
    }
    #[test]
    fn return_cost_uses_headwind_instead_of_reversing_outbound_cost() {
        let grid = Grid::new(5, 3, 500.0, vec![Cell::default(); 15]).unwrap();
        let mission = plan_mission(
            &grid,
            &MissionRequest {
                wind: WindVector {
                    east_mps: 5.0,
                    north_mps: 0.0,
                },
                ..request()
            },
        )
        .unwrap();
        assert!(mission.returning.duration_seconds > mission.outbound.duration_seconds);
    }
    #[test]
    fn a_hard_hazard_bypasses_improvement_hysteresis() {
        let mut grid = Grid::new(5, 3, 500.0, vec![Cell::default(); 15]).unwrap();
        let previous = plan_mission(&grid, &request()).unwrap();
        grid.set_blocked(CellPosition { x: 2, y: 1 }, true).unwrap();
        let ReplanDecision::Replace { mission, reasons } =
            replan(&grid, &request(), &previous, 100.0).unwrap()
        else {
            panic!("hard hazard must reroute")
        };
        assert!(
            !mission
                .outbound
                .cells
                .contains(&CellPosition { x: 2, y: 1 })
        );
        assert!(reasons.iter().any(|r| r.contains("hard hazard")));
    }

    #[test]
    fn diagonal_hard_hazard_invalidates_adjacent_route_edge() {
        let mut grid = Grid::new(2, 2, 500.0, vec![Cell::default(); 4]).unwrap();
        let req = MissionRequest {
            start: CellPosition { x: 0, y: 0 },
            target: CellPosition { x: 1, y: 1 },
            ..request()
        };
        let previous = plan_mission(&grid, &req).unwrap();
        grid.set_blocked(CellPosition { x: 1, y: 0 }, true).unwrap();
        assert!(matches!(
            replan(&grid, &req, &previous, 100.0),
            Ok(ReplanDecision::Replace { .. })
        ));
    }

    #[test]
    fn gust_and_impossible_headwind_reject_mission_and_soft_changes_do_not_chatter() {
        let grid = Grid::new(5, 3, 500.0, vec![Cell::default(); 15]).unwrap();
        assert!(
            plan_mission(
                &grid,
                &MissionRequest {
                    gust_mps: Some(30.0),
                    ..request()
                }
            )
            .is_err()
        );
        assert_eq!(
            plan_mission(
                &grid,
                &MissionRequest {
                    wind: WindVector {
                        east_mps: 20.0,
                        north_mps: 0.0
                    },
                    ..request()
                }
            ),
            Err(ComputeError::Unreachable)
        );
        let prior = plan_mission(&grid, &request()).unwrap();
        let req = MissionRequest {
            wind: WindVector {
                east_mps: 0.01,
                north_mps: 0.0,
            },
            ..request()
        };
        assert_eq!(
            replan(&grid, &req, &prior, 5.0).unwrap(),
            ReplanDecision::Keep
        );
    }
}
