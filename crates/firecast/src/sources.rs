//! Documented public APIs: Open-Meteo (forecast, elevation), AWS/Mapzen Terrarium
//! terrain tiles, OpenStreetMap Overpass, IPMA weather warnings. Every call has a
//! deadline and a payload bound; failures are reported, never treated as data.
use crate::FirecastError;
use crate::fuel::LandCover;
use crate::geo::{LocalFrame, haversine_m};
use crate::landcover::{LandCoverGrid, overpass_query, rasterise_overpass};
use crate::terrain::{Dem, resample};
use crate::weather::{HourWeather, WeatherField, WeatherPoint};
use chrono::{DateTime, NaiveDateTime, Timelike, Utc};
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

pub const OPEN_METEO: &str = "https://api.open-meteo.com";
pub const TERRARIUM: &str =
    "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png";
pub const OVERPASS: &str = "https://overpass-api.de/api/interpreter";
pub const IPMA_WARNINGS: &str = "https://api.ipma.pt/open-data/forecast/warnings/warnings_www.json";

const HOURLY: &str = "temperature_2m,relative_humidity_2m,wind_speed_10m,wind_direction_10m,wind_gusts_10m,\
wind_speed_120m,wind_direction_120m,cloud_cover,shortwave_radiation,precipitation,vapour_pressure_deficit";

#[derive(Clone)]
pub struct HttpSources {
    http: reqwest::Client,
    pub open_meteo: String,
    pub terrarium: String,
    pub overpass: Option<String>,
    pub ipma_warnings: String,
}

