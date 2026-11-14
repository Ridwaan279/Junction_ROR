use emberwatch_domain::{
    Provenance, RunContext, Source, SourceBatch, SourceError, SourceFuture, SourceHealth,
    Timestamp, Weather,
};
use serde_json::Value;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

pub const OBSERVATIONS_URL: &str =
    "https://api.ipma.pt/open-data/observation/meteorology/stations/observations.json";
pub const STATIONS_URL: &str =
    "https://api.ipma.pt/open-data/observation/meteorology/stations/stations.json";
pub const FIRE_RISK_URL: &str =
    "https://api.ipma.pt/open-data/forecast/meteorology/rcm/rcm-d0.json";

/// IPMA documentation displays naive timestamps without explicitly naming their timezone.
/// Unknown refuses to invent an instant; UtcConfigured is an explicit operator assumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeBasis {
    Unknown,
    UtcConfigured,
}
pub struct IpmaSource {
    http: reqwest::Client,
    basis: TimeBasis,
}
impl IpmaSource {
    pub fn new(basis: TimeBasis) -> Result<Self, SourceError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("EmberWatch/0.1 weather research (IPMA attribution retained)")
            .build()
            .map_err(|_| {
                crate::error("HTTP_CLIENT", "Could not initialize weather HTTP client.")
            })?;
        Ok(Self { http, basis })
    }
    async fn get(&self, url: &str) -> Result<Value, SourceError> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|_| crate::error("IPMA_NETWORK", "IPMA request failed or timed out."))?;
        if !response.status().is_success() {
            return Err(crate::error(
                "IPMA_HTTP",
                "IPMA returned an unsuccessful status; retain prior data as stale.",
            ));
        }
        crate::context::read_json(response, 8 * 1024 * 1024).await
    }
    pub async fn fire_risk(&self, context: &RunContext) -> Result<SourceBatch, SourceError> {
        crate::live_only(context)?;
        let payload = self.get(FIRE_RISK_URL).await?;
        normalize_fire_risk(context, &payload)
    }
}
impl Source for IpmaSource {
    fn id(&self) -> &str {
        "ipma-observations"
    }
    fn fetch<'a>(&'a self, context: &'a RunContext) -> SourceFuture<'a> {
        Box::pin(async move {
            crate::live_only(context)?;
            let start = Instant::now();
            let (observations, stations) =
                tokio::try_join!(self.get(OBSERVATIONS_URL), self.get(STATIONS_URL))?;
            let mut result = normalize_observations(context, &observations, &stations, self.basis)?;
            result.usage.latency_ms = Some(crate::context::latency_ms(start));
            Ok(result)
        })
    }
}

pub fn normalize_observations(
    context: &RunContext,
    observations: &Value,
    stations: &Value,
    basis: TimeBasis,
) -> Result<SourceBatch, SourceError> {
    crate::live_only(context)?;
    let mut result = crate::batch(context, "ipma-observations");
    result.coverage.scope="IPMA hourly station records within requested bbox/time window; station points are not area coverage".into();
    let station_rows = stations.as_array().ok_or_else(|| {
        crate::error(
            "INVALID_IPMA_STATIONS",
            "Expected an IPMA station feature array.",
        )
    })?;
    let mut positions = HashMap::new();
    for station in station_rows {
        let Some(id) = identifier(&station["properties"]["idEstacao"]) else {
            continue;
        };
        let Some(coords) = station["geometry"]["coordinates"]
            .as_array()
            .filter(|values| values.len() == 2)
        else {
            continue;
        };
        if let (Some(lon), Some(lat)) = (coords[0].as_f64(), coords[1].as_f64()) {
            if crate::in_bbox([lon, lat], context.bbox) {
                positions.insert(id, [lon, lat]);
            }
        }
    }
    let times = observations.as_object().ok_or_else(|| {
        crate::error(
            "INVALID_IPMA_OBSERVATIONS",
            "Expected timestamp-keyed IPMA observations.",
        )
    })?;
    let mut skipped = 0usize;
    for (time, records) in times {
        let Some((valid_at, assumed)) = observation_time(time, basis) else {
            skipped += 1;
            continue;
        };
        if valid_at < context.window_start
            || valid_at > context.window_end
            || valid_at > result.captured_at
        {
            continue;
        }
        let Some(records) = records.as_object() else {
            skipped += 1;
            continue;
        };
        for (station_id, record) in records {
            let Some(position) = positions.get(station_id).copied() else {
                continue;
            };
            if !record.is_object() {
                skipped += 1;
                continue;
            }
            let mut flags = vec![
                "hourly_station_observation".into(),
                "gust_not_provided".into(),
            ];
            if assumed {
                flags.push("timezone_assumed_utc_by_operator".into());
            }
            let measured = value(record, "intensidadeVento").filter(|v| *v >= 0.0);
            let speed = measured.or_else(|| {
                value(record, "intensidadeVentoKM")
                    .filter(|v| *v >= 0.0)
                    .map(|v| v / 3.6)
            });
            if measured.is_none() && speed.is_some() {
                flags.push("wind_converted_from_kmh".into());
            }
            let record_hash = crate::hash(record.to_string().as_bytes());
            result.weather.push(Weather {
                id: crate::hash(
                    format!("{}|ipma|{station_id}|{valid_at}", context.run_id).as_bytes(),
                ),
                run_id: context.run_id.clone(),
                provenance: Provenance {
                    source_id: "ipma-observations".into(),
                    source_url: OBSERVATIONS_URL.into(),
                    origin_group: "ipma".into(),
                    external_id: Some(format!("{station_id}|{time}")),
                    event_at: Some(valid_at),
                    published_at: None,
                    captured_at: result.captured_at,
                    available_at: result.captured_at,
                    availability_basis: "captured_documented_api".into(),
                    evidence_hash: record_hash,
                    quality_flags: flags,
                },
                station_id: Some(station_id.clone()),
                position: Some(position),
                valid_at,
                forecast: false,
                temperature_c: value(record, "temperatura"),
                relative_humidity_pct: value(record, "humidade")
                    .filter(|v| (0.0..=100.0).contains(v)),
                wind_speed_mps: speed,
                wind_from_deg: record["idDireccVento"].as_i64().and_then(wind_direction),
                gust_mps: None,
                fire_risk: None,
            });
        }
    }
    result.coverage.complete = skipped == 0 && !times.is_empty();
    if skipped > 0 {
        result.coverage.notes.push(format!("Skipped {skipped} timestamp groups/records with unknown timezone or malformed data; do not substitute capture time."));
    }
    result.coverage.notes.push("Source: IPMA; linked IPMA logo required in web use. Hourly observations; absent gusts remain null.".into());
    result.health = if result.weather.is_empty() || skipped > 0 {
        SourceHealth::Partial
    } else {
        SourceHealth::Healthy
    };
    result.weather.sort_by(|a, b| {
        a.valid_at
            .cmp(&b.valid_at)
            .then(a.station_id.cmp(&b.station_id))
    });
    Ok(result)
}

