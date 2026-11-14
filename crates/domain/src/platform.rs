//! Versioned shared boundary types. No platform, HTTP, or database dependencies.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin};
use uuid::Uuid;

pub const CONTRACT_VERSION: &str = "0.1.0";
pub const PORTUGAL_BBOX: [f64; 4] = [-9.6, 36.9, -6.1, 42.2];

pub fn live_run_id() -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, b"emberwatch:portugal:live")
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Point {
    pub longitude: f64,
    pub latitude: f64,
}

impl Point {
    pub fn validate(self) -> Result<(), String> {
        if !self.longitude.is_finite()
            || !self.latitude.is_finite()
            || !(-180.0..=180.0).contains(&self.longitude)
            || !(-90.0..=90.0).contains(&self.latitude)
        {
            return Err("coordinates must be finite WGS84 longitude/latitude".into());
        }
        Ok(())
    }

    pub fn in_bbox(self, bbox: [f64; 4]) -> bool {
        self.longitude >= bbox[0]
            && self.longitude <= bbox[2]
            && self.latitude >= bbox[1]
            && self.latitude <= bbox[3]
    }
}

pub fn validate_bbox(bbox: [f64; 4]) -> Result<(), String> {
    Point {
        longitude: bbox[0],
        latitude: bbox[1],
    }
    .validate()?;
    Point {
        longitude: bbox[2],
        latitude: bbox[3],
    }
    .validate()?;
    if bbox[0] >= bbox[2] || bbox[1] >= bbox[3] {
        return Err("bbox must be west,south,east,north with positive area".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    Live,
    Historical,
    Synthetic,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Report,
    Official,
    Satellite,
    Context,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationalStatus {
    #[default]
    Unknown,
    Active,
    Contained,
    Resolved,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLevel {
    Reported,
    Corroborated,
    SatelliteSupported,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityBasis {
    Published,
    Captured,
    Synthetic,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Evidence {
    #[serde(default)]
    pub id: Uuid,
    pub source_id: String,
    pub external_id: String,
    pub origin_group: String,
    pub kind: EvidenceKind,
    pub url: String,
    pub title: String,
    pub place_name: String,
    #[serde(default)]
    pub municipality: Option<String>,
    pub point: Option<Point>,
    pub radius_m: f64,
    pub observed_at: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub available_at: DateTime<Utc>,
    pub availability_basis: AvailabilityBasis,
    pub captured_at: DateTime<Utc>,
    #[serde(default)]
    pub status: OperationalStatus,
    #[serde(default)]
    pub raw_status: Option<String>,
    #[serde(default)]
    pub is_official: bool,
    #[serde(default)]
    pub attrs: BTreeMap<String, Value>,
}

impl Evidence {
    pub fn validate(&self) -> Result<(), String> {
        for (label, value) in [
            ("source_id", &self.source_id),
            ("external_id", &self.external_id),
            ("origin_group", &self.origin_group),
            ("title", &self.title),
        ] {
            if value.trim().is_empty() || value.len() > 4096 {
                return Err(format!("{label} must contain 1..4096 bytes"));
            }
        }
        if !(self.url.starts_with("https://") || self.url.starts_with("http://")) {
            return Err("evidence URL must use HTTP(S)".into());
        }
        if !self.radius_m.is_finite() || !(0.0..=100_000.0).contains(&self.radius_m) {
            return Err("radius_m must be finite and within 0..100000".into());
        }
        if let Some(point) = self.point {
            point.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct WeatherSample {
    pub source_id: String,
    pub station_id: String,
    pub point: Point,
    pub valid_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub captured_at: DateTime<Utc>,
    pub is_forecast: bool,
    pub temperature_c: Option<f64>,
    pub humidity_pct: Option<f64>,
    pub wind_kmh: Option<f64>,
    /// Meteorological direction FROM which wind blows, clockwise from north.
    pub wind_from_deg: Option<f64>,
    pub gust_kmh: Option<f64>,
}

impl WeatherSample {
    pub fn validate(&self) -> Result<(), String> {
        self.point.validate()?;
        if self.source_id.trim().is_empty() || self.station_id.trim().is_empty() {
            return Err("weather source_id and station_id are required".into());
        }
        for value in [
            self.temperature_c,
            self.humidity_pct,
            self.wind_kmh,
            self.wind_from_deg,
            self.gust_kmh,
        ]
        .into_iter()
        .flatten()
        {
            if !value.is_finite() {
                return Err("weather values must be finite".into());
            }
        }
        if self
            .humidity_pct
            .is_some_and(|v| !(0.0..=100.0).contains(&v))
            || self
                .wind_from_deg
                .is_some_and(|v| !(0.0..360.0).contains(&v))
            || self.wind_kmh.is_some_and(|v| v < 0.0)
            || self.gust_kmh.is_some_and(|v| v < 0.0)
        {
            return Err("weather value outside its physical range".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Incident {
    pub id: Uuid,
    pub run_id: Uuid,
    pub point: Point,
    pub radius_m: f64,
    pub place_name: String,
    pub municipality: Option<String>,
    pub status: OperationalStatus,
    pub evidence_level: EvidenceLevel,
    pub official_source: bool,
    pub evidence_ids: Vec<Uuid>,
    pub origin_groups: Vec<String>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceHealth {
    Healthy,
    EmptyVerified,
    Partial,
    Stale,
    Blocked,
    Failed,
    Unconfigured,
    NeedsValidation,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Coverage {
    pub complete: bool,
    pub explicit_empty: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceDescriptor {
    pub id: String,
    pub name: String,
    pub fetch: String,
    pub origin_group: String,
    pub enabled: bool,
    pub cadence_s: u64,
    pub health: SourceHealth,
    pub validated: bool,
    pub replayable: bool,
    pub last_attempt: Option<DateTime<Utc>>,
    pub last_success: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub coverage: Option<Coverage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceBatch {
    pub source_id: String,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub weather: Vec<WeatherSample>,
    pub coverage: Coverage,
    pub captured_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RunContext {
    pub run_id: Uuid,
    pub mode: RunMode,
    pub bbox: [f64; 4],
    pub from: DateTime<Utc>,
    pub now: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("source is not configured")]
    Unconfigured,
    #[error("upstream refused the request")]
    Blocked,
    #[error("upstream request failed")]
    Unavailable,
    #[error("invalid upstream response: {0}")]
    Invalid(String),
}

pub type SourceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<SourceBatch, SourceError>> + Send + 'a>>;

pub trait Source: Send + Sync {
    fn descriptor(&self) -> SourceDescriptor;
    fn run(&self, context: RunContext) -> SourceFuture<'_>;
}

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub struct LiveClock;
impl Clock for LiveClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
pub struct FixedClock(pub DateTime<Utc>);
impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScenarioSpec {
    pub name: String,
    pub mode: RunMode,
    pub bbox: [f64; 4],
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub speed: f64,
    pub seed: u64,
}

impl ScenarioSpec {
    pub fn validate(&self) -> Result<(), String> {
        validate_bbox(self.bbox)?;
        if self.mode == RunMode::Live {
            return Err("scenario mode must be historical or synthetic".into());
        }
        if self.name.trim().is_empty() || self.name.len() > 200 {
            return Err("scenario name must contain 1..200 bytes".into());
        }
        if self.ends_at <= self.starts_at
            || (self.ends_at - self.starts_at) > chrono::Duration::days(31)
        {
            return Err("scenario window must be positive and at most 31 days".into());
        }
        if !self.speed.is_finite() || !(0.1..=3600.0).contains(&self.speed) {
            return Err("speed must be within 0.1..3600".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Scenario {
    pub id: Uuid,
    pub spec: ScenarioSpec,
    pub cursor: DateTime<Utc>,
    pub playing: bool,
    pub epoch: u64,
    /// Append-only evidence revisions; API views expose only available revisions.
    pub evidence: Vec<Evidence>,
    pub weather: Vec<WeatherSample>,
    pub incidents: Vec<Incident>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScenarioDataset {
    pub spec: ScenarioSpec,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub weather: Vec<WeatherSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EventEnvelope {
    pub id: u64,
    #[serde(rename = "type")]
    pub event_type: String,
    pub run_id: Uuid,
    pub scenario_id: Option<Uuid>,
    pub epoch: u64,
    pub event_time: DateTime<Utc>,
    pub emitted_at: DateTime<Utc>,
    pub payload: Value,
}

/// Agent C implements producers of these contracts; platform owns persistence.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Forecast {
    pub run_id: Uuid,
    pub incident_id: Uuid,
    pub epoch: u64,
    pub model_version: String,
    pub seed: u64,
    pub input_evidence_ids: Vec<Uuid>,
    pub generated_at: DateTime<Utc>,
    pub illustrative: bool,
    pub geojson: Value,
    pub narrative: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DronePlan {
    pub run_id: Uuid,
    pub incident_id: Uuid,
    pub epoch: u64,
    pub drone_id: Uuid,
    pub path_geojson: Value,
    pub eta_s: f64,
    pub battery_required_pct: f64,
    pub reason: String,
    pub simulated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Briefing {
    pub run_id: Uuid,
    pub incident_id: Uuid,
    pub epoch: u64,
    pub text_pt: String,
    pub text_en: String,
    pub source_urls: Vec<String>,
    pub audio_url: Option<String>,
    pub simulated: bool,
}