impl HttpSources {
    pub fn new() -> Result<Self, FirecastError> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(6))
            .timeout(Duration::from_secs(25))
            .user_agent("EmberWatch-firecast/0.1 (research prototype; attribution retained)")
            .build()
            .map_err(|_| FirecastError::Config("could not build HTTP client".into()))?;
        Ok(Self {
            http,
            open_meteo: std::env::var("OPEN_METEO_URL").unwrap_or_else(|_| OPEN_METEO.into()),
            terrarium: std::env::var("TERRAIN_TILES_URL").unwrap_or_else(|_| TERRARIUM.into()),
            overpass: match std::env::var("OVERPASS_URL") {
                Ok(v) if v == "off" => None,
                Ok(v) => Some(v),
                Err(_) => Some(OVERPASS.into()),
            },
            ipma_warnings: IPMA_WARNINGS.into(),
        })
    }

    async fn get_bytes(&self, url: &str, limit: usize) -> Result<Vec<u8>, FirecastError> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|_| FirecastError::Network(host(url)))?;
        let status = response.status();
        let bytes = read_bounded(response, limit).await?;
        if !status.is_success() {
            let reason = serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|v| v.get("reason").and_then(Value::as_str).map(str::to_owned))
                .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
            return Err(FirecastError::Upstream(format!(
                "{}: {}",
                host(url),
                truncate(&reason, 160)
            )));
        }
        Ok(bytes)
    }

    pub(crate) async fn get_json(&self, url: &str, limit: usize) -> Result<Value, FirecastError> {
        let bytes = self.get_bytes(url, limit).await?;
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| FirecastError::Parse(host(url)))?;
        if value.get("error").and_then(Value::as_bool) == Some(true) {
            let reason = value
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("error");
            return Err(FirecastError::Upstream(format!(
                "{}: {}",
                host(url),
                truncate(reason, 160)
            )));
        }
        Ok(value)
    }

    /// Hourly weather at `points` from `start` − 3 h to `start` + `hours` h.
    pub async fn weather(
        &self,
        points: &[[f64; 2]],
        start: DateTime<Utc>,
        hours: i64,
    ) -> Result<(WeatherField, String), FirecastError> {
        let from = floor_hour(start) - chrono::Duration::hours(3);
        let to = floor_hour(start) + chrono::Duration::hours(hours + 1);
        let lat: Vec<String> = points.iter().map(|p| format!("{:.4}", p[1])).collect();
        let lon: Vec<String> = points.iter().map(|p| format!("{:.4}", p[0])).collect();
        let url = format!(
            "{}/v1/forecast?latitude={}&longitude={}&hourly={HOURLY}&wind_speed_unit=ms&timezone=GMT&start_hour={}&end_hour={}",
            self.open_meteo,
            lat.join(","),
            lon.join(","),
            from.format("%Y-%m-%dT%H:%M"),
            to.format("%Y-%m-%dT%H:%M"),
        );
        let value = self.get_json(&url, 4 * 1024 * 1024).await?;
        Ok((parse_open_meteo(&value, points)?, url))
    }

    /// Hourly ERA5 reanalysis (Open-Meteo archive) for whole UTC days, inclusive.
    pub async fn weather_archive(
        &self,
        points: &[[f64; 2]],
        first_day: chrono::NaiveDate,
        last_day: chrono::NaiveDate,
    ) -> Result<(WeatherField, String), FirecastError> {
        let lat: Vec<String> = points.iter().map(|p| format!("{:.4}", p[1])).collect();
        let lon: Vec<String> = points.iter().map(|p| format!("{:.4}", p[0])).collect();
        let hourly = "temperature_2m,relative_humidity_2m,wind_speed_10m,wind_direction_10m,wind_gusts_10m,\
wind_speed_100m,wind_direction_100m,cloud_cover,shortwave_radiation,precipitation,vapour_pressure_deficit";
        let url = format!(
            "https://archive-api.open-meteo.com/v1/archive?latitude={}&longitude={}&start_date={first_day}&end_date={last_day}&hourly={hourly}&wind_speed_unit=ms&timezone=GMT",
            lat.join(","),
            lon.join(","),
        );
        let value = self.get_json(&url, 16 * 1024 * 1024).await?;
        Ok((parse_open_meteo(&value, points)?, url))
    }

    /// DEM from Terrarium tiles (zoom 11, ≈58 m pixels at 40° N), bilinear sampled.
    pub async fn dem_tiles(
        &self,
        frame: LocalFrame,
        width: usize,
        height: usize,
        cell_m: f64,
    ) -> Result<Dem, FirecastError> {
        self.dem_tiles_at_zoom(frame, width, height, cell_m, 11)
            .await
    }

    /// Explicit resolution for an offline regional planning DEM. No synthetic
    /// fallback: a missing tile fails the capture. The same 16-tile bound applies.
    pub async fn dem_tiles_at_zoom(
        &self,
        frame: LocalFrame,
        width: usize,
        height: usize,
        cell_m: f64,
        zoom: u32,
    ) -> Result<Dem, FirecastError> {
        if !(8..=11).contains(&zoom)
            || width == 0
            || height == 0
            || width.checked_mul(height).is_none_or(|n| n > 250_000)
            || !cell_m.is_finite()
            || cell_m <= 0.
        {
            return Err(FirecastError::Config("invalid bounded regional DEM".into()));
        }
        let z = zoom;
        let sw = frame.to_lon_lat(0.0, 0.0);
        let ne = frame.to_lon_lat(width as f64 * cell_m, height as f64 * cell_m);
        let (x0, y1) = tile_xy(sw, z);
        let (x1, y0) = tile_xy(ne, z);
        let (tx0, tx1, ty0, ty1) = (
            x0.floor() as u32,
            x1.floor() as u32,
            y0.floor() as u32,
            y1.floor() as u32,
        );
        if (tx1 - tx0 + 1) * (ty1 - ty0 + 1) > 16 {
            return Err(FirecastError::Config(
                "terrain domain needs too many tiles".into(),
            ));
        }
        let mut tiles = Vec::new();
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let url = self
                    .terrarium
                    .replace("{z}", &z.to_string())
                    .replace("{x}", &tx.to_string())
                    .replace("{y}", &ty.to_string());
                tiles.push(((tx, ty), url));
            }
        }
        let fetched = futures::future::join_all(
            tiles
                .iter()
                .map(|(_, url)| self.get_bytes(url, 2 * 1024 * 1024)),
        )
        .await;
        let mut decoded = std::collections::HashMap::new();
        for ((key, _), bytes) in tiles.iter().zip(fetched) {
            decoded.insert(*key, decode_terrarium(&bytes?)?);
        }
        let mut elevation_m = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let p = frame.to_lon_lat((x as f64 + 0.5) * cell_m, (y as f64 + 0.5) * cell_m);
                let (gx, gy) = tile_xy(p, z);
                let (px, py) = (gx * 256.0 - 0.5, gy * 256.0 - 0.5);
                let sample = |sx: f64, sy: f64| -> Option<f64> {
                    let (tx, ty) = ((sx / 256.0).floor() as u32, (sy / 256.0).floor() as u32);
                    let tile = decoded.get(&(tx, ty))?;
                    let (ix, iy) = (
                        (sx - f64::from(tx) * 256.0) as usize,
                        (sy - f64::from(ty) * 256.0) as usize,
                    );
                    tile.get(iy.min(255) * 256 + ix.min(255)).copied()
                };
                let (fx, fy) = (px.floor(), py.floor());
                let (tx, ty) = (px - fx, py - fy);
                let v = match (
                    sample(fx, fy),
                    sample(fx + 1.0, fy),
                    sample(fx, fy + 1.0),
                    sample(fx + 1.0, fy + 1.0),
                ) {
                    (Some(a), Some(b), Some(c), Some(d)) => {
                        (a * (1.0 - tx) + b * tx) * (1.0 - ty) + (c * (1.0 - tx) + d * tx) * ty
                    }
                    (Some(a), ..) => a,
                    _ => return Err(FirecastError::Parse("terrain tile coverage".into())),
                };
                elevation_m.push(v as f32);
            }
        }
        Ok(Dem {
            width,
            height,
            cell_m,
            elevation_m,
            frame,
        })
    }

    /// Coarse DEM from the Open-Meteo elevation API (Copernicus GLO-90), resampled.
    pub async fn dem_open_meteo(
        &self,
        frame: LocalFrame,
        width: usize,
        height: usize,
        cell_m: f64,
    ) -> Result<Dem, FirecastError> {
        const N: usize = 21;
        let (span_x, span_y) = (width as f64 * cell_m, height as f64 * cell_m);
        let coords: Vec<[f64; 2]> = (0..N * N)
            .map(|k| {
                frame.to_lon_lat(
                    ((k % N) as f64 + 0.5) / N as f64 * span_x,
                    ((k / N) as f64 + 0.5) / N as f64 * span_y,
                )
            })
            .collect();
        let mut coarse = Vec::with_capacity(N * N);
        for chunk in coords.chunks(100) {
            let lat: Vec<String> = chunk.iter().map(|p| format!("{:.5}", p[1])).collect();
            let lon: Vec<String> = chunk.iter().map(|p| format!("{:.5}", p[0])).collect();
            let url = format!(
                "{}/v1/elevation?latitude={}&longitude={}",
                self.open_meteo,
                lat.join(","),
                lon.join(",")
            );
            let value = self.get_json(&url, 256 * 1024).await?;
            let values = value
                .get("elevation")
                .and_then(Value::as_array)
                .ok_or_else(|| FirecastError::Parse("elevation".into()))?;
            if values.len() != chunk.len() {
                return Err(FirecastError::Parse("elevation count".into()));
            }
            coarse.extend(values.iter().map(|v| v.as_f64().unwrap_or(0.0)));
        }
        Ok(Dem {
            width,
            height,
            cell_m,
            elevation_m: resample(&coarse, N, width, height),
            frame,
        })
    }

    pub async fn land_cover(
        &self,
        frame: LocalFrame,
        width: usize,
        height: usize,
        cell_m: f64,
        default: LandCover,
    ) -> Result<(LandCoverGrid, String), FirecastError> {
        let url = self
            .overpass
            .clone()
            .ok_or_else(|| FirecastError::Config("Overpass disabled".into()))?;
        let sw = frame.to_lon_lat(0.0, 0.0);
        let ne = frame.to_lon_lat(width as f64 * cell_m, height as f64 * cell_m);
        let query = overpass_query([sw[1], sw[0], ne[1], ne[0]]);
        let response = self
            .http
            .post(&url)
            .timeout(Duration::from_secs(35))
            .form(&[("data", query)])
            .send()
            .await
            .map_err(|_| FirecastError::Network(host(&url)))?;
        if !response.status().is_success() {
            return Err(FirecastError::Upstream(format!(
                "{}: HTTP {}",
                host(&url),
                response.status().as_u16()
            )));
        }
        let bytes = read_bounded(response, 24 * 1024 * 1024).await?;
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| FirecastError::Parse("overpass".into()))?;
        Ok((
            rasterise_overpass(&value, frame, width, height, cell_m, default),
            url,
        ))
    }

    pub async fn ipma_warnings(
        &self,
        lon_lat: [f64; 2],
        at: DateTime<Utc>,
    ) -> Result<(Vec<WeatherWarning>, String), FirecastError> {
        let value = self.get_json(&self.ipma_warnings, 2 * 1024 * 1024).await?;
        Ok((
            parse_ipma_warnings(&value, lon_lat, at),
            self.ipma_warnings.clone(),
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WeatherWarning {
    pub area: String,
    pub kind: String,
    pub level: String,
    pub text: String,
    pub start: Option<String>,
    pub end: Option<String>,
}

/// District capitals used to pick the IPMA warning area (nearest capital).
const DISTRICTS: [(&str, f64, f64); 18] = [
    ("AVR", -8.6538, 40.6405),
    ("BJA", -7.8632, 38.0151),
    ("BRG", -8.4265, 41.5454),
    ("BGC", -6.7567, 41.8061),
    ("CBO", -7.4909, 39.8222),
    ("CBR", -8.4103, 40.2033),
    ("EVR", -7.9135, 38.5714),
    ("FAR", -7.9322, 37.0194),
    ("GDA", -7.2679, 40.5373),
    ("LRA", -8.8071, 39.7436),
    ("LSB", -9.1393, 38.7223),
    ("PTG", -7.4285, 39.2967),
    ("PTO", -8.6291, 41.1579),
    ("STM", -8.6850, 39.2369),
    ("STB", -8.8882, 38.5244),
    ("VCT", -8.8329, 41.6932),
    ("VRL", -7.7441, 41.3006),
    ("VIS", -7.9125, 40.6566),
];

pub fn district_code(lon_lat: [f64; 2]) -> &'static str {
    DISTRICTS
        .iter()
        .min_by(|a, b| {
            haversine_m(lon_lat, [a.1, a.2]).total_cmp(&haversine_m(lon_lat, [b.1, b.2]))
        })
        .map_or("LSB", |d| d.0)
}

pub fn parse_ipma_warnings(
    value: &Value,
    lon_lat: [f64; 2],
    at: DateTime<Utc>,
) -> Vec<WeatherWarning> {
    let area = district_code(lon_lat);
    let parse_t = |v: &Value| {
        v.as_str()
            .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok())
    };
    value
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter(|w| w.get("idAreaAviso").and_then(Value::as_str) == Some(area))
                .filter(|w| {
                    w.get("awarenessLevelID")
                        .and_then(Value::as_str)
                        .is_some_and(|l| l != "green")
                })
                .filter(
                    |w| match (parse_t(&w["startTime"]), parse_t(&w["endTime"])) {
                        (Some(s), Some(e)) => {
                            s.and_utc() <= at + chrono::Duration::hours(12) && e.and_utc() >= at
                        }
                        _ => true,
                    },
                )
                .map(|w| WeatherWarning {
                    area: area.into(),
                    kind: w["awarenessTypeName"].as_str().unwrap_or("").into(),
                    level: w["awarenessLevelID"].as_str().unwrap_or("").into(),
                    text: truncate(w["text"].as_str().unwrap_or(""), 300),
                    start: w["startTime"].as_str().map(str::to_owned),
                    end: w["endTime"].as_str().map(str::to_owned),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_open_meteo(value: &Value, points: &[[f64; 2]]) -> Result<WeatherField, FirecastError> {
    let locations: Vec<&Value> = match value {
        Value::Array(items) => items.iter().collect(),
        object => vec![object],
    };
    if locations.len() != points.len() {
        return Err(FirecastError::Parse("open-meteo location count".into()));
    }
    let mut out = Vec::new();
    for (loc, point) in locations.iter().zip(points) {
        let hourly = loc
            .get("hourly")
            .ok_or_else(|| FirecastError::Parse("open-meteo hourly".into()))?;
        let times = hourly
            .get("time")
            .and_then(Value::as_array)
            .ok_or_else(|| FirecastError::Parse("open-meteo time".into()))?;
        let series = |key: &str| -> Vec<Option<f64>> {
            hourly
                .get(key)
                .and_then(Value::as_array)
                .map(|a| a.iter().map(Value::as_f64).collect())
                .unwrap_or_default()
        };
        let (t2, rh, ws, wd, gust, ws120, wd120, cloud, sw, rain, vpd) = (
            series("temperature_2m"),
            series("relative_humidity_2m"),
            series("wind_speed_10m"),
            series("wind_direction_10m"),
            series("wind_gusts_10m"),
            // 120 m (forecast models) or 100 m (ERA5 reanalysis) as the transport level.
            Some(series("wind_speed_120m"))
                .filter(|v| v.iter().any(Option::is_some))
                .unwrap_or_else(|| series("wind_speed_100m")),
            Some(series("wind_direction_120m"))
                .filter(|v| v.iter().any(Option::is_some))
                .unwrap_or_else(|| series("wind_direction_100m")),
            series("cloud_cover"),
            series("shortwave_radiation"),
            series("precipitation"),
            series("vapour_pressure_deficit"),
        );
        let at = |v: &Vec<Option<f64>>, i: usize| v.get(i).copied().flatten();
        let mut hours = Vec::new();
        for (i, t) in times.iter().enumerate() {
            let Some(time) = t
                .as_str()
                .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").ok())
            else {
                continue;
            };
            let (Some(temp_c), Some(rh_pct), Some(wind_mps), Some(wind_from_deg)) =
                (at(&t2, i), at(&rh, i), at(&ws, i), at(&wd, i))
            else {
                continue;
            };
            let hour = HourWeather {
                time: time.and_utc(),
                temp_c,
                rh_pct,
                wind_mps,
                wind_from_deg,
                gust_mps: at(&gust, i),
                wind_upper_mps: at(&ws120, i),
                wind_upper_from_deg: at(&wd120, i),
                cloud_pct: at(&cloud, i),
                shortwave_wm2: at(&sw, i),
                precip_mm: at(&rain, i),
                vpd_kpa: at(&vpd, i),
            };
            if hour.validate() {
                hours.push(hour);
            }
        }
        out.push(WeatherPoint {
            lon_lat: *point,
            hours,
        });
    }
    let field = WeatherField { points: out };
    if field.is_empty() {
        return Err(FirecastError::Parse(
            "open-meteo returned no complete hours".into(),
        ));
    }
    Ok(field)
}

/// Terrarium encoding: elevation = R·256 + G + B/256 − 32768 (metres).
pub fn decode_terrarium(bytes: &[u8]) -> Result<Vec<f64>, FirecastError> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|_| FirecastError::Parse("terrain png".into()))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|_| FirecastError::Parse("terrain png frame".into()))?;
    if info.width != 256 || info.height != 256 || info.bit_depth != png::BitDepth::Eight {
        return Err(FirecastError::Parse("terrain tile shape".into()));
    }
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => return Err(FirecastError::Parse("terrain tile colour type".into())),
    };
    Ok(buf[..info.buffer_size()]
        .chunks(channels)
        .map(|px| f64::from(px[0]) * 256.0 + f64::from(px[1]) + f64::from(px[2]) / 256.0 - 32768.0)
        .collect())
}

