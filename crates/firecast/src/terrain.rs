//! Digital elevation model on the local grid, slope and aspect by Horn's (1981)
//! third-order finite difference.
use crate::geo::LocalFrame;
use serde::Serialize;

/// Elevation (m) on a regular local grid: row-major, row 0 = south edge,
/// cell centres at ((x + 0.5) * cell, (y + 0.5) * cell) from the frame origin.
#[derive(Clone, Debug, PartialEq)]
pub struct Dem {
    pub width: usize,
    pub height: usize,
    pub cell_m: f64,
    pub elevation_m: Vec<f32>,
    pub frame: LocalFrame,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct SlopeAt {
    /// rise / run
    pub tan: f64,
    /// Compass bearing (radians) pointing uphill. Undefined (0) on flat cells.
    pub upslope: f64,
}

impl Dem {
    pub fn at(&self, x: usize, y: usize) -> f64 {
        f64::from(self.elevation_m[y * self.width + x])
    }
    fn clamped(&self, x: isize, y: isize) -> f64 {
        let x = x.clamp(0, self.width as isize - 1) as usize;
        let y = y.clamp(0, self.height as isize - 1) as usize;
        self.at(x, y)
    }
    pub fn slope(&self, x: usize, y: usize) -> SlopeAt {
        let (x, y) = (x as isize, y as isize);
        let z = |dx: isize, dy: isize| self.clamped(x + dx, y + dy);
        // Rows increase northward, so "north" neighbours are y + 1.
        let dzdx = ((z(1, 1) + 2.0 * z(1, 0) + z(1, -1)) - (z(-1, 1) + 2.0 * z(-1, 0) + z(-1, -1)))
            / (8.0 * self.cell_m);
        let dzdy = ((z(-1, 1) + 2.0 * z(0, 1) + z(1, 1)) - (z(-1, -1) + 2.0 * z(0, -1) + z(1, -1)))
            / (8.0 * self.cell_m);
        let tan = dzdx.hypot(dzdy);
        let upslope = if tan > 1e-9 {
            dzdx.atan2(dzdy).rem_euclid(std::f64::consts::TAU)
        } else {
            0.0
        };
        SlopeAt { tan, upslope }
    }
    pub fn relief_m(&self) -> f64 {
        let (lo, hi) = self
            .elevation_m
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
        f64::from(hi - lo)
    }
}

/// Bilinear resample of a coarse lon/lat-sampled elevation grid onto the target grid.
/// `coarse` holds `n × n` samples at cell centres spanning the same extent.
pub fn resample(coarse: &[f64], n: usize, target_width: usize, target_height: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(target_width * target_height);
    for y in 0..target_height {
        for x in 0..target_width {
            let fx = (x as f64 + 0.5) / target_width as f64 * n as f64 - 0.5;
            let fy = (y as f64 + 0.5) / target_height as f64 * n as f64 - 0.5;
            let (x0, y0) = (
                fx.floor().clamp(0.0, (n - 1) as f64),
                fy.floor().clamp(0.0, (n - 1) as f64),
            );
            let (x1, y1) = (
                (x0 + 1.0).min((n - 1) as f64),
                (y0 + 1.0).min((n - 1) as f64),
            );
            let (tx, ty) = ((fx - x0).clamp(0.0, 1.0), (fy - y0).clamp(0.0, 1.0));
            let v = |xx: f64, yy: f64| coarse[yy as usize * n + xx as usize];
            let top = v(x0, y1) * (1.0 - tx) + v(x1, y1) * tx;
            let bottom = v(x0, y0) * (1.0 - tx) + v(x1, y0) * tx;
            out.push((bottom * (1.0 - ty) + top * ty) as f32);
        }
    }
    out
}

/// Deterministic synthetic relief (labelled synthetic wherever used): a NE–SW ridge
/// system with a valley, roughly Portuguese central-mountain scale.
pub fn synthetic(frame: LocalFrame, width: usize, height: usize, cell_m: f64) -> Dem {
    let mut elevation_m = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let (e, n) = ((x as f64 + 0.5) * cell_m, (y as f64 + 0.5) * cell_m);
            let along = (e + n) / std::f64::consts::SQRT_2;
            let across = (n - e) / std::f64::consts::SQRT_2;
            let z = 320.0
                + 180.0 * (across / 2600.0).sin()
                + 60.0 * (along / 1900.0).cos()
                + 25.0 * ((e / 700.0).sin() * (n / 900.0).cos());
            elevation_m.push(z.max(5.0) as f32);
        }
    }
    Dem {
        width,
        height,
        cell_m,
        elevation_m,
        frame,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(rise_per_cell_east: f64) -> Dem {
        let mut elevation_m = Vec::new();
        for _y in 0..5 {
            for x in 0..5 {
                elevation_m.push((x as f64 * rise_per_cell_east) as f32);
            }
        }
        Dem {
            width: 5,
            height: 5,
            cell_m: 100.0,
            elevation_m,
            frame: LocalFrame {
                origin_lon_lat: [-8.4, 40.7],
            },
        }
    }

    #[test]
    fn horn_slope_of_an_eastward_ramp_points_east() {
        let dem = ramp(50.0);
        let s = dem.slope(2, 2);
        assert!((s.tan - 0.5).abs() < 1e-9);
        assert!((s.upslope.to_degrees() - 90.0).abs() < 1e-6);
        assert_eq!(ramp(0.0).slope(2, 2).tan, 0.0);
    }

    #[test]
    fn resample_preserves_constant_and_linear_fields() {
        let coarse = vec![10.0; 9];
        assert!(
            resample(&coarse, 3, 6, 6)
                .iter()
                .all(|v| (*v - 10.0).abs() < 1e-6)
        );
        let syn = synthetic(
            LocalFrame {
                origin_lon_lat: [-8.4, 40.7],
            },
            20,
            20,
            120.0,
        );
        assert!(syn.relief_m() > 50.0);
    }
}
