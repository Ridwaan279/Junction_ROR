use crate::uploads::parse_firms_csv;
use chrono::{DateTime, Utc};
use emberwatch_domain::*;
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

fn descriptor(id: &str, name: &str, cadence_s: u64, ready: bool) -> SourceDescriptor {
    SourceDescriptor {
        id: id.into(),
        name: name.into(),
        fetch: "api".into(),
        origin_group: id.into(),
        enabled: false,
        cadence_s,
        health: if ready {
            SourceHealth::Stale
        } else {
            SourceHealth::Unconfigured
        },
        validated: ready,
        replayable: true,
        last_attempt: None,
        last_success: None,
        last_error: None,
        coverage: None,
    }
}

pub fn http_client() -> Result<Client, reqwest::Error> {
    Client::builder()
        .timeout(Duration::from_secs(25))
        .connect_timeout(Duration::from_secs(8))
        .user_agent("EmberWatch/0.1 (Portugal research prototype)")
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
}

pub struct FirmsSource {
    client: Client,
    key: Option<String>,
}

impl FirmsSource {
    pub fn new(client: Client, key: Option<String>) -> Self {
        Self { client, key }
    }
}

impl Source for FirmsSource {
    fn descriptor(&self) -> SourceDescriptor {
        descriptor(
            "nasa-firms",
            "NASA FIRMS VIIRS SNPP",
            600,
            self.key.as_ref().is_some_and(|key| !key.is_empty()),
        )
    }

    fn run(&self, context: RunContext) -> SourceFuture<'_> {
        Box::pin(async move {
            let key = self
                .key
                .as_deref()
                .filter(|key| !key.is_empty())
                .ok_or(SourceError::Unconfigured)?;
            if !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                return Err(SourceError::Unconfigured);
            }
            let [west, south, east, north] = context.bbox;
            let product = if context.mode == RunMode::Live {
                "VIIRS_SNPP_NRT"
            } else {
                "VIIRS_SNPP_SP"
            };
            let date = if context.mode == RunMode::Live {
                String::new()
            } else {
                format!("/{}", context.from.format("%Y-%m-%d"))
            };
            // Never return reqwest errors containing a credential-bearing FIRMS URL.
            let url = format!(
                "https://firms.modaps.eosdis.nasa.gov/api/area/csv/{key}/{product}/{west},{south},{east},{north}/1{date}"
            );
            let response = self
                .client
                .get(url)
                .send()
                .await
                .map_err(|_| SourceError::Unavailable)?;
            if matches!(response.status().as_u16(), 401 | 403 | 429) {
                return Err(SourceError::Blocked);
            }
            if !response.status().is_success() {
                return Err(SourceError::Unavailable);
            }
            if response
                .content_length()
                .is_some_and(|n| n > 2 * 1024 * 1024)
            {
                return Err(SourceError::Invalid("response exceeds limit".into()));
            }
            let text = response
                .text()
                .await
                .map_err(|_| SourceError::Unavailable)?;
            if text.len() > 2 * 1024 * 1024 {
                return Err(SourceError::Invalid("response exceeds limit".into()));
            }
            let captured_at = Utc::now();
            let evidence =
                parse_firms_csv(&text, "nasa-firms", captured_at).map_err(SourceError::Invalid)?;
            let empty = evidence.is_empty();
            Ok(SourceBatch {
                source_id: "nasa-firms".into(),
                evidence,
                weather: vec![],
                coverage: Coverage {
                    complete: true,
                    explicit_empty: empty,
                    note: Some(format!(
                        "{product}: selected product/date/bbox, not complete fire coverage"
                    )),
                },
                captured_at,
            })
        })
    }
}