/// Fractional Web-Mercator tile coordinates.
pub fn tile_xy(lon_lat: [f64; 2], z: u32) -> (f64, f64) {
    let n = f64::from(1u32 << z);
    let lat = lon_lat[1].to_radians();
    (
        (lon_lat[0] + 180.0) / 360.0 * n,
        (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * n,
    )
}

pub(crate) fn floor_hour(t: DateTime<Utc>) -> DateTime<Utc> {
    t.with_minute(0)
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(t)
}

pub(crate) async fn read_bounded(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, FirecastError> {
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err(FirecastError::Upstream("payload exceeds bound".into()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| FirecastError::Network("body".into()))?
    {
        if bytes.len() + chunk.len() > limit {
            return Err(FirecastError::Upstream("payload exceeds bound".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(crate) fn host(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| "upstream".into())
}

pub(crate) fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    #[test]
    fn parses_multi_location_open_meteo_and_skips_null_hours() {
        let loc = |speed: f64| {
            json!({"latitude":40.7,"longitude":-8.4,"hourly":{
                "time":["2026-08-10T12:00","2026-08-10T13:00"],
                "temperature_2m":[31.0,32.0],"relative_humidity_2m":[22.0,null],
                "wind_speed_10m":[speed,speed],"wind_direction_10m":[300.0,305.0],
                "wind_gusts_10m":[9.0,9.5],"wind_speed_120m":[8.0,8.2],"wind_direction_120m":[310.0,312.0],
                "cloud_cover":[5.0,10.0],"shortwave_radiation":[820.0,780.0],"precipitation":[0.0,0.0],
                "vapour_pressure_deficit":[3.4,3.6]}})
        };
        let field =
            parse_open_meteo(&json!([loc(5.0), loc(6.0)]), &[[-8.4, 40.7], [-8.3, 40.7]]).unwrap();
        assert_eq!(field.points.len(), 2);
        assert_eq!(field.points[0].hours.len(), 1); // second hour has null RH
        let h = field.points[1].hours[0];
        assert_eq!(h.time, Utc.with_ymd_and_hms(2026, 8, 10, 12, 0, 0).unwrap());
        assert_eq!((h.wind_mps, h.wind_upper_from_deg), (6.0, Some(310.0)));
        assert!(parse_open_meteo(&json!([loc(5.0)]), &[[-8.4, 40.7], [-8.3, 40.7]]).is_err());
    }

    #[test]
    fn ipma_warnings_filter_by_nearest_district_level_and_time() {
        let at = Utc.with_ymd_and_hms(2026, 8, 10, 12, 0, 0).unwrap();
        let rows = json!([
            {"idAreaAviso":"AVR","awarenessTypeName":"Tempo Quente","awarenessLevelID":"orange","text":"Calor","startTime":"2026-08-10T09:00:00","endTime":"2026-08-10T21:00:00"},
            {"idAreaAviso":"AVR","awarenessTypeName":"Vento","awarenessLevelID":"green","text":"","startTime":"2026-08-10T09:00:00","endTime":"2026-08-10T21:00:00"},
            {"idAreaAviso":"FAR","awarenessTypeName":"Vento","awarenessLevelID":"yellow","text":"","startTime":"2026-08-10T09:00:00","endTime":"2026-08-10T21:00:00"},
            {"idAreaAviso":"AVR","awarenessTypeName":"Vento","awarenessLevelID":"yellow","text":"old","startTime":"2026-08-08T09:00:00","endTime":"2026-08-08T21:00:00"}
        ]);
        let w = parse_ipma_warnings(&rows, [-8.367, 40.735], at);
        assert_eq!(w.len(), 1);
        assert_eq!(
            (w[0].kind.as_str(), w[0].level.as_str()),
            ("Tempo Quente", "orange")
        );
        assert_eq!(district_code([-7.93, 37.02]), "FAR");
    }

    #[test]
    fn terrarium_pixels_decode_and_tiles_locate() {
        let mut raw = Vec::new();
        for _ in 0..256 * 256 {
            raw.extend_from_slice(&[128, 100, 128]); // 128*256 + 100 + 0.5 - 32768 = 100.5 m
        }
        let mut png_bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png_bytes, 256, 256);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&raw).unwrap();
        }
        let decoded = decode_terrarium(&png_bytes).unwrap();
        assert!((decoded[0] - 100.5).abs() < 1e-9);
        let (x, y) = tile_xy([0.0, 0.0], 1);
        assert!((x - 1.0).abs() < 1e-9 && (y - 1.0).abs() < 1e-9);
    }
}