pub fn wind_direction(code: i64) -> Option<f64> {
    match code {
        1 | 9 => Some(0.0),
        2..=8 => Some((code - 1) as f64 * 45.0),
        _ => None,
    }
}
fn value(record: &Value, key: &str) -> Option<f64> {
    record
        .get(key)
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && *v != -99.0)
}
fn identifier(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|v| v.to_string()))
}
fn observation_time(value: &str, basis: TimeBasis) -> Option<(Timestamp, bool)> {
    if let Ok(time) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some((time.to_utc(), false));
    }
    if basis == TimeBasis::Unknown {
        return None;
    }
    ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M"]
        .into_iter()
        .find_map(|format| chrono::NaiveDateTime::parse_from_str(value, format).ok())
        .map(|time| (time.and_utc(), true))
}

pub fn normalize_fire_risk(
    context: &RunContext,
    payload: &Value,
) -> Result<SourceBatch, SourceError> {
    crate::live_only(context)?;
    let mut result = crate::batch(context, "ipma-fire-risk");
    let day = payload["dataPrev"]
        .as_str()
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .ok_or_else(|| crate::error("INVALID_IPMA_RISK", "Missing valid daily risk date."))?;
    let valid_at = day
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| crate::error("INVALID_IPMA_RISK", "Invalid risk date."))?
        .and_utc();
    let locals = payload["local"]
        .as_object()
        .ok_or_else(|| crate::error("INVALID_IPMA_RISK", "Missing municipality risk records."))?;
    for (dico, record) in locals {
        let (Some(lon), Some(lat)) = (record["longitude"].as_f64(), record["latitude"].as_f64())
        else {
            continue;
        };
        if !crate::in_bbox([lon, lat], context.bbox) {
            continue;
        }
        let risk = record["data"]["rcm"]
            .as_u64()
            .filter(|v| (1..=5).contains(v))
            .map(|v| v as u8);
        result.weather.push(Weather {
            id: crate::hash(format!("{}|ipma-risk|{dico}|{day}", context.run_id).as_bytes()),
            run_id: context.run_id.clone(),
            provenance: Provenance {
                source_id: "ipma-fire-risk".into(),
                source_url: FIRE_RISK_URL.into(),
                origin_group: "ipma".into(),
                external_id: Some(dico.clone()),
                event_at: None,
                published_at: None,
                captured_at: result.captured_at,
                available_at: result.captured_at,
                availability_basis: "captured_daily_forecast; fileDate timezone unverified".into(),
                evidence_hash: crate::hash(record.to_string().as_bytes()),
                quality_flags: vec![
                    format!("daily_validity_date:{day}"),
                    "valid_at_is_utc_calendar_day_anchor_not_issuance_time".into(),
                ],
            },
            station_id: Some(format!("municipality:{dico}")),
            position: Some([lon, lat]),
            valid_at,
            forecast: true,
            temperature_c: None,
            relative_humidity_pct: None,
            wind_speed_mps: None,
            wind_from_deg: None,
            gust_mps: None,
            fire_risk: risk,
        });
    }
    result.coverage.scope = format!("IPMA municipality daily forecast for {day}, bbox-filtered");
    result.coverage.notes.push("Daily category, not measured fire activity; midnight UTC is a calendar-date anchor only. Source: IPMA.".into());
    result.health = if result.weather.is_empty() {
        SourceHealth::Partial
    } else {
        SourceHealth::Healthy
    };
    Ok(result)
}
