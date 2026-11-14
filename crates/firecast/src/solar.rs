//! Solar position (NOAA General Solar Position, Spencer 1971 Fourier series).
use chrono::{DateTime, Datelike, Timelike, Utc};

/// Solar elevation and azimuth in degrees (azimuth clockwise from north).
pub fn position(lon_lat: [f64; 2], t: DateTime<Utc>) -> (f64, f64) {
    let [lon, lat] = lon_lat;
    let days_in_year = if t.date_naive().leap_year() {
        366.0
    } else {
        365.0
    };
    let hour = f64::from(t.hour()) + f64::from(t.minute()) / 60.0 + f64::from(t.second()) / 3600.0;
    let gamma = 2.0 * std::f64::consts::PI / days_in_year
        * (f64::from(t.ordinal()) - 1.0 + (hour - 12.0) / 24.0);
    let eqtime = 229.18
        * (0.000075 + 0.001868 * gamma.cos()
            - 0.032077 * gamma.sin()
            - 0.014615 * (2.0 * gamma).cos()
            - 0.040849 * (2.0 * gamma).sin());
    let decl = 0.006918 - 0.399912 * gamma.cos() + 0.070257 * gamma.sin()
        - 0.006758 * (2.0 * gamma).cos()
        + 0.000907 * (2.0 * gamma).sin()
        - 0.002697 * (3.0 * gamma).cos()
        + 0.00148 * (3.0 * gamma).sin();
    let true_solar_min = hour * 60.0 + eqtime + 4.0 * lon;
    let hour_angle = (true_solar_min / 4.0 - 180.0).to_radians();
    let phi = lat.to_radians();
    let cos_zenith =
        (phi.sin() * decl.sin() + phi.cos() * decl.cos() * hour_angle.cos()).clamp(-1.0, 1.0);
    let zenith = cos_zenith.acos();
    let elevation = 90.0 - zenith.to_degrees();
    let az = (decl.sin() - phi.sin() * cos_zenith) / (phi.cos() * zenith.sin()).max(1e-9);
    let mut azimuth = az.clamp(-1.0, 1.0).acos().to_degrees();
    if hour_angle > 0.0 {
        azimuth = 360.0 - azimuth;
    }
    (elevation, azimuth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn lisbon_noon_and_midnight() {
        // Summer solstice solar noon at Lisbon (~13:37 UTC): elevation ≈ 90 − 38.7 + 23.4 ≈ 74.7°
        let noon = Utc.with_ymd_and_hms(2026, 6, 21, 12, 37, 0).unwrap();
        let (el, az) = position([-9.14, 38.72], noon);
        assert!((el - 74.7).abs() < 1.0, "{el}");
        assert!((az - 180.0).abs() < 8.0, "{az}");
        let night = Utc.with_ymd_and_hms(2026, 6, 21, 0, 30, 0).unwrap();
        assert!(position([-9.14, 38.72], night).0 < -20.0);
    }
}
