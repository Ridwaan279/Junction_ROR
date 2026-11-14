//! Land cover (fuel) raster for the spread grid.
//!
//! Primary source: OpenStreetMap polygons via the Overpass API (landuse/natural
//! tags, rasterised by scanline). Cells without mapped cover take the regional
//! dominant vegetation, which Context.dev report extraction can supply. Elevation
//! at or below sea level is treated as water (non-burnable).
use crate::fuel::LandCover;
use crate::geo::LocalFrame;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct LandCoverGrid {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<LandCover>,
    /// Fraction of cells whose class came from mapped polygons (rest = regional default).
    pub mapped_fraction: f64,
}

impl LandCoverGrid {
    pub fn uniform(width: usize, height: usize, class: LandCover) -> Self {
        Self {
            width,
            height,
            cells: vec![class; width * height],
            mapped_fraction: 0.0,
        }
    }
    pub fn fractions(&self) -> BTreeMap<String, f64> {
        let mut counts: BTreeMap<String, f64> = BTreeMap::new();
        for c in &self.cells {
            *counts.entry(c.label().to_string()).or_default() += 1.0;
        }
        let total = self.cells.len().max(1) as f64;
        counts
            .values_mut()
            .for_each(|v| *v = (*v / total * 1000.0).round() / 1000.0);
        counts
    }
}

/// OSM tags to land cover (None = tag is not a fuel class we use).
pub fn class_from_tags(tags: &Value) -> Option<LandCover> {
    let get = |k: &str| tags.get(k).and_then(Value::as_str).unwrap_or("");
    let (landuse, natural, leaf, genus, species) = (
        get("landuse"),
        get("natural"),
        get("leaf_type"),
        get("genus"),
        get("species"),
    );
    let taxon = format!("{genus} {species}").to_lowercase();
    if taxon.contains("eucalyptus") {
        return Some(LandCover::Eucalyptus);
    }
    if taxon.contains("pinus") {
        return Some(LandCover::MaritimePine);
    }
    if taxon.contains("quercus suber")
        || taxon.contains("quercus rotundifolia")
        || taxon.contains("quercus ilex")
    {
        return Some(LandCover::Montado);
    }
    match (landuse, natural) {
        ("forest", _) | (_, "wood") => Some(match leaf {
            "needleleaved" => LandCover::MaritimePine,
            "broadleaved" => LandCover::Eucalyptus, // dominant broadleaf plantation in central/north PT
            _ => LandCover::MixedForest,
        }),
        (_, "scrub") | (_, "heath") => Some(LandCover::Shrubland),
        ("meadow", _) | ("grass", _) | (_, "grassland") => Some(LandCover::Grassland),
        ("farmland", _)
        | ("vineyard", _)
        | ("orchard", _)
        | ("farmyard", _)
        | ("allotments", _) => Some(LandCover::Agriculture),
        ("residential", _)
        | ("industrial", _)
        | ("commercial", _)
        | ("retail", _)
        | ("quarry", _)
        | ("reservoir", _)
        | ("basin", _)
        | (_, "water")
        | (_, "bare_rock")
        | (_, "beach")
        | (_, "sand")
        | (_, "wetland") => Some(LandCover::NonBurnable),
        _ => None,
    }
}

/// Fill priority when polygons overlap: non-burnable wins, then specific species.
fn priority(class: LandCover) -> u8 {
    match class {
        LandCover::NonBurnable => 9,
        LandCover::Eucalyptus | LandCover::MaritimePine | LandCover::Montado => 7,
        LandCover::Shrubland | LandCover::TallShrubland => 6,
        LandCover::Agriculture | LandCover::Grassland => 5,
        _ => 4,
    }
}

/// Rasterise an Overpass JSON response (`out geom;`) onto the grid.
pub fn rasterise_overpass(
    response: &Value,
    frame: LocalFrame,
    width: usize,
    height: usize,
    cell_m: f64,
    default: LandCover,
) -> LandCoverGrid {
    let mut cells: Vec<Option<LandCover>> = vec![None; width * height];
    let empty = Vec::new();
    for element in response
        .get("elements")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
    {
        let Some(class) = element.get("tags").and_then(class_from_tags) else {
            continue;
        };
        let mut rings: Vec<Vec<(f64, f64)>> = Vec::new();
        let to_ring = |geom: &Value| -> Option<Vec<(f64, f64)>> {
            let pts: Vec<(f64, f64)> = geom
                .as_array()?
                .iter()
                .filter_map(|p| {
                    Some(frame.to_local([p.get("lon")?.as_f64()?, p.get("lat")?.as_f64()?]))
                })
                .collect();
            (pts.len() >= 3).then_some(pts)
        };
        match element.get("type").and_then(Value::as_str) {
            Some("way") => rings.extend(element.get("geometry").and_then(to_ring)),
            Some("relation") => {
                for member in element
                    .get("members")
                    .and_then(Value::as_array)
                    .unwrap_or(&empty)
                {
                    if member.get("role").and_then(Value::as_str) == Some("outer") {
                        rings.extend(member.get("geometry").and_then(to_ring));
                    }
                }
            }
            _ => {}
        }
        for ring in rings {
            fill_ring(&ring, &mut cells, width, height, cell_m, class);
        }
    }
    let mapped = cells.iter().filter(|c| c.is_some()).count();
    LandCoverGrid {
        width,
        height,
        mapped_fraction: mapped as f64 / cells.len().max(1) as f64,
        cells: cells.into_iter().map(|c| c.unwrap_or(default)).collect(),
    }
}

