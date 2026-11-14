//! Marching-squares contours of a scalar grid → GeoJSON MultiPolygon rings, with
//! hole nesting and Douglas–Peucker simplification.
use std::collections::HashMap;

/// Polygons in grid-node coordinates: outer ring followed by holes. Node (i, j) is
/// the centre of cell (i, j); padding nodes at -1 and w/h close every ring.
pub type Polygon = Vec<Vec<(f64, f64)>>;
type EdgeKey = (isize, isize, u8);

/// 3×3 binomial smoothing, used for display contours only.
pub fn smooth(field: &[f64], w: usize, h: usize) -> Vec<f64> {
    let k = [1.0, 2.0, 1.0];
    let mut out = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            let (mut sum, mut weight) = (0.0, 0.0);
            for (dy, ky) in (-1isize..=1).zip(k) {
                for (dx, kx) in (-1isize..=1).zip(k) {
                    let (nx, ny) = (x as isize + dx, y as isize + dy);
                    if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                        sum += field[ny as usize * w + nx as usize] * kx * ky;
                        weight += kx * ky;
                    }
                }
            }
            out[y * w + x] = sum / weight;
        }
    }
    out
}

pub fn contours(field: &[f64], w: usize, h: usize, threshold: f64) -> Vec<Polygon> {
    let value = |i: isize, j: isize| -> Option<f64> {
        (i >= 0 && j >= 0 && (i as usize) < w && (j as usize) < h)
            .then(|| field[j as usize * w + i as usize])
    };
    let inside = |i: isize, j: isize| value(i, j).is_some_and(|v| v >= threshold);
    // Edge key: (i, j, 0) horizontal from node (i,j) to (i+1,j); (i, j, 1) vertical to (i,j+1).
    let point = |key: (isize, isize, u8)| -> (f64, f64) {
        let (i, j, dir) = key;
        let (a, b) = if dir == 0 {
            ((i, j), (i + 1, j))
        } else {
            ((i, j), (i, j + 1))
        };
        let t = match (value(a.0, a.1), value(b.0, b.1)) {
            (Some(va), Some(vb)) if (vb - va).abs() > 1e-12 => {
                ((threshold - va) / (vb - va)).clamp(0.0, 1.0)
            }
            _ => 0.5,
        };
        (
            a.0 as f64 + t * (b.0 - a.0) as f64,
            a.1 as f64 + t * (b.1 - a.1) as f64,
        )
    };
    let mut links: HashMap<EdgeKey, Vec<EdgeKey>> = HashMap::new();
    let mut add = |a: (isize, isize, u8), b: (isize, isize, u8)| {
        links.entry(a).or_default().push(b);
        links.entry(b).or_default().push(a);
    };
    for j in -1..(h as isize) {
        for i in -1..(w as isize) {
            let (bl, br, tr, tl) = (
                inside(i, j),
                inside(i + 1, j),
                inside(i + 1, j + 1),
                inside(i, j + 1),
            );
            let case = usize::from(bl)
                | usize::from(br) << 1
                | usize::from(tr) << 2
                | usize::from(tl) << 3;
            let bottom = (i, j, 0);
            let right = (i + 1, j, 1);
            let top = (i, j + 1, 0);
            let left = (i, j, 1);
            let centre_inside = || {
                let vals: Vec<f64> = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)]
                    .iter()
                    .map(|&(a, b)| value(a, b).unwrap_or(f64::MIN))
                    .collect();
                vals.iter().sum::<f64>() / 4.0 >= threshold
            };
            match case {
                1 | 14 => add(left, bottom),
                2 | 13 => add(bottom, right),
                3 | 12 => add(left, right),
                4 | 11 => add(right, top),
                6 | 9 => add(bottom, top),
                7 | 8 => add(left, top),
                5 => {
                    if centre_inside() {
                        add(bottom, right);
                        add(left, top);
                    } else {
                        add(left, bottom);
                        add(right, top);
                    }
                }
                10 => {
                    if centre_inside() {
                        add(left, bottom);
                        add(right, top);
                    } else {
                        add(bottom, right);
                        add(left, top);
                    }
                }
                _ => {}
            }
        }
    }
    // Walk closed rings.
    let mut rings: Vec<Vec<(f64, f64)>> = Vec::new();
    // With padding every crossed edge joins exactly two segments, so rings close.
    let mut keys: Vec<_> = links.keys().copied().collect();
    keys.sort();
    let mut visited = std::collections::HashSet::new();
    for start in keys {
        if visited.contains(&start) {
            continue;
        }
        let mut ring = Vec::new();
        let (mut prev, mut current) = (None, start);
        loop {
            visited.insert(current);
            ring.push(point(current));
            let neighbours = &links[&current];
            let next = match prev {
                Some(p) => neighbours.iter().find(|n| **n != p),
                None => neighbours.first(),
            };
            match next {
                Some(n) if *n == start || visited.contains(n) => break,
                Some(n) => {
                    prev = Some(current);
                    current = *n;
                }
                None => break,
            }
        }
        if ring.len() >= 3 {
            rings.push(ring);
        }
    }
    nest(rings)
}