pub struct OpenMeteoSource {
    client: Client,
}
impl OpenMeteoSource {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub async fn elevation(&self, points: &[Point]) -> Result<Vec<Option<f64>>, SourceError> {
        if points.is_empty() || points.len() > 100 {
            return Err(SourceError::Invalid(
                "elevation accepts 1..100 points".into(),
            ));
        }
        for point in points {
            point.validate().map_err(SourceError::Invalid)?;
        }
        let lat = points
            .iter()
            .map(|p| p.latitude.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let lon = points
            .iter()
            .map(|p| p.longitude.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let response = self
            .client
            .get("https://api.open-meteo.com/v1/elevation")
            .query(&[("latitude", lat), ("longitude", lon)])
            .send()
            .await
            .map_err(|_| SourceError::Unavailable)?;
        if !response.status().is_success() {
            return Err(SourceError::Unavailable);
        }
        let data: Value = response
            .json()
            .await
            .map_err(|_| SourceError::Invalid("elevation JSON".into()))?;
        let values = data["elevation"]
            .as_array()
            .ok_or_else(|| SourceError::Invalid("missing elevation".into()))?;
        if values.len() != points.len() {
            return Err(SourceError::Invalid("elevation count mismatch".into()));
        }
        Ok(values.iter().map(Value::as_f64).collect())
    }
}

impl Source for OpenMeteoSource {
    fn descriptor(&self) -> SourceDescriptor {
        descriptor(
            "open-meteo",
            "Open-Meteo forecast (bbox-centre sample)",
            3600,
            true,
        )
    }
    fn run(&self, context: RunContext) -> SourceFuture<'_> {
        Box::pin(async move {
            // A bounded initial sample; additional grid acquisition can reuse this adapter.
            let latitude = (context.bbox[1] + context.bbox[3]) / 2.0;
            let longitude = (context.bbox[0] + context.bbox[2]) / 2.0;
            let archive = context.mode != RunMode::Live;
            let url = if archive {
                "https://archive-api.open-meteo.com/v1/archive"
            } else {
                "https://api.open-meteo.com/v1/forecast"
            };
            let mut query = vec![("latitude", latitude.to_string()), ("longitude", longitude.to_string()),
                ("hourly", "temperature_2m,relative_humidity_2m,wind_speed_10m,wind_direction_10m,wind_gusts_10m".into()),
                ("wind_speed_unit", "kmh".into()), ("timeformat", "unixtime".into()), ("timezone", "UTC".into())];
            if archive {
                query.push(("start_date", context.from.format("%Y-%m-%d").to_string()));
                query.push(("end_date", context.now.format("%Y-%m-%d").to_string()));
            } else {
                query.push(("forecast_days", "2".into()));
            }
            let response = self
                .client
                .get(url)
                .query(&query)
                .send()
                .await
                .map_err(|_| SourceError::Unavailable)?;
            if matches!(response.status().as_u16(), 403 | 429) {
                return Err(SourceError::Blocked);
            }
            if !response.status().is_success() {
                return Err(SourceError::Unavailable);
            }
            let value: Value = response
                .json()
                .await
                .map_err(|_| SourceError::Invalid("weather JSON".into()))?;
            let captured_at = Utc::now();
            let weather = parse_open_meteo(&value, captured_at)?;
            Ok(SourceBatch {
                source_id: "open-meteo".into(),
                evidence: vec![],
                weather,
                coverage: Coverage {
                    complete: true,
                    explicit_empty: false,
                    note: Some("Single bbox-centre model sample; not a national wind grid".into()),
                },
                captured_at,
            })
        })
    }
}

pub fn parse_open_meteo(
    value: &Value,
    captured_at: DateTime<Utc>,
) -> Result<Vec<WeatherSample>, SourceError> {
    let point = Point {
        latitude: value["latitude"]
            .as_f64()
            .ok_or_else(|| SourceError::Invalid("latitude".into()))?,
        longitude: value["longitude"]
            .as_f64()
            .ok_or_else(|| SourceError::Invalid("longitude".into()))?,
    };
    point.validate().map_err(SourceError::Invalid)?;
    let hourly = &value["hourly"];
    let times = hourly["time"]
        .as_array()
        .ok_or_else(|| SourceError::Invalid("hourly.time".into()))?;
    if times.is_empty() || times.len() > 744 {
        return Err(SourceError::Invalid("invalid hourly sample count".into()));
    }
    let mut samples = Vec::new();
    for (index, timestamp) in times.iter().enumerate() {
        let seconds = timestamp
            .as_i64()
            .ok_or_else(|| SourceError::Invalid("non-unix timestamp".into()))?;
        let valid_at = DateTime::from_timestamp(seconds, 0)
            .ok_or_else(|| SourceError::Invalid("timestamp range".into()))?;
        let field = |name: &str| hourly[name].get(index).and_then(Value::as_f64);
        let sample = WeatherSample {
            source_id: "open-meteo".into(),
            station_id: format!("grid:{},{}", point.longitude, point.latitude),
            point,
            valid_at,
            available_at: captured_at,
            captured_at,
            is_forecast: true,
            temperature_c: field("temperature_2m"),
            humidity_pct: field("relative_humidity_2m"),
            wind_kmh: field("wind_speed_10m"),
            wind_from_deg: field("wind_direction_10m")
                .map(|direction| if direction == 360.0 { 0.0 } else { direction }),
            gust_kmh: field("wind_gusts_10m"),
        };
        sample.validate().map_err(SourceError::Invalid)?;
        samples.push(sample);
    }
    Ok(samples)
}
