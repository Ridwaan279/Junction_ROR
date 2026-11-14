//! Axum composition root; Agent B/C adapters plug into shared domain contracts.
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, patch, post},
};
use chrono::{DateTime, Duration, Utc};
use emberwatch_domain::*;
use emberwatch_platform::{
    Platform, PlatformError, Store,
    sources::{FirmsSource, OpenMeteoSource, http_client},
    uploads::{MAX_UPLOAD_BYTES, parse_upload},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{convert::Infallible, sync::Arc, time::Duration as StdDuration};
use tokio::sync::broadcast;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

pub const DEMO_DATASET: &str = include_str!("../../../contracts/fixtures/portugal-synthetic.json");

#[derive(Clone, Default)]
pub struct OperatorAccess {
    pub token: Option<String>,
    pub allow_local_mutations: bool,
}

impl OperatorAccess {
    fn require(&self, headers: &HeaderMap) -> Result<(), ApiError> {
        if self.allow_local_mutations {
            return Ok(());
        }
        let candidate = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        if let (Some(expected), Some(candidate)) = (&self.token, candidate) {
            // Compare every byte; don't log or echo credentials.
            let difference = expected
                .bytes()
                .zip(candidate.bytes())
                .fold(0u8, |diff, (a, b)| diff | (a ^ b));
            if candidate.len() == expected.len() && difference == 0 {
                return Ok(());
            }
        }
        Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "operator_required".into(),
            "Operator authorization required".into(),
        ))
    }
}

#[derive(Clone)]
struct AppState {
    platform: Platform,
    access: OperatorAccess,
}

pub struct ApiError(StatusCode, String, String);
impl From<PlatformError> for ApiError {
    fn from(error: PlatformError) -> Self {
        let (status, code) = match &error {
            PlatformError::Invalid(_) => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_input"),
            PlatformError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            PlatformError::SourceUnavailable => {
                (StatusCode::SERVICE_UNAVAILABLE, "source_unavailable")
            }
            PlatformError::Storage(_) => (StatusCode::SERVICE_UNAVAILABLE, "storage_unavailable"),
        };
        if matches!(&error, PlatformError::Storage(_)) {
            tracing::error!(%error, "state persistence failed");
        }
        Self(status, code.into(), error.to_string())
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.0,
            Json(json!({"error":{"code":self.1,"message":self.2}})),
        )
            .into_response()
    }
}
fn invalid(message: impl Into<String>) -> ApiError {
    ApiError(
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_input".into(),
        message.into(),
    )
}

pub fn router(platform: Platform, access: OperatorAccess) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/api/runs/{id}", get(run_snapshot))
        .route("/api/incidents", get(incidents))
        .route("/api/incidents/{id}", get(incident))
        .route("/api/incidents/{id}/timeline", get(timeline))
        .route("/api/incidents/{id}/forecast", get(response_pending))
        .route("/api/incidents/{id}/dispatch", post(response_pending))
        .route("/api/drones", get(response_pending))
        .route("/api/scenarios", get(scenarios).post(create_scenario))
        .route("/api/scenarios/{id}/load", post(load_scenario))
        .route("/api/scenarios/{id}/upload", post(upload))
        .route("/api/scenarios/{id}/play", post(play))
        .route("/api/scenarios/{id}/pause", post(pause))
        .route("/api/scenarios/{id}/seek", post(seek))
        .route("/api/sources", get(sources))
        .route("/api/sources/{id}", patch(configure_source))
        .route("/api/sources/{id}/run", post(run_source))
        .route("/api/stations", get(weather))
        .route("/api/wind-field", get(weather))
        .route("/api/events", get(events))
        .route("/contracts/openapi.yaml", get(openapi))
        .route("/contracts/schema.json", get(schema))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
        .layer(TraceLayer::new_for_http())
        .with_state(AppState { platform, access })
}