fn contains(ring: &[(f64, f64)], p: (f64, f64)) -> bool {
    let mut inside = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[j]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn nest(rings: Vec<Vec<(f64, f64)>>) -> Vec<Polygon> {
    let depth: Vec<usize> = rings
        .iter()
        .enumerate()
        .map(|(i, r)| {
            rings
                .iter()
                .enumerate()
                .filter(|(k, o)| *k != i && contains(o, r[0]))
                .count()
        })
        .collect();
    let mut polygons: Vec<(usize, Polygon)> = Vec::new();
    for (i, ring) in rings.iter().enumerate() {
        if depth[i].is_multiple_of(2) {
            polygons.push((i, vec![ring.clone()]));
        }
    }
    for (i, ring) in rings.iter().enumerate() {
        if depth[i] % 2 == 1
            && let Some((_, poly)) = polygons
                .iter_mut()
                .filter(|(k, _)| contains(&rings[*k], ring[0]) && depth[*k] + 1 == depth[i])
                .last()
        {
            poly.push(ring.clone());
        }
    }
    polygons.into_iter().map(|(_, p)| p).collect()
}

/// Douglas–Peucker simplification of a closed ring.
pub fn simplify(ring: &[(f64, f64)], tolerance: f64) -> Vec<(f64, f64)> {
    if ring.len() < 8 {
        return ring.to_vec();
    }
    fn dp(points: &[(f64, f64)], tol: f64, keep: &mut [bool], lo: usize, hi: usize) {
        if hi <= lo + 1 {
            return;
        }
        let (a, b) = (points[lo], points[hi]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = dx.hypot(dy).max(1e-12);
        let (mut idx, mut max) = (lo, 0.0);
        for (k, p) in points.iter().enumerate().take(hi).skip(lo + 1) {
            let d = ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / len;
            if d > max {
                max = d;
                idx = k;
            }
        }
        if max > tol {
            keep[idx] = true;
            dp(points, tol, keep, lo, idx);
            dp(points, tol, keep, idx, hi);
        }
    }
    let half = ring.len() / 2;
    let mut keep = vec![false; ring.len()];
    keep[0] = true;
    keep[half] = true;
    keep[ring.len() - 1] = true;
    dp(ring, tolerance, &mut keep, 0, half);
    dp(ring, tolerance, &mut keep, half, ring.len() - 1);
    ring.iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(p, _)| *p)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc(w: usize, r: f64) -> Vec<f64> {
        let c = (w as f64 - 1.0) / 2.0;
        (0..w * w)
            .map(|k| {
                let (x, y) = ((k % w) as f64, (k / w) as f64);
                if (x - c).hypot(y - c) <= r { 1.0 } else { 0.0 }
            })
            .collect()
    }

    #[test]
    fn disc_gives_one_closed_ring_with_expected_area() {
        let field = disc(21, 6.0);
        let polys = contours(&field, 21, 21, 0.5);
        assert_eq!(polys.len(), 1);
        let ring = &polys[0][0];
        let area: f64 = (0..ring.len())
            .map(|i| {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                a.0 * b.1 - b.0 * a.1
            })
            .sum::<f64>()
            .abs()
            / 2.0;
        assert!((area - std::f64::consts::PI * 36.0).abs() < 20.0, "{area}");
    }

    #[test]
    fn annulus_produces_a_hole_and_edge_fields_close() {
        let outer = disc(21, 8.0);
        let inner = disc(21, 3.0);
        let field: Vec<f64> = outer.iter().zip(&inner).map(|(o, i)| o - i).collect();
        let polys = contours(&field, 21, 21, 0.5);
        assert_eq!(polys.len(), 1);
        assert_eq!(polys[0].len(), 2);
        let full = vec![1.0; 25];
        let edge = contours(&full, 5, 5, 0.5);
        assert_eq!(edge.len(), 1);
        assert!(contours(&[0.0; 25], 5, 5, 0.5).is_empty());
        assert!(simplify(&edge[0][0], 0.1).len() <= edge[0][0].len());
    }
}