/// Even-odd scanline fill of cell centres inside `ring` (local metres).
fn fill_ring(
    ring: &[(f64, f64)],
    cells: &mut [Option<LandCover>],
    width: usize,
    height: usize,
    cell_m: f64,
    class: LandCover,
) {
    let (min_y, max_y) = ring
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
    let y_start = ((min_y / cell_m - 0.5).ceil().max(0.0)) as usize;
    let y_end = ((max_y / cell_m - 0.5).floor()).min(height as f64 - 1.0);
    if y_end < 0.0 {
        return;
    }
    for y in y_start..=(y_end as usize) {
        let cy = (y as f64 + 0.5) * cell_m;
        let mut xs: Vec<f64> = Vec::new();
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            if (a.1 <= cy && b.1 > cy) || (b.1 <= cy && a.1 > cy) {
                xs.push(a.0 + (cy - a.1) / (b.1 - a.1) * (b.0 - a.0));
            }
        }
        xs.sort_by(f64::total_cmp);
        for pair in xs.chunks(2) {
            if pair.len() < 2 {
                continue;
            }
            let x0 = ((pair[0] / cell_m - 0.5).ceil().max(0.0)) as usize;
            let x1 = (pair[1] / cell_m - 0.5).floor();
            if x1 < 0.0 {
                continue;
            }
            for x in x0..=(x1 as usize).min(width - 1) {
                let cell = &mut cells[y * width + x];
                if cell.is_none_or(|old| priority(class) >= priority(old)) {
                    *cell = Some(class);
                }
            }
        }
    }
}

/// Overpass QL for wildland-relevant polygons in a lon/lat bbox (south, west, north, east).
pub fn overpass_query(bbox_swne: [f64; 4]) -> String {
    let b = format!(
        "{:.5},{:.5},{:.5},{:.5}",
        bbox_swne[0], bbox_swne[1], bbox_swne[2], bbox_swne[3]
    );
    format!(
        "[out:json][timeout:25];(way[\"landuse\"~\"^(forest|meadow|grass|farmland|vineyard|orchard|residential|industrial|commercial|quarry|reservoir)$\"]({b});\
way[\"natural\"~\"^(wood|scrub|heath|grassland|water|bare_rock|wetland)$\"]({b});\
relation[\"landuse\"=\"forest\"]({b});relation[\"natural\"~\"^(wood|water)$\"]({b}););out geom;"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tags_map_to_fuel_classes() {
        assert_eq!(
            class_from_tags(&json!({"landuse":"forest","genus":"Eucalyptus"})),
            Some(LandCover::Eucalyptus)
        );
        assert_eq!(
            class_from_tags(&json!({"natural":"wood","leaf_type":"needleleaved"})),
            Some(LandCover::MaritimePine)
        );
        assert_eq!(
            class_from_tags(&json!({"natural":"scrub"})),
            Some(LandCover::Shrubland)
        );
        assert_eq!(
            class_from_tags(&json!({"natural":"water"})),
            Some(LandCover::NonBurnable)
        );
        assert_eq!(class_from_tags(&json!({"highway":"primary"})), None);
    }

    #[test]
    fn rasterises_a_square_polygon_and_keeps_default_elsewhere() {
        let frame = LocalFrame {
            origin_lon_lat: [-8.40, 40.70],
        };
        let corner = |e: f64, n: f64| {
            let p = frame.to_lon_lat(e, n);
            json!({"lon":p[0],"lat":p[1]})
        };
        let response = json!({"elements":[{"type":"way","tags":{"natural":"water"},
            "geometry":[corner(0.0,0.0),corner(500.0,0.0),corner(500.0,500.0),corner(0.0,500.0),corner(0.0,0.0)]}]});
        let grid = rasterise_overpass(&response, frame, 10, 10, 100.0, LandCover::Eucalyptus);
        assert_eq!(grid.cells[0], LandCover::NonBurnable);
        assert_eq!(grid.cells[4 * 10 + 4], LandCover::NonBurnable);
        assert_eq!(grid.cells[9 * 10 + 9], LandCover::Eucalyptus);
        assert!((grid.mapped_fraction - 0.25).abs() < 1e-9);
    }
}
