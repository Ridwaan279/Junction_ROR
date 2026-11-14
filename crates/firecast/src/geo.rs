//! Local tangent-plane geometry. x = east, y = north, metres. Bearings are compass
//! degrees/radians: 0 = north, clockwise positive.

pub const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// Equirectangular frame anchored at a south-west origin. Adequate for the
/// <= 60 km domains used here (distortion well under 0.1 %).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalFrame {
    pub origin_lon_lat: [f64; 2],
}

impl LocalFrame {
    pub fn centred(centre_lon_lat: [f64; 2], half_extent_m: f64) -> Self {
        let [lon, lat] = centre_lon_lat;
        Self {
            origin_lon_lat: [
                lon - (half_extent_m / (EARTH_RADIUS_M * lat.to_radians().cos())).to_degrees(),
                lat - (half_extent_m / EARTH_RADIUS_M).to_degrees(),
            ],
        }
    }
    pub fn to_lon_lat(&self, east_m: f64, north_m: f64) -> [f64; 2] {
        let [lon0, lat0] = self.origin_lon_lat;
        let lat = lat0 + (north_m / EARTH_RADIUS_M).to_degrees();
        // Use the latitude at the point for the east scale (keeps long edges accurate).
        let lon = lon0 + (east_m / (EARTH_RADIUS_M * lat.to_radians().cos())).to_degrees();
        [lon, lat]
    }
    pub fn to_local(&self, lon_lat: [f64; 2]) -> (f64, f64) {
        let [lon0, lat0] = self.origin_lon_lat;
        let north = (lon_lat[1] - lat0).to_radians() * EARTH_RADIUS_M;
        let east =
            (lon_lat[0] - lon0).to_radians() * EARTH_RADIUS_M * lon_lat[1].to_radians().cos();
        (east, north)
    }
}

/// Great-circle distance in metres.
pub fn haversine_m(a: [f64; 2], b: [f64; 2]) -> f64 {
    let (lat1, lat2) = (a[1].to_radians(), b[1].to_radians());
    let dlat = lat2 - lat1;
    let dlon = (b[0] - a[0]).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

/// Normalise any angle in degrees to [0, 360).
pub fn norm_deg(deg: f64) -> f64 {
    deg.rem_euclid(360.0)
}

/// Smallest absolute difference between two bearings, degrees in [0, 180].
pub fn bearing_delta_deg(a: f64, b: f64) -> f64 {
    let d = norm_deg(a - b);
    if d > 180.0 { 360.0 - d } else { d }
}

/// Vector (east, north) travelling toward `to_bearing_rad` with magnitude `m`.
pub fn vector_toward(m: f64, to_bearing_rad: f64) -> (f64, f64) {
    (m * to_bearing_rad.sin(), m * to_bearing_rad.cos())
}

/// Bearing (radians, [0, 2π)) of an (east, north) vector.
pub fn bearing_of(east: f64, north: f64) -> f64 {
    east.atan2(north).rem_euclid(std::f64::consts::TAU)
}

/// Meteorological "from" direction (degrees) to the bearing the air travels toward.
pub fn from_to_toward_deg(from_deg: f64) -> f64 {
    norm_deg(from_deg + 180.0)
}

/// Portuguese/English compass words to a bearing in degrees.
pub fn parse_compass(text: &str) -> Option<f64> {
    let t = text.to_lowercase();
    // Longest tokens first so "nordeste" is not read as "norte".
    const WORDS: [(&str, f64); 24] = [
        ("nor-nordeste", 22.5),
        ("lés-nordeste", 67.5),
        ("nordeste", 45.0),
        ("noroeste", 315.0),
        ("sudeste", 135.0),
        ("sueste", 135.0),
        ("sudoeste", 225.0),
        ("northeast", 45.0),
        ("northwest", 315.0),
        ("southeast", 135.0),
        ("southwest", 225.0),
        ("nortada", 0.0),
        ("levante", 90.0),
        ("leste", 90.0),
        ("norte", 0.0),
        ("north", 0.0),
        ("south", 180.0),
        ("oeste", 270.0),
        ("poente", 270.0),
        ("west", 270.0),
        ("east", 90.0),
        ("este", 90.0),
        ("sul", 180.0),
        ("nascente", 90.0),
    ];
    for (word, bearing) in WORDS {
        if t.contains(word) {
            return Some(bearing);
        }
    }
    let upper = text.trim().to_uppercase();
    match upper.as_str() {
        "N" => Some(0.0),
        "NNE" => Some(22.5),
        "NE" => Some(45.0),
        "ENE" => Some(67.5),
        "E" | "L" => Some(90.0),
        "ESE" => Some(112.5),
        "SE" => Some(135.0),
        "SSE" => Some(157.5),
        "S" => Some(180.0),
        "SSW" | "SSO" => Some(202.5),
        "SW" | "SO" => Some(225.0),
        "WSW" | "OSO" => Some(247.5),
        "W" | "O" => Some(270.0),
        "WNW" | "ONO" => Some(292.5),
        "NW" | "NO" => Some(315.0),
        "NNW" | "NNO" => Some(337.5),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_round_trips_within_a_metre() {
        let frame = LocalFrame::centred([-8.367, 40.735], 10_000.0);
        let p = frame.to_lon_lat(12_345.0, 7_890.0);
        let (e, n) = frame.to_local(p);
        assert!((e - 12_345.0).abs() < 1.0 && (n - 7_890.0).abs() < 1.0);
        let centre = frame.to_lon_lat(10_000.0, 10_000.0);
        assert!(haversine_m(centre, [-8.367, 40.735]) < 5.0);
    }

    #[test]
    fn bearings_and_compass_words() {
        assert!((bearing_of(1.0, 0.0).to_degrees() - 90.0).abs() < 1e-9);
        assert!((from_to_toward_deg(270.0) - 90.0).abs() < 1e-9);
        assert_eq!(bearing_delta_deg(350.0, 10.0), 20.0);
        assert_eq!(parse_compass("vento de nordeste"), Some(45.0));
        assert_eq!(parse_compass("progride para sul"), Some(180.0));
        assert_eq!(parse_compass("NW"), Some(315.0));
        assert_eq!(parse_compass("sem direção"), None);
    }
}
