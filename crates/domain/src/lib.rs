//! Shared wire contracts. All timestamps serialize as RFC3339 UTC, positions as lon/lat.
/// Agent A's native persistence/API contract. Browser DTOs remain at this crate's root.
pub mod platform;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{future::Future, pin::Pin, sync::Arc};

pub type Timestamp = DateTime<Utc>;
pub const PORTUGAL_BBOX: [f64; 4] = [-9.6, 36.9, -6.1, 42.2];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DataMode {
    Live,
    Historical,
    Synthetic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
    pub scope: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Provenance {
    pub source_id: String,
    pub source_url: String,
    pub origin_group: String,
    pub external_id: Option<String>,
    pub event_at: Option<Timestamp>,
    pub published_at: Option<Timestamp>,
    pub captured_at: Timestamp,
    pub available_at: Timestamp,
    pub availability_basis: String,
    pub evidence_hash: String,
    pub quality_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Evidence {
    pub id: String,
    pub run_id: String,
    pub mode: DataMode,
    pub provenance: Provenance,
    /// GeoJSON order: longitude, latitude. Never imply a geocode was page-visible.
    pub position: Option<[f64; 2]>,
    pub uncertainty_m: Option<f64>,
    pub location_name: Option<String>,
    pub municipality: Option<String>,
    pub kind: String,
    pub raw_status: Option<String>,
    pub title: String,
    pub body: Option<String>,
    pub attributes: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Weather {
    pub id: String,
    pub run_id: String,
    pub provenance: Provenance,
    pub station_id: Option<String>,
    pub position: Option<[f64; 2]>,
    pub valid_at: Timestamp,
    pub forecast: bool,
    pub temperature_c: Option<f64>,
    pub relative_humidity_pct: Option<f64>,
    pub wind_speed_mps: Option<f64>,
    pub wind_from_deg: Option<f64>,
    pub gust_mps: Option<f64>,
    pub fire_risk: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
pub struct Usage {
    pub request_id: Option<String>,
    pub credits: Option<f64>,
    pub cache_age_ms: Option<u64>,
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceBatch {
    pub run_id: String,
    pub source_id: String,
    pub captured_at: Timestamp,
    pub health: SourceHealth,
    pub coverage: Coverage,
    pub evidence: Vec<Evidence>,
    pub weather: Vec<Weather>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceStatus {
    pub id: String,
    pub name: String,
    pub source_url: String,
    pub via: Option<String>,
    pub origin_group: String,
    pub enabled: bool,
    pub health: SourceHealth,
    pub coverage: Coverage,
    pub last_success_at: Option<Timestamp>,
    pub captured_at: Option<Timestamp>,
    pub data_at: Option<Timestamp>,
    pub attribution: String,
    pub notes: Vec<String>,
}

pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Utc::now()
    }
}
#[derive(Clone)]
pub struct FixedClock(pub Timestamp);
impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

#[derive(Clone)]
pub struct RunContext {
    pub run_id: String,
    pub scenario_id: Option<String>,
    pub mode: DataMode,
    pub epoch: u64,
    pub bbox: [f64; 4],
    pub window_start: Timestamp,
    pub window_end: Timestamp,
    pub clock: Arc<dyn Clock>,
}

#[derive(Debug, thiserror::Error)]
#[error("{code}: {message}")]
pub struct SourceError {
    pub code: String,
    pub message: String,
}
pub type SourceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<SourceBatch, SourceError>> + Send + 'a>>;
pub trait Source: Send + Sync {
    fn id(&self) -> &str;
    fn fetch<'a>(&'a self, context: &'a RunContext) -> SourceFuture<'a>;
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Incident {
    pub id: String,
    pub run_id: String,
    pub mode: DataMode,
    pub title: String,
    pub location_name: Option<String>,
    pub position: Option<[f64; 2]>,
    pub uncertainty_m: Option<f64>,
    pub status: String,
    pub evidence_level: String,
    pub evidence_ids: Vec<String>,
    pub origin_groups: Vec<String>,
    pub first_event_at: Option<Timestamp>,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Forecast {
    pub id: String,
    pub run_id: String,
    pub incident_id: String,
    pub epoch: u64,
    pub generated_at: Timestamp,
    pub input_evidence_ids: Vec<String>,
    pub input_weather_ids: Vec<String>,
    pub model_version: String,
    pub seed: u64,
    pub horizon_seconds: u32,
    pub illustrative: bool,
    pub geometry: Value,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct DronePlan {
    pub id: String,
    pub run_id: String,
    pub incident_id: String,
    pub epoch: u64,
    pub generated_at: Timestamp,
    pub simulated: bool,
    pub route: Vec<[f64; 2]>,
    pub return_route: Vec<[f64; 2]>,
    pub distance_m: f64,
    pub duration_seconds: f64,
    pub reserve_pct: f64,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Briefing {
    pub id: String,
    pub run_id: String,
    pub incident_id: String,
    pub epoch: u64,
    pub generated_at: Timestamp,
    pub text_pt: String,
    pub text_en: String,
    pub citations: Vec<String>,
    pub audio_url: Option<String>,
    pub simulated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Scenario {
    pub id: String,
    pub run_id: String,
    pub name: String,
    pub mode: DataMode,
    pub start_at: Timestamp,
    pub end_at: Timestamp,
    pub current_at: Timestamp,
    pub playing: bool,
    pub speed: f64,
    pub seed: u64,
    pub epoch: u64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EventEnvelope {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub run_id: String,
    pub scenario_id: Option<String>,
    pub epoch: u64,
    pub event_time: Timestamp,
    pub emitted_at: Timestamp,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Snapshot {
    pub scenario: Scenario,
    pub incidents: Vec<Incident>,
    pub evidence: Vec<Evidence>,
    pub weather: Vec<Weather>,
    pub sources: Vec<SourceStatus>,
    pub forecasts: Vec<Forecast>,
    pub drone_plans: Vec<DronePlan>,
    pub briefings: Vec<Briefing>,
}
