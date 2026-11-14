//! Uncalibrated cellular illustration, never a measured perimeter or operational forecast.
use crate::grid::{CellPosition, ComputeError, Grid};
use crate::wind::WindVector;

#[derive(Clone, Debug)]
pub struct SpreadInput {
    pub seed: u64,
    pub ignitions: Vec<CellPosition>,
    pub steps: u32,
    pub step_seconds: u32,
    pub base_probability: f64,
    pub wind: WindVector,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpreadResult {
    /// First arrival per cell; null means not reached in the simulated horizon.
    pub arrival_seconds: Vec<Option<u32>>,
    pub burned_cells: Vec<CellPosition>,
    pub seed: u64,
    pub horizon_seconds: u32,
}

pub fn simulate_spread(grid: &Grid, input: &SpreadInput) -> Result<SpreadResult, ComputeError> {
    input.wind.validate()?;
    if input.steps > 360
        || input.step_seconds == 0
        || input.step_seconds > 3600
        || !input.base_probability.is_finite()
        || !(0.0..=1.0).contains(&input.base_probability)
        || input.ignitions.is_empty()
        || input.ignitions.len() > grid.cells.len()
    {
        return Err(ComputeError::InvalidInput(
            "invalid or unbounded spread input".into(),
        ));
    }
    let mut arrival = vec![None; grid.cells.len()];
    for &position in &input.ignitions {
        let index = grid
            .index(position)
            .ok_or_else(|| ComputeError::InvalidInput("ignition outside grid".into()))?;
        if grid.cells[index].blocked || grid.cells[index].fuel == 0.0 {
            return Err(ComputeError::InvalidInput(
                "ignition must have burnable fuel".into(),
            ));
        }
        arrival[index] = Some(0);
    }
    let mut random = if input.seed == 0 {
        0x9e37_79b9_7f4a_7c15
    } else {
        input.seed
    };
    for step in 1..=input.steps {
        let previous = arrival.clone();
        for index in 0..previous.len() {
            if previous[index].is_none() {
                continue;
            }
            let position = grid.position(index);
            for neighbor in grid.neighbors(position, false) {
                let next = grid.index(neighbor).expect("bounded neighbor");
                let cell = grid.cells[next];
                if arrival[next].is_some() || cell.blocked || cell.fuel == 0.0 {
                    continue;
                }
                let dx = neighbor.x as f64 - position.x as f64;
                let dy = neighbor.y as f64 - position.y as f64;
                let wind_factor =
                    (1.0 + (input.wind.east_mps * dx + input.wind.north_mps * dy) / 5.0).max(0.0);
                let slope = (cell.elevation_m - grid.cells[index].elevation_m) / grid.cell_size_m;
                let slope_factor = (1.0 + slope).clamp(0.1, 4.0);
                let probability = (input.base_probability
                    * cell.fuel
                    * (1.0 - cell.humidity)
                    * wind_factor
                    * slope_factor)
                    .clamp(0.0, 1.0);
                // Fixed xorshift64 stream; row-major traversal and immutable previous state
                // make the result independent of executor scheduling and wall time.
                random ^= random << 13;
                random ^= random >> 7;
                random ^= random << 17;
                let uniform = (random >> 11) as f64 / ((1_u64 << 53) as f64);
                if uniform < probability {
                    arrival[next] = Some(step * input.step_seconds);
                }
            }
        }
    }
    let burned_cells = arrival
        .iter()
        .enumerate()
        .filter_map(|(i, at)| at.map(|_| grid.position(i)))
        .collect();
    Ok(SpreadResult {
        arrival_seconds: arrival,
        burned_cells,
        seed: input.seed,
        horizon_seconds: input.steps * input.step_seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Cell;
    fn input() -> SpreadInput {
        SpreadInput {
            seed: 42,
            ignitions: vec![CellPosition { x: 2, y: 2 }],
            steps: 1,
            step_seconds: 60,
            base_probability: 0.5,
            wind: WindVector {
                east_mps: 10.0,
                north_mps: 0.0,
            },
        }
    }

    #[test]
    fn seeded_spread_is_reproducible_and_west_wind_pushes_east() {
        let grid = Grid::new(5, 5, 500.0, vec![Cell::default(); 25]).unwrap();
        let result = simulate_spread(&grid, &input()).unwrap();
        assert_eq!(result, simulate_spread(&grid, &input()).unwrap());
        assert_eq!(result.arrival_seconds[13], Some(60)); // east of ignition
        assert_eq!(result.arrival_seconds[11], None); // upwind
        assert_eq!(result.arrival_seconds[12], Some(0));
        assert_eq!(result.horizon_seconds, 60);
    }
    #[test]
    fn a_nonburnable_barrier_cannot_be_crossed() {
        let mut cells = vec![Cell::default(); 5];
        cells[2].fuel = 0.0;
        let grid = Grid::new(5, 1, 500.0, cells).unwrap();
        let config = SpreadInput {
            ignitions: vec![CellPosition { x: 0, y: 0 }],
            steps: 10,
            base_probability: 1.0,
            ..input()
        };
        let result = simulate_spread(&grid, &config).unwrap();
        assert_eq!(
            result.arrival_seconds,
            vec![Some(0), Some(60), None, None, None]
        );
    }
    #[test]
    fn invalid_ignition_and_unbounded_steps_are_rejected() {
        let grid = Grid::new(5, 5, 500.0, vec![Cell::default(); 25]).unwrap();
        assert!(
            simulate_spread(
                &grid,
                &SpreadInput {
                    steps: 10_000,
                    ..input()
                }
            )
            .is_err()
        );
        assert!(
            simulate_spread(
                &grid,
                &SpreadInput {
                    ignitions: vec![CellPosition { x: 5, y: 0 }],
                    ..input()
                }
            )
            .is_err()
        );
        assert!(
            simulate_spread(
                &grid,
                &SpreadInput {
                    base_probability: f64::NAN,
                    ..input()
                }
            )
            .is_err()
        );
    }
}
