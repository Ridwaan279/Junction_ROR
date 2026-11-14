//! Ensemble minimum-travel-time fire growth.
//!
//! Each cell carries a Rothermel head rate, head direction and Anderson (1983)
//! ellipse eccentricity computed from its fuel, slope and the hourly wind. Fire
//! arrival times follow the shortest-path (Dijkstra) formulation of fire growth
//! (Finney 2002, minimum travel time), with a 16-neighbour stencil to limit grid
//! anisotropy. An ensemble perturbs wind, fuel moisture and the ignition point
//! within its reported uncertainty, so outputs are burn probabilities.
use crate::fuel::LandCover;
use crate::moisture::Moisture;
use crate::rothermel::{FuelBed, Spread, fuel_bed, spread};
use crate::terrain::SlopeAt;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Static landscape on the spread grid (row-major, row 0 south).
#[derive(Clone, Debug)]
pub struct Landscape {
    pub width: usize,
    pub height: usize,
    pub cell_m: f64,
    pub cover: Vec<LandCover>,
    pub slope: Vec<SlopeAt>,
}

/// Conditions for one simulated hour.
#[derive(Clone, Debug)]
pub struct HourConditions {
    /// 10 m wind per cell (east, north), m/s.
    pub wind: Vec<(f32, f32)>,
    pub moisture: Moisture,
}

#[derive(Clone, Debug)]
pub struct EnsembleConfig {
    pub members: u32,
    pub seed: u64,
    pub horizon_s: f64,
    /// Ignition in local metres.
    pub ignition: (f64, f64),
    /// One-sigma ignition location uncertainty, metres.
    pub ignition_sigma_m: f64,
    pub wind_speed_log_sigma: f64,
    pub wind_dir_sigma_deg: f64,
    pub dead_moisture_sigma_pct: f64,
    pub live_moisture_sigma_pct: f64,
}

