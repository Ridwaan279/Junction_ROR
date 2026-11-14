//! Bounded local Cartesian grid. All coordinates and distances are metres.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellPosition {
    pub x: usize,
    pub y: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub fuel: f64,
    pub humidity: f64,
    pub elevation_m: f64,
    pub blocked: bool,
    pub smoke_penalty: f64,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            fuel: 1.0,
            humidity: 0.0,
            elevation_m: 0.0,
            blocked: false,
            smoke_penalty: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) cell_size_m: f64,
    pub(crate) cells: Vec<Cell>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComputeError {
    InvalidInput(String),
    Unreachable,
    InsufficientBattery,
    Capacity,
    Worker(String),
}

impl std::fmt::Display for ComputeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ComputeError {}

impl Grid {
    pub fn new(
        width: usize,
        height: usize,
        cell_size_m: f64,
        cells: Vec<Cell>,
    ) -> Result<Self, ComputeError> {
        if width == 0
            || height == 0
            || width.checked_mul(height) != Some(cells.len())
            || cells.len() > 40_000
            || !cell_size_m.is_finite()
            || cell_size_m <= 0.0
            || width as f64 * cell_size_m > 60_000.0
            || height as f64 * cell_size_m > 60_000.0
        {
            return Err(ComputeError::InvalidInput(
                "grid must fit a 60 km box and 40000 cells".into(),
            ));
        }
        if cells.iter().any(|c| {
            !c.fuel.is_finite()
                || !(0.0..=1.0).contains(&c.fuel)
                || !c.humidity.is_finite()
                || !(0.0..=1.0).contains(&c.humidity)
                || !c.elevation_m.is_finite()
                || !(-500.0..=9000.0).contains(&c.elevation_m)
                || !c.smoke_penalty.is_finite()
                || !(0.0..=100.0).contains(&c.smoke_penalty)
        }) {
            return Err(ComputeError::InvalidInput(
                "invalid fuel, humidity, elevation or smoke".into(),
            ));
        }
        Ok(Self {
            width,
            height,
            cell_size_m,
            cells,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn cell_size_m(&self) -> f64 {
        self.cell_size_m
    }
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }
    pub fn index(&self, p: CellPosition) -> Option<usize> {
        (p.x < self.width && p.y < self.height).then(|| p.y * self.width + p.x)
    }
    pub fn position(&self, index: usize) -> CellPosition {
        CellPosition {
            x: index % self.width,
            y: index / self.width,
        }
    }
    pub fn cell(&self, p: CellPosition) -> Option<&Cell> {
        self.index(p).map(|i| &self.cells[i])
    }
    pub fn set_blocked(&mut self, p: CellPosition, blocked: bool) -> Result<(), ComputeError> {
        let i = self
            .index(p)
            .ok_or_else(|| ComputeError::InvalidInput("cell outside grid".into()))?;
        self.cells[i].blocked = blocked;
        Ok(())
    }
    pub(crate) fn neighbors(&self, p: CellPosition, diagonal: bool) -> Vec<CellPosition> {
        let mut result = Vec::with_capacity(8);
        for dy in -1_isize..=1 {
            for dx in -1_isize..=1 {
                if (dx == 0 && dy == 0) || (!diagonal && dx != 0 && dy != 0) {
                    continue;
                }
                let x = p.x as isize + dx;
                let y = p.y as isize + dy;
                if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
                    result.push(CellPosition {
                        x: x as usize,
                        y: y as usize,
                    });
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_bounded_grid_in_metres() {
        assert!(Grid::new(120, 120, 500.0, vec![Cell::default(); 14400]).is_ok());
    }

    #[test]
    fn rejects_oversize_nonfinite_and_malformed_cells() {
        assert!(Grid::new(121, 120, 500.0, vec![Cell::default(); 14520]).is_err());
        assert!(Grid::new(1, 1, f64::NAN, vec![Cell::default()]).is_err());
        assert!(Grid::new(2, 2, 500.0, vec![Cell::default()]).is_err());
        let bad = Cell {
            humidity: -0.1,
            ..Cell::default()
        };
        assert!(Grid::new(1, 1, 500.0, vec![bad]).is_err());
    }

    #[test]
    fn untrusted_out_of_bounds_position_cannot_overflow_index() {
        let grid = Grid::new(2, 2, 500.0, vec![Cell::default(); 4]).unwrap();
        assert_eq!(
            grid.index(CellPosition {
                x: usize::MAX,
                y: usize::MAX
            }),
            None
        );
    }
}