async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(json!({"status":"ok","contract_version":CONTRACT_VERSION,
        "storage":state.platform.storage_kind(),"live_run_id":live_run_id(),
        "geography":"mainland_portugal","bbox":PORTUGAL_BBOX}))
}
async fn openapi() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/yaml")],
        include_str!("../../../contracts/openapi.yaml"),
    )
}
async fn schema() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/schema+json")],
        include_str!("../../../contracts/schema.json"),
    )
}
async fn run_snapshot(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Scenario>, ApiError> {
    Ok(Json(state.platform.run(id).await?))
}

#[derive(Deserialize)]
struct RunQuery {
    run_id: Option<Uuid>,
    bbox: Option<String>,
    since: Option<DateTime<Utc>>,
    at: Option<DateTime<Utc>>,
}
impl RunQuery {
    fn run_id(&self) -> Uuid {
        self.run_id.unwrap_or_else(live_run_id)
    }
    fn bbox(&self) -> Result<Option<[f64; 4]>, ApiError> {
        self.bbox
            .as_ref()
            .map(|text| {
                let values: Vec<f64> = text
                    .split(',')
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| invalid("bbox requires four comma-separated numbers"))?;
                let bbox: [f64; 4] = values
                    .try_into()
                    .map_err(|_| invalid("bbox requires four numbers"))?;
                validate_bbox(bbox).map_err(invalid)?;
                Ok(bbox)
            })
            .transpose()
    }
}
async fn incidents(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
) -> Result<Json<Vec<Incident>>, ApiError> {
    let mut incidents = state.platform.run(query.run_id()).await?.incidents;
    if let Some(bbox) = query.bbox()? {
        incidents.retain(|i| i.point.in_bbox(bbox));
    }
    if let Some(since) = query.since {
        incidents.retain(|i| i.last_seen >= since);
    }
    Ok(Json(incidents))
}
async fn incident(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<RunQuery>,
) -> Result<Json<Incident>, ApiError> {
    let item = state
        .platform
        .run(query.run_id())
        .await?
        .incidents
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| PlatformError::NotFound("incident".into()))?;
    Ok(Json(item))
}
async fn timeline(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<RunQuery>,
) -> Result<Json<Vec<Evidence>>, ApiError> {
    let run = state.platform.run(query.run_id()).await?;
    let incident = run
        .incidents
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| PlatformError::NotFound("incident".into()))?;
    Ok(Json(
        run.evidence
            .into_iter()
            .filter(|e| incident.evidence_ids.contains(&e.id))
            .collect(),
    ))
}
async fn sources(State(state): State<AppState>) -> Json<Vec<SourceDescriptor>> {
    Json(state.platform.sources().await)
}
async fn scenarios(State(state): State<AppState>) -> Json<Vec<Scenario>> {
    Json(state.platform.scenarios().await)
}
async fn create_scenario(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(spec): Json<ScenarioSpec>,
) -> Result<(StatusCode, Json<Scenario>), ApiError> {
    state.access.require(&headers)?;
    Ok((
        StatusCode::CREATED,
        Json(state.platform.create_scenario(spec).await?),
    ))
}
#[derive(Deserialize)]
struct DatasetInput {
    #[serde(default)]
    evidence: Vec<Evidence>,
    #[serde(default)]
    weather: Vec<WeatherSample>,
}
async fn load_scenario(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<DatasetInput>,
) -> Result<Json<Scenario>, ApiError> {
    state.access.require(&headers)?;
    let run = state.platform.run(id).await?;
    if run.spec.mode == RunMode::Live {
        return Err(invalid("datasets must be loaded into a scenario"));
    }
    // Validate the entire upload before accepting any source batch.
    for item in &input.evidence {
        item.validate().map_err(invalid)?;
    }
    for item in &input.weather {
        item.validate().map_err(invalid)?;
    }
    if input.evidence.len() + input.weather.len() > 20_000 {
        return Err(invalid("dataset exceeds 20000 records"));
    }
    state
        .platform
        .load_into(id, input.evidence, input.weather)
        .await?;
    Ok(Json(state.platform.run(id).await?))
}
#[derive(Deserialize)]
struct UploadInput {
    format: String,
    data: String,
    source_id: String,
}
async fn upload(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<UploadInput>,
) -> Result<Json<Value>, ApiError> {
    state.access.require(&headers)?;
    if state.platform.run(id).await?.spec.mode == RunMode::Live {
        return Err(invalid("uploads require a scenario"));
    }
    let batch =
        parse_upload(&input.format, &input.data, &input.source_id, Utc::now()).map_err(invalid)?;
    let count = batch.evidence.len();
    let from = batch.evidence.iter().map(|e| e.available_at).min();
    let to = batch.evidence.iter().map(|e| e.available_at).max();
    let added = state.platform.ingest(id, batch).await?;
    Ok(Json(
        json!({"records":count,"added":added,"available_from":from,"available_to":to}),
    ))
}
#[derive(Default, Deserialize)]
struct PlayInput {
    speed: Option<f64>,
}
async fn play(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    input: Option<Json<PlayInput>>,
) -> Result<Json<Scenario>, ApiError> {
    Ok(Json(
        state
            .platform
            .play(id, input.and_then(|Json(body)| body.speed))
            .await?,
    ))
}
async fn pause(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Scenario>, ApiError> {
    Ok(Json(state.platform.pause(id).await?))
}
#[derive(Deserialize)]
struct SeekInput {
    at: DateTime<Utc>,
}
async fn seek(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<SeekInput>,
) -> Result<Json<Scenario>, ApiError> {
    Ok(Json(state.platform.seek(id, input.at).await?))
}
#[derive(Deserialize)]
struct SourcePatch {
    enabled: bool,
}
async fn configure_source(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<SourcePatch>,
) -> Result<Json<SourceDescriptor>, ApiError> {
    state.access.require(&headers)?;
    Ok(Json(
        state
            .platform
            .set_source_enabled(&id, input.enabled)
            .await?,
    ))
}
async fn run_source(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<SourceBatch>, ApiError> {
    state.access.require(&headers)?;
    Ok(Json(state.platform.run_source(&id).await?))
}
async fn weather(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
) -> Result<Json<Vec<WeatherSample>>, ApiError> {
    let mut samples = state.platform.run(query.run_id()).await?.weather;
    if let Some(bbox) = query.bbox()? {
        samples.retain(|sample| sample.point.in_bbox(bbox));
    }
    if let Some(at) = query.at {
        samples.retain(|sample| (sample.valid_at - at).abs() <= Duration::hours(1));
    }
    Ok(Json(samples))
}
async fn response_pending() -> ApiError {
    ApiError(
        StatusCode::NOT_IMPLEMENTED,
        "agent_c_pending".into(),
        "Forecast, drone and dispatch implementations belong to Agent C".into(),
    )
}
async fn events(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
    headers: HeaderMap,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let after = headers
        .get("last-event-id")
        .map(|header| {
            header
                .to_str()
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .ok_or_else(|| invalid("invalid Last-Event-ID"))
        })
        .transpose()?;
    let run_id = query.run_id();
    let mut subscription = state.platform.subscribe(run_id, after).await?;
    let stream = async_stream::stream! {
        for envelope in subscription.backlog {
            if let Ok(event) = encode_event(&envelope) { yield Ok(event); }
        }
        loop {
            match subscription.receiver.recv().await {
                Ok(envelope) if envelope.run_id == run_id => {
                    if let Ok(event) = encode_event(&envelope) { yield Ok(event); }
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    if let Ok(reset) = state.platform.subscribe(run_id, None).await {
                        subscription = reset;
                        for envelope in &subscription.backlog {
                            if let Ok(event) = encode_event(envelope) { yield Ok(event); }
                        }
                    } else { break; }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(StdDuration::from_secs(15))
            .text("heartbeat"),
    ))
}
fn encode_event(envelope: &EventEnvelope) -> Result<Event, serde_json::Error> {
    Ok(Event::default()
        .id(envelope.id.to_string())
        .event(&envelope.event_type)
        .data(serde_json::to_string(envelope)?))
}

pub async fn serve() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8000".into());
    let socket: std::net::SocketAddr = bind.parse()?;
    let access = OperatorAccess {
        token: std::env::var("OPERATOR_TOKEN")
            .ok()
            .filter(|s| !s.is_empty()),
        allow_local_mutations: std::env::var("ALLOW_LOCAL_MUTATIONS").as_deref() == Ok("true"),
    };
    anyhow::ensure!(
        !access.allow_local_mutations || socket.ip().is_loopback(),
        "ALLOW_LOCAL_MUTATIONS requires a loopback bind address"
    );
    let store = if let Ok(url) = std::env::var("DATABASE_URL") {
        Store::postgres(&url).await?
    } else {
        Store::file(std::env::var("STATE_PATH").unwrap_or_else(|_| ".state/platform.json".into()))
    };
    let client = http_client()?;
    let platform = Platform::open(
        store,
        vec![
            Arc::new(FirmsSource::new(
                client.clone(),
                std::env::var("FIRMS_MAP_KEY").ok(),
            )),
            Arc::new(OpenMeteoSource::new(client)),
        ],
    )
    .await?;
    let demo: ScenarioDataset = serde_json::from_str(DEMO_DATASET)?;
    if !platform
        .scenarios()
        .await
        .iter()
        .any(|run| run.spec.name == demo.spec.name)
    {
        platform.load_dataset(demo).await?;
    }
    if std::env::var("ENABLE_LIVE_SOURCES").as_deref() == Ok("true") {
        platform.set_source_enabled("open-meteo", true).await?;
        if std::env::var("FIRMS_MAP_KEY").is_ok() {
            platform.set_source_enabled("nasa-firms", true).await?;
        }
    }
    let replay_platform = platform.clone();
    let replay = tokio::spawn(async move {
        let mut interval = tokio::time::interval(StdDuration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut previous = tokio::time::Instant::now();
        loop {
            interval.tick().await;
            let now = tokio::time::Instant::now();
            let elapsed =
                Duration::from_std(now - previous).unwrap_or_else(|_| Duration::seconds(1));
            previous = now;
            if let Err(error) = replay_platform.advance(elapsed).await {
                tracing::error!(%error, "replay advance failed");
            }
        }
    });
    let live_platform = platform.clone();
    let scheduler = tokio::spawn(async move {
        let mut interval = tokio::time::interval(StdDuration::from_secs(10));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            live_platform.poll_due().await;
        }
    });
    let origin = std::env::var("CORS_ORIGIN").unwrap_or_else(|_| "http://localhost:3000".into());
    let cors = CorsLayer::new()
        .allow_origin(origin.parse::<HeaderValue>()?)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PATCH,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            axum::http::HeaderName::from_static("last-event-id"),
        ]);
    let app = router(platform, access).layer(cors);
    let listener = tokio::net::TcpListener::bind(socket).await?;
    tracing::info!(%bind, "EmberWatch Rust platform listening");
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    replay.abort();
    scheduler.abort();
    result?;
    Ok(())
}