impl EnsembleConfig {
    pub fn new(
        ignition: (f64, f64),
        ignition_sigma_m: f64,
        horizon_s: f64,
        members: u32,
        seed: u64,
    ) -> Self {
        Self {
            members,
            seed,
            horizon_s,
            ignition,
            ignition_sigma_m,
            wind_speed_log_sigma: 0.2,
            wind_dir_sigma_deg: 12.0,
            dead_moisture_sigma_pct: 1.0,
            live_moisture_sigma_pct: 10.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Perturbation {
    pub wind_factor: f64,
    pub wind_dir_offset_deg: f64,
    pub dead_moisture_offset_pct: f64,
    pub live_moisture_offset_pct: f64,
    pub ignition: (f64, f64),
}

#[derive(Clone, Debug)]
pub struct MemberResult {
    pub perturbation: Perturbation,
    /// Arrival time in seconds; infinity = not reached within the horizon.
    pub arrival: Vec<f32>,
    pub ignition_cell: usize,
}

#[derive(Clone, Debug)]
pub struct EnsembleResult {
    pub members: Vec<MemberResult>,
    pub width: usize,
    pub height: usize,
    pub cell_m: f64,
    pub horizon_s: f64,
    pub edge_reached: bool,
}

impl EnsembleResult {
    /// Fraction of members that burned each cell by `t` seconds.
    pub fn burn_probability(&self, t: f64) -> Vec<f64> {
        let n = self.members.len().max(1) as f64;
        let mut p = vec![0.0; self.width * self.height];
        for m in &self.members {
            for (i, a) in m.arrival.iter().enumerate() {
                if f64::from(*a) <= t {
                    p[i] += 1.0;
                }
            }
        }
        p.iter_mut().for_each(|v| *v /= n);
        p
    }
    /// Burned area (ha) per member at `t`, sorted ascending.
    pub fn areas_ha(&self, t: f64) -> Vec<f64> {
        let cell_ha = self.cell_m * self.cell_m / 10_000.0;
        let mut a: Vec<f64> = self
            .members
            .iter()
            .map(|m| m.arrival.iter().filter(|a| f64::from(**a) <= t).count() as f64 * cell_ha)
            .collect();
        a.sort_by(f64::total_cmp);
        a
    }
}

/// Seeded xorshift64* generator with Box–Muller normals.
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9e37_79b9_7f4a_7c15
        } else {
            seed
        })
    }
    pub fn next_f64(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let v = self.0.wrapping_mul(0x2545_f491_4f6c_dd1d);
        ((v >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }
    pub fn normal(&mut self) -> f64 {
        let (u1, u2) = (self.next_f64(), self.next_f64());
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

#[derive(Copy, Clone, PartialEq)]
struct Node(f64, usize);
impl Eq for Node {}
impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .0
            .total_cmp(&self.0)
            .then_with(|| other.1.cmp(&self.1))
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

const STENCIL: [(isize, isize); 16] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
    (1, 2),
    (2, 1),
    (-1, 2),
    (-2, 1),
    (1, -2),
    (2, -1),
    (-1, -2),
    (-2, -1),
];

pub fn perturbations(config: &EnsembleConfig) -> Vec<Perturbation> {
    let mut rng = Rng::new(config.seed);
    (0..config.members)
        .map(|i| {
            if i == 0 {
                // Member 0 is the central (unperturbed) estimate.
                return Perturbation {
                    wind_factor: 1.0,
                    wind_dir_offset_deg: 0.0,
                    dead_moisture_offset_pct: 0.0,
                    live_moisture_offset_pct: 0.0,
                    ignition: config.ignition,
                };
            }
            let r = (rng.normal() * config.ignition_sigma_m).clamp(
                -2.0 * config.ignition_sigma_m,
                2.0 * config.ignition_sigma_m,
            );
            let angle = rng.next_f64() * std::f64::consts::TAU;
            Perturbation {
                wind_factor: (rng.normal() * config.wind_speed_log_sigma).exp(),
                wind_dir_offset_deg: (rng.normal() * config.wind_dir_sigma_deg).clamp(-35.0, 35.0),
                dead_moisture_offset_pct: (rng.normal() * config.dead_moisture_sigma_pct)
                    .clamp(-2.5, 2.5),
                live_moisture_offset_pct: (rng.normal() * config.live_moisture_sigma_pct)
                    .clamp(-25.0, 25.0),
                ignition: (
                    config.ignition.0 + r * angle.sin(),
                    config.ignition.1 + r * angle.cos(),
                ),
            }
        })
        .collect()
}

/// Fuel beds per hour and land-cover class for a member's moisture.
pub fn beds_for(hours: &[HourConditions], p: &Perturbation) -> Vec<Vec<Option<FuelBed>>> {
    hours
        .iter()
        .map(|h| {
            let m = h
                .moisture
                .offset(p.dead_moisture_offset_pct, p.live_moisture_offset_pct);
            LandCover::ALL
                .iter()
                .map(|c| c.fuel().and_then(|f| fuel_bed(f, &m)))
                .collect()
        })
        .collect()
}

/// Spread state of one cell in one hour for a member.
pub fn cell_spread(
    landscape: &Landscape,
    hours: &[HourConditions],
    beds: &[Vec<Option<FuelBed>>],
    p: &Perturbation,
    hour: usize,
    cell: usize,
) -> Option<Spread> {
    let cover = landscape.cover[cell];
    let fuel = cover.fuel()?;
    let bed = beds[hour][usize::from(cover.index())]?;
    let (e, n) = hours[hour].wind[cell];
    let (e, n) = (f64::from(e) * p.wind_factor, f64::from(n) * p.wind_factor);
    let speed10 = e.hypot(n);
    let toward = e.atan2(n) + p.wind_dir_offset_deg.to_radians();
    // 10 m → 20 ft (6.1 m) open wind ≈ /1.15, then the fuel's wind adjustment factor.
    let midflame = speed10 / 1.15 * fuel.wind_adjustment;
    let slope = landscape.slope[cell];
    let s = spread(&bed, midflame, toward, slope.tan, slope.upslope);
    (s.head_ros_mps > 1e-6).then_some(s)
}

fn burnable(landscape: &Landscape, x: isize, y: isize) -> bool {
    x >= 0
        && y >= 0
        && (x as usize) < landscape.width
        && (y as usize) < landscape.height
        && landscape.cover[y as usize * landscape.width + x as usize] != LandCover::NonBurnable
}

fn nearest_burnable(landscape: &Landscape, x: isize, y: isize) -> Option<usize> {
    for r in 0..=4isize {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs().max(dy.abs()) == r && burnable(landscape, x + dx, y + dy) {
                    return Some((y + dy) as usize * landscape.width + (x + dx) as usize);
                }
            }
        }
    }
    None
}

