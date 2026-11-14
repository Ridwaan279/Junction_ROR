//! Hourly weather at a set of points, interpolated in space (inverse distance,
//! wind blended as vectors) and linearly in time.
use crate::geo::{bearing_of, from_to_toward_deg, haversine_m, norm_deg};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HourWeather {
    pub time: DateTime<Utc>,
    pub temp_c: f64,
    pub rh_pct: f64,
    /// 10 m wind.
    pub wind_mps: f64,
    pub wind_from_deg: f64,
    pub gust_mps: Option<f64>,
    /// Transport-layer wind (120 m above ground) when available.
    pub wind_upper_mps: Option<f64>,
    pub wind_upper_from_deg: Option<f64>,
    pub cloud_pct: Option<f64>,
    pub shortwave_wm2: Option<f64>,
    pub precip_mm: Option<f64>,
    pub vpd_kpa: Option<f64>,
}

impl HourWeather {
    pub fn wind_vector(&self) -> (f64, f64) {
        let to = from_to_toward_deg(self.wind_from_deg).to_radians();
        (self.wind_mps * to.sin(), self.wind_mps * to.cos())
    }
    pub fn upper_vector(&self) -> Option<(f64, f64)> {
        let (s, d) = (self.wind_upper_mps?, self.wind_upper_from_deg?);
        let to = from_to_toward_deg(d).to_radians();
        Some((s * to.sin(), s * to.cos()))
    }
    pub fn validate(&self) -> bool {
        [self.temp_c, self.rh_pct, self.wind_mps, self.wind_from_deg]
            .iter()
            .all(|v| v.is_finite())
            && (-40.0..=60.0).contains(&self.temp_c)
            && (0.0..=100.0).contains(&self.rh_pct)
            && (0.0..=80.0).contains(&self.wind_mps)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeatherPoint {
    pub lon_lat: [f64; 2],
    /// Sorted by time, hourly.
    pub hours: Vec<HourWeather>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeatherField {
    pub points: Vec<WeatherPoint>,
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
fn lerp_opt(a: Option<f64>, b: Option<f64>, t: f64) -> Option<f64> {
    Some(lerp(a?, b?, t))
}

impl WeatherField {
    pub fn is_empty(&self) -> bool {
        self.points.iter().all(|p| p.hours.is_empty())
    }
    pub fn first_time(&self) -> Option<DateTime<Utc>> {
        self.points
            .iter()
            .filter_map(|p| p.hours.first().map(|h| h.time))
            .min()
    }

    fn at_point(point: &WeatherPoint, t: DateTime<Utc>) -> Option<HourWeather> {
        let hours = &point.hours;
        let first = hours.first()?;
        if t <= first.time {
            return Some(*first);
        }
        let last = hours.last()?;
        if t >= last.time {
            return Some(*last);
        }
        let i = hours.partition_point(|h| h.time <= t).saturating_sub(1);
        let (a, b) = (&hours[i], &hours[(i + 1).min(hours.len() - 1)]);
        let span = (b.time - a.time).num_seconds().max(1) as f64;
        let f = ((t - a.time).num_seconds() as f64 / span).clamp(0.0, 1.0);
        let (ae, an) = a.wind_vector();
        let (be, bn) = b.wind_vector();
        let (e, n) = (lerp(ae, be, f), lerp(an, bn, f));
        let upper = match (a.upper_vector(), b.upper_vector()) {
            (Some((ue, un)), Some((ve, vn))) => Some((lerp(ue, ve, f), lerp(un, vn, f))),
            _ => None,
        };
        Some(HourWeather {
            time: t,
            temp_c: lerp(a.temp_c, b.temp_c, f),
            rh_pct: lerp(a.rh_pct, b.rh_pct, f),
            wind_mps: e.hypot(n),
            wind_from_deg: norm_deg(bearing_of(e, n).to_degrees() + 180.0),
            gust_mps: lerp_opt(a.gust_mps, b.gust_mps, f),
            wind_upper_mps: upper.map(|(e, n)| e.hypot(n)),
            wind_upper_from_deg: upper
                .map(|(e, n)| norm_deg(bearing_of(e, n).to_degrees() + 180.0)),
            cloud_pct: lerp_opt(a.cloud_pct, b.cloud_pct, f),
            shortwave_wm2: lerp_opt(a.shortwave_wm2, b.shortwave_wm2, f),
            precip_mm: lerp_opt(a.precip_mm, b.precip_mm, f),
            vpd_kpa: lerp_opt(a.vpd_kpa, b.vpd_kpa, f),
        })
    }

    /// Inverse-distance-squared interpolation; wind components blended as vectors.
    pub fn sample(&self, lon_lat: [f64; 2], t: DateTime<Utc>) -> Option<HourWeather> {
        let mut samples = Vec::with_capacity(self.points.len());
        for p in &self.points {
            if let Some(h) = Self::at_point(p, t) {
                let d = haversine_m(lon_lat, p.lon_lat);
                if d < 1.0 {
                    return Some(h);
                }
                samples.push((1.0 / (d * d), h));
            }
        }
        let total: f64 = samples.iter().map(|(w, _)| w).sum();
        if samples.is_empty() || total <= 0.0 {
            return None;
        }
        let avg = |f: &dyn Fn(&HourWeather) -> f64| {
            samples.iter().map(|(w, h)| w * f(h)).sum::<f64>() / total
        };
        let avg_opt = |f: &dyn Fn(&HourWeather) -> Option<f64>| {
            let present: Vec<_> = samples
                .iter()
                .filter_map(|(w, h)| f(h).map(|v| (*w, v)))
                .collect();
            let tw: f64 = present.iter().map(|(w, _)| w).sum();
            (tw > 0.0).then(|| present.iter().map(|(w, v)| w * v).sum::<f64>() / tw)
        };
        let e = avg(&|h| h.wind_vector().0);
        let n = avg(&|h| h.wind_vector().1);
        let ue = avg_opt(&|h| h.upper_vector().map(|v| v.0));
        let un = avg_opt(&|h| h.upper_vector().map(|v| v.1));
        let upper = ue.zip(un);
        Some(HourWeather {
            time: t,
            temp_c: avg(&|h| h.temp_c),
            rh_pct: avg(&|h| h.rh_pct),
            wind_mps: e.hypot(n),
            wind_from_deg: norm_deg(bearing_of(e, n).to_degrees() + 180.0),
            gust_mps: avg_opt(&|h| h.gust_mps),
            wind_upper_mps: upper.map(|(e, n)| e.hypot(n)),
            wind_upper_from_deg: upper
                .map(|(e, n)| norm_deg(bearing_of(e, n).to_degrees() + 180.0)),
            cloud_pct: avg_opt(&|h| h.cloud_pct),
            shortwave_wm2: avg_opt(&|h| h.shortwave_wm2),
            precip_mm: avg_opt(&|h| h.precip_mm),
            vpd_kpa: avg_opt(&|h| h.vpd_kpa),
        })
    }

    /// Total precipitation over the `hours` before `t` at the nearest point.
    pub fn precip_before(&self, lon_lat: [f64; 2], t: DateTime<Utc>, hours: i64) -> Option<f64> {
        let nearest = self.points.iter().min_by(|a, b| {
            haversine_m(lon_lat, a.lon_lat).total_cmp(&haversine_m(lon_lat, b.lon_lat))
        })?;
        let from = t - chrono::Duration::hours(hours);
        let values: Vec<f64> = nearest
            .hours
            .iter()
            .filter(|h| h.time > from && h.time <= t)
            .filter_map(|h| h.precip_mm)
            .collect();
        (!values.is_empty()).then(|| values.iter().sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn hour(t: i64, speed: f64, from: f64) -> HourWeather {
        HourWeather {
            time: Utc.timestamp_opt(1_786_000_000 + t * 3600, 0).unwrap(),
            temp_c: 30.0,
            rh_pct: 20.0,
            wind_mps: speed,
            wind_from_deg: from,
            gust_mps: Some(speed * 1.6),
            wind_upper_mps: None,
            wind_upper_from_deg: None,
            cloud_pct: Some(0.0),
            shortwave_wm2: None,
            precip_mm: Some(0.0),
            vpd_kpa: None,
        }
    }

    #[test]
    fn interpolates_wind_as_vectors_across_north() {
        let field = WeatherField {
            points: vec![
                WeatherPoint {
                    lon_lat: [-8.40, 40.70],
                    hours: vec![hour(0, 5.0, 350.0)],
                },
                WeatherPoint {
                    lon_lat: [-8.30, 40.70],
                    hours: vec![hour(0, 5.0, 10.0)],
                },
            ],
        };
        let mid = field
            .sample([-8.35, 40.70], field.first_time().unwrap())
            .unwrap();
        assert!(
            mid.wind_from_deg < 1.0 || mid.wind_from_deg > 359.0,
            "{}",
            mid.wind_from_deg
        );
        assert!(mid.wind_mps > 4.9);
    }

    #[test]
    fn interpolates_in_time_and_clamps_outside_range() {
        let point = WeatherPoint {
            lon_lat: [-8.4, 40.7],
            hours: vec![hour(0, 2.0, 270.0), hour(1, 6.0, 270.0)],
        };
        let field = WeatherField {
            points: vec![point],
        };
        let t0 = field.first_time().unwrap();
        let half = field
            .sample([-8.4, 40.7], t0 + chrono::Duration::minutes(30))
            .unwrap();
        assert!((half.wind_mps - 4.0).abs() < 1e-9);
        let late = field
            .sample([-8.4, 40.7], t0 + chrono::Duration::hours(9))
            .unwrap();
        assert!((late.wind_mps - 6.0).abs() < 1e-9);
    }
}