pub fn run_member(
    landscape: &Landscape,
    hours: &[HourConditions],
    p: Perturbation,
    horizon_s: f64,
) -> Option<MemberResult> {
    let (w, h) = (landscape.width, landscape.height);
    let ix = (p.ignition.0 / landscape.cell_m).floor() as isize;
    let iy = (p.ignition.1 / landscape.cell_m).floor() as isize;
    let start = nearest_burnable(
        landscape,
        ix.clamp(0, w as isize - 1),
        iy.clamp(0, h as isize - 1),
    )?;
    let beds = beds_for(hours, &p);
    let mut cache: Vec<Vec<Option<Option<Spread>>>> = vec![vec![None; w * h]; hours.len()];
    let mut get = |hour: usize, cell: usize| -> Option<Spread> {
        *cache[hour][cell]
            .get_or_insert_with(|| cell_spread(landscape, hours, &beds, &p, hour, cell))
    };
    let mut arrival = vec![f32::INFINITY; w * h];
    let mut best = vec![f64::INFINITY; w * h];
    let mut done = vec![false; w * h];
    let mut heap = BinaryHeap::new();
    best[start] = 0.0;
    heap.push(Node(0.0, start));
    while let Some(Node(t, cell)) = heap.pop() {
        if done[cell] || t > best[cell] {
            continue;
        }
        if t > horizon_s {
            break;
        }
        done[cell] = true;
        arrival[cell] = t as f32;
        let hour = ((t / 3600.0) as usize).min(hours.len() - 1);
        let Some(src) = get(hour, cell) else { continue };
        let (x, y) = ((cell % w) as isize, (cell / w) as isize);
        for (dx, dy) in STENCIL {
            let (nx, ny) = (x + dx, y + dy);
            if !burnable(landscape, nx, ny) {
                continue;
            }
            // Do not leak through one-cell barriers.
            let crossing_ok = match (dx.abs(), dy.abs()) {
                (1, 1) => burnable(landscape, x + dx, y) || burnable(landscape, x, y + dy),
                (1, 2) => {
                    burnable(landscape, x, y + dy.signum())
                        && burnable(landscape, x + dx, y + dy.signum())
                }
                (2, 1) => {
                    burnable(landscape, x + dx.signum(), y)
                        && burnable(landscape, x + dx.signum(), y + dy)
                }
                _ => true,
            };
            if !crossing_ok {
                continue;
            }
            let next = ny as usize * w + nx as usize;
            if done[next] {
                continue;
            }
            let Some(dst) = get(hour, next) else { continue };
            let bearing = (dx as f64).atan2(dy as f64);
            let distance = landscape.cell_m * ((dx * dx + dy * dy) as f64).sqrt();
            let r_src = src.ros_at(bearing - src.head_bearing);
            let r_dst = dst.ros_at(bearing - dst.head_bearing);
            if r_src <= 1e-9 || r_dst <= 1e-9 {
                continue;
            }
            let candidate = t + 0.5 * distance * (1.0 / r_src + 1.0 / r_dst);
            if candidate < best[next] {
                best[next] = candidate;
                heap.push(Node(candidate, next));
            }
        }
    }
    Some(MemberResult {
        perturbation: p,
        arrival,
        ignition_cell: start,
    })
}

/// Run the ensemble. Members are independent and seeded, so the result does not
/// depend on thread scheduling.
pub fn run_ensemble(
    landscape: &Landscape,
    hours: &[HourConditions],
    config: &EnsembleConfig,
) -> Result<EnsembleResult, String> {
    if hours.is_empty() || landscape.cover.len() != landscape.width * landscape.height {
        return Err("empty hours or malformed landscape".into());
    }
    if hours.iter().any(|h| h.wind.len() != landscape.cover.len()) {
        return Err("wind grid does not match landscape".into());
    }
    let perturbations = perturbations(config);
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .clamp(1, 4);
    let chunk = perturbations.len().div_ceil(threads).max(1);
    let mut members: Vec<Option<MemberResult>> = vec![None; perturbations.len()];
    std::thread::scope(|scope| {
        let handles: Vec<_> = perturbations
            .chunks(chunk)
            .enumerate()
            .map(|(ci, ps)| {
                scope.spawn(move || {
                    ps.iter()
                        .enumerate()
                        .map(|(i, p)| {
                            (
                                ci * chunk + i,
                                run_member(landscape, hours, *p, config.horizon_s),
                            )
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            for (i, r) in handle.join().unwrap_or_default() {
                members[i] = r;
            }
        }
    });
    let members: Vec<MemberResult> = members.into_iter().flatten().collect();
    if members.is_empty() {
        return Err("ignition is not on burnable fuel".into());
    }
    let (w, h) = (landscape.width, landscape.height);
    let edge_reached = members.iter().any(|m| {
        (0..w).any(|x| m.arrival[x].is_finite() || m.arrival[(h - 1) * w + x].is_finite())
            || (0..h).any(|y| m.arrival[y * w].is_finite() || m.arrival[y * w + w - 1].is_finite())
    });
    Ok(EnsembleResult {
        members,
        width: w,
        height: h,
        cell_m: landscape.cell_m,
        horizon_s: config.horizon_s,
        edge_reached,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(n: usize, cover: LandCover) -> Landscape {
        Landscape {
            width: n,
            height: n,
            cell_m: 100.0,
            cover: vec![cover; n * n],
            slope: vec![
                SlopeAt {
                    tan: 0.0,
                    upslope: 0.0
                };
                n * n
            ],
        }
    }
    fn hours(n: usize, east: f32, north: f32) -> Vec<HourConditions> {
        vec![
            HourConditions {
                wind: vec![(east, north); n * n],
                moisture: Moisture {
                    dead_1h: 0.06,
                    dead_10h: 0.07,
                    dead_100h: 0.08,
                    live: 0.9
                }
            };
            2
        ]
    }

    #[test]
    fn west_wind_grows_an_eastward_ellipse() {
        let land = flat(61, LandCover::Grassland);
        let cfg = EnsembleConfig::new((3050.0, 3050.0), 0.0, 1800.0, 1, 1);
        let r = run_ensemble(&land, &hours(61, 6.0, 0.0), &cfg).unwrap();
        let a = &r.members[0].arrival;
        let row = 30 * 61;
        let east = (31..61).filter(|x| a[row + x].is_finite()).count();
        let west = (0..30).rev().filter(|x| a[row + x].is_finite()).count();
        let north = (31..61).filter(|y| a[y * 61 + 30].is_finite()).count();
        assert!(east > 3 * west.max(1), "east {east} west {west}");
        assert!(east > north, "east {east} north {north}");
        // arrival increases monotonically downwind
        assert!(a[row + 35] < a[row + 40]);
    }

    #[test]
    fn calm_flat_fire_is_nearly_circular() {
        let land = flat(41, LandCover::Grassland);
        // Calm short grass spreads ~1.4 m/min, so look 3 cells out over 6 h.
        let cfg = EnsembleConfig::new((2050.0, 2050.0), 0.0, 6.0 * 3600.0, 1, 1);
        let r = run_ensemble(&land, &hours(41, 0.0, 0.0), &cfg).unwrap();
        let a = &r.members[0].arrival;
        let (e, n) = (a[20 * 41 + 23], a[23 * 41 + 20]);
        assert!(e.is_finite() && n.is_finite());
        assert!((e - n).abs() / e < 0.05, "{e} vs {n}");
    }

    #[test]
    fn barriers_stop_fire_and_ensemble_is_reproducible() {
        let mut land = flat(31, LandCover::Grassland);
        for y in 0..31 {
            land.cover[y * 31 + 20] = LandCover::NonBurnable;
        }
        let cfg = EnsembleConfig::new((1050.0, 1550.0), 100.0, 7200.0, 8, 7);
        let wind = hours(31, 8.0, 0.0);
        let a = run_ensemble(&land, &wind, &cfg).unwrap();
        let b = run_ensemble(&land, &wind, &cfg).unwrap();
        for (ma, mb) in a.members.iter().zip(&b.members) {
            assert_eq!(ma.arrival, mb.arrival);
        }
        for m in &a.members {
            assert!((0..31).all(|y| !m.arrival[y * 31 + 25].is_finite()));
        }
        let p = a.burn_probability(7200.0);
        assert!(p.iter().all(|v| (0.0..=1.0).contains(v)));
        let areas = a.areas_ha(3600.0);
        assert!(areas.windows(2).all(|w| w[0] <= w[1]));
    }
}
