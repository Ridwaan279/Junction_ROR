//! Compatibility adapter from Agent A's native platform to the browser contract.
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use emberwatch_domain::{self as wire, platform as native};
use emberwatch_platform::{Platform, PlatformError};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex, broadcast};
use uuid::Uuid;

type ResponseBundle = (wire::Forecast, wire::DronePlan, wire::Briefing);
type OutputCache = BTreeMap<(Uuid, String, u64), ResponseBundle>;
#[derive(Clone)]
struct Bridge {
    platform: Platform,
    outputs: Arc<Mutex<OutputCache>>,
    computed: broadcast::Sender<Uuid>,
    sequence: Arc<AtomicU64>,
    compute: emberwatch_response::ComputePool,
}
#[derive(Deserialize)]
struct Scope {
    run_id: Uuid,
}
type Result<T> = std::result::Result<T, Error>;
struct Error(StatusCode, String);
impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        (self.0, Json(json!({"error":self.1}))).into_response()
    }
}
impl From<PlatformError> for Error {
    fn from(error: PlatformError) -> Self {
        Self(
            match error {
                PlatformError::NotFound(_) => StatusCode::NOT_FOUND,
                PlatformError::Invalid(_) => StatusCode::BAD_REQUEST,
                _ => StatusCode::SERVICE_UNAVAILABLE,
            },
            error.to_string(),
        )
    }
}

pub fn router(platform: Platform) -> Router {
    let (computed, _) = broadcast::channel(64);
    let state = Bridge {
        platform,
        outputs: Arc::new(Mutex::new(BTreeMap::new())),
        computed,
        sequence: Arc::new(AtomicU64::new(1)),
        compute: emberwatch_response::ComputePool::new(2).expect("bounded pool"),
    };
    Router::new()
        .route("/healthz", get(health))
        .route("/api/scenarios", get(scenarios))
        .route("/api/snapshot", get(snapshot))
        .route("/api/incidents", get(incidents))
        .route("/api/sources", get(sources))
        .route("/api/incidents/{id}/dispatch", post(dispatch))
        .route("/api/scenarios/{id}/{command}", post(command))
        .route("/api/events", get(events))
        .with_state(state)
}
async fn health(State(state): State<Bridge>) -> Json<Value> {
    Json(
        json!({"status":"ok","storage":state.platform.storage_kind(),"backend":"agent-a","live_ingestion":"operator_gated"}),
    )
}
fn mode(mode: native::RunMode) -> wire::DataMode {
    match mode {
        native::RunMode::Live => wire::DataMode::Live,
        native::RunMode::Historical => wire::DataMode::Historical,
        native::RunMode::Synthetic => wire::DataMode::Synthetic,
    }
}
fn scenario(run: &native::Scenario) -> wire::Scenario {
    wire::Scenario {
        id: run.id.to_string(),
        run_id: run.id.to_string(),
        name: run.spec.name.clone(),
        mode: mode(run.spec.mode),
        start_at: run.spec.starts_at,
        end_at: run.spec.ends_at,
        current_at: run.cursor,
        playing: run.playing,
        speed: run.spec.speed,
        seed: run.spec.seed,
        epoch: run.epoch,
        description: if run.spec.mode == native::RunMode::Synthetic {
            "Invented training observations, not a real incident or historical reconstruction."
        } else {
            "Agent A platform run; attribution and availability retained."
        }
        .into(),
    }
}
fn digest(value: &impl serde::Serialize) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("wire record"))
    )
}
fn registered_url(id: &str) -> Option<&'static str> {
    match id {
        "fogos-context" => Some("https://fogos.pt/pt"),
        "ipma" => Some("https://api.ipma.pt/"),
        "nasa-firms" => Some("https://firms.modaps.eosdis.nasa.gov/"),
        "open-meteo" => Some("https://open-meteo.com/"),
        "synthetic-weather" => Some("https://example.invalid/synthetic/weather"),
        _ => None,
    }
}
fn evidence(run: &native::Scenario, item: &native::Evidence) -> wire::Evidence {
    wire::Evidence {
        id: item.id.to_string(),
        run_id: run.id.to_string(),
        mode: mode(run.spec.mode),
        provenance: wire::Provenance {
            source_id: item.source_id.clone(),
            source_url: item.url.clone(),
            origin_group: item.origin_group.clone(),
            external_id: Some(item.external_id.clone()),
            event_at: item.observed_at,
            published_at: item.published_at,
            captured_at: item.captured_at,
            available_at: item.available_at,
            availability_basis: match item.availability_basis {
                native::AvailabilityBasis::Published => "published",
                native::AvailabilityBasis::Captured => "captured",
                native::AvailabilityBasis::Synthetic => "synthetic",
            }
            .into(),
            evidence_hash: digest(item),
            quality_flags: if run.spec.mode == native::RunMode::Synthetic {
                vec!["synthetic".into()]
            } else {
                vec!["agent_a_normalized".into()]
            },
        },
        position: item.point.map(|p| [p.longitude, p.latitude]),
        uncertainty_m: Some(item.radius_m),
        location_name: Some(item.place_name.clone()),
        municipality: item.municipality.clone(),
        kind: match item.kind {
            native::EvidenceKind::Satellite => "satellite",
            native::EvidenceKind::Official => "official",
            native::EvidenceKind::Report => "public_report",
            native::EvidenceKind::Context => "context",
        }
        .into(),
        raw_status: item.raw_status.clone(),
        title: item.title.clone(),
        body: None,
        attributes: json!(item.attrs),
    }
}
async fn view(state: &Bridge, id: Uuid) -> Result<wire::Snapshot> {
    let run = state.platform.run(id).await?;
    let evidence: Vec<_> = run.evidence.iter().map(|e| evidence(&run, e)).collect();
    let incidents = run
        .incidents
        .iter()
        .map(|i| wire::Incident {
            id: i.id.to_string(),
            run_id: run.id.to_string(),
            mode: mode(run.spec.mode),
            title: run
                .evidence
                .iter()
                .find(|e| i.evidence_ids.contains(&e.id))
                .map_or_else(|| i.place_name.clone(), |e| e.title.clone()),
            location_name: Some(i.place_name.clone()),
            position: Some([i.point.longitude, i.point.latitude]),
            uncertainty_m: Some(i.radius_m),
            status: serde_json::to_value(i.status)
                .unwrap()
                .as_str()
                .unwrap()
                .into(),
            evidence_level: serde_json::to_value(i.evidence_level)
                .unwrap()
                .as_str()
                .unwrap()
                .into(),
            evidence_ids: i.evidence_ids.iter().map(ToString::to_string).collect(),
            origin_groups: i.origin_groups.clone(),
            first_event_at: run
                .evidence
                .iter()
                .filter(|e| i.evidence_ids.contains(&e.id))
                .filter_map(|e| e.observed_at)
                .min(),
            updated_at: i.last_seen,
        })
        .collect();
    // Unmapped weather URLs remain unknown in the native contract. Omit those
    // samples here rather than invent non-null provenance for the browser DTO.
    let weather = run
        .weather
        .iter()
        .filter(|w| w.valid_at <= run.cursor)
        .filter_map(|w| {
            let source_url = registered_url(&w.source_id)?;
            Some(wire::Weather {
                id: digest(w),
                run_id: run.id.to_string(),
                provenance: wire::Provenance {
                    source_id: w.source_id.clone(),
                    source_url: source_url.into(),
                    origin_group: w.source_id.clone(),
                    external_id: Some(w.station_id.clone()),
                    event_at: Some(w.valid_at),
                    published_at: None,
                    captured_at: w.captured_at,
                    available_at: w.available_at,
                    availability_basis: "native_available_at".into(),
                    evidence_hash: digest(w),
                    quality_flags: vec![
                        if run.spec.mode == native::RunMode::Synthetic {
                            "synthetic"
                        } else {
                            "registered_adapter_url"
                        }
                        .into(),
                    ],
                },
                station_id: Some(w.station_id.clone()),
                position: Some([w.point.longitude, w.point.latitude]),
                valid_at: w.valid_at,
                forecast: w.is_forecast,
                temperature_c: w.temperature_c,
                relative_humidity_pct: w.humidity_pct,
                wind_speed_mps: w.wind_kmh.map(|v| v / 3.6),
                wind_from_deg: w.wind_from_deg,
                gust_mps: w.gust_kmh.map(|v| v / 3.6),
                fire_risk: None,
            })
        })
        .collect();
    let mut sources = Vec::new();
    for s in state.platform.sources().await {
        let replay = run.spec.mode != native::RunMode::Live;
        if let Some(url) = registered_url(&s.id) {
            sources.push(wire::SourceStatus {
                id: s.id,
                name: s.name,
                source_url: url.into(),
                via: (s.fetch == "context_scrape").then(|| "Context.dev".into()),
                origin_group: s.origin_group,
                enabled: s.enabled,
                health: if replay && s.last_attempt.is_some() {
                    wire::SourceHealth::NeedsValidation
                } else {
                    serde_json::from_value(serde_json::to_value(s.health).unwrap()).unwrap()
                },
                coverage: wire::Coverage {
                    complete: false,
                    explicit_empty: false,
                    scope: "Current live-source configuration, not replay coverage".into(),
                    notes: s.last_error.into_iter().collect(),
                },
                last_success_at: if replay { None } else { s.last_success },
                captured_at: if replay { None } else { s.last_attempt },
                data_at: None,
                attribution: url.into(),
                notes: vec!["Live registry metadata is not a capture from this replay.".into()],
            });
        }
    }
    let mut visible_sources = BTreeSet::new();
    for item in &evidence {
        if visible_sources.insert(item.provenance.source_id.clone())
            && !sources.iter().any(|s| s.id == item.provenance.source_id)
        {
            sources.push(wire::SourceStatus {
                id: item.provenance.source_id.clone(),
                name: item.provenance.source_id.clone(),
                source_url: item.provenance.source_url.clone(),
                via: None,
                origin_group: item.provenance.origin_group.clone(),
                enabled: false,
                health: wire::SourceHealth::Healthy,
                coverage: wire::Coverage {
                    complete: false,
                    explicit_empty: false,
                    scope: "Visible scenario observations only".into(),
                    notes: vec!["Not national coverage".into()],
                },
                last_success_at: Some(item.provenance.captured_at),
                captured_at: Some(item.provenance.captured_at),
                data_at: item.provenance.event_at,
                attribution: if run.spec.mode == native::RunMode::Synthetic {
                    "EmberWatch synthetic training data".into()
                } else {
                    item.provenance.source_id.clone()
                },
                notes: vec![],
            });
        }
    }
    let (mut forecasts, mut drone_plans, mut briefings) = (vec![], vec![], vec![]);
    for ((run_id, incident_id, epoch), (f, d, b)) in state.outputs.lock().await.iter() {
        if *run_id == id
            && *epoch == run.epoch
            && f.generated_at <= run.cursor
            && run
                .incidents
                .iter()
                .any(|i| i.id.to_string() == *incident_id)
            && f.input_evidence_ids
                .iter()
                .all(|id| evidence.iter().any(|e| &e.id == id))
        {
            forecasts.push(f.clone());
            drone_plans.push(d.clone());
            briefings.push(b.clone());
        }
    }
    Ok(wire::Snapshot {
        scenario: scenario(&run),
        incidents,
        evidence,
        weather,
        sources,
        forecasts,
        drone_plans,
        briefings,
    })
}
async fn scenarios(State(state): State<Bridge>) -> Json<Vec<wire::Scenario>> {
    Json(
        state
            .platform
            .scenarios()
            .await
            .iter()
            .map(scenario)
            .collect(),
    )
}
async fn snapshot(
    State(state): State<Bridge>,
    Query(scope): Query<Scope>,
) -> Result<Json<wire::Snapshot>> {
    Ok(Json(view(&state, scope.run_id).await?))
}
async fn incidents(
    State(state): State<Bridge>,
    Query(scope): Query<Scope>,
) -> Result<Json<Vec<wire::Incident>>> {
    Ok(Json(view(&state, scope.run_id).await?.incidents))
}
async fn sources(
    State(state): State<Bridge>,
    Query(scope): Query<Scope>,
) -> Result<Json<Vec<wire::SourceStatus>>> {
    Ok(Json(view(&state, scope.run_id).await?.sources))
}
async fn command(
    State(state): State<Bridge>,
    Query(scope): Query<Scope>,
    Path((id, action)): Path<(Uuid, String)>,
    body: Option<Json<Value>>,
) -> Result<Json<wire::Snapshot>> {
    if id != scope.run_id {
        return Err(Error(
            StatusCode::NOT_FOUND,
            "scenario/run scope mismatch".into(),
        ));
    }
    let input = body.map_or(json!({}), |Json(v)| v);
    match action.as_str() {
        "play" => {
            state
                .platform
                .play(
                    id,
                    input
                        .get("speed")
                        .map(|v| {
                            v.as_f64().ok_or_else(|| {
                                Error(StatusCode::BAD_REQUEST, "invalid speed".into())
                            })
                        })
                        .transpose()?,
                )
                .await?;
        }
        "pause" => {
            state.platform.pause(id).await?;
        }
        "seek" | "reset" => {
            let at = if action == "reset" {
                state.platform.run(id).await?.spec.starts_at
            } else {
                input
                    .get("at")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error(StatusCode::BAD_REQUEST, "at required".into()))?
                    .parse()
                    .map_err(|_| Error(StatusCode::BAD_REQUEST, "invalid timestamp".into()))?
            };
            state.platform.seek(id, at).await?;
            state
                .outputs
                .lock()
                .await
                .retain(|(run_id, _, _), _| *run_id != id);
        }
        _ => return Err(Error(StatusCode::NOT_FOUND, "unknown command".into())),
    }
    Ok(Json(view(&state, id).await?))
}
async fn dispatch(
    State(state): State<Bridge>,
    Query(scope): Query<Scope>,
    Path(id): Path<String>,
) -> Result<Json<wire::Snapshot>> {
    let mut snapshot = view(&state, scope.run_id).await?;
    let incident = snapshot
        .incidents
        .iter()
        .find(|i| i.id == id)
        .cloned()
        .ok_or_else(|| {
            Error(
                StatusCode::NOT_FOUND,
                "incident not found in this run".into(),
            )
        })?;
    snapshot
        .evidence
        .retain(|e| incident.evidence_ids.contains(&e.id));
    let at = snapshot.scenario.current_at;
    let epoch = snapshot.scenario.epoch;
    let context = wire::RunContext {
        run_id: snapshot.scenario.run_id.clone(),
        scenario_id: Some(snapshot.scenario.id.clone()),
        mode: snapshot.scenario.mode,
        epoch,
        bbox: native::PORTUGAL_BBOX,
        window_start: snapshot.scenario.start_at,
        window_end: snapshot.scenario.end_at,
        clock: Arc::new(wire::FixedClock(at)),
    };
    let output = state
        .compute
        .run(move || {
            emberwatch_response::demo_bundle(
                &context,
                &incident,
                &snapshot.evidence,
                &snapshot.weather,
            )
        })
        .await
        .map_err(|e| Error(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;
    let current = state.platform.run(scope.run_id).await?;
    if current.epoch != epoch || current.cursor != at {
        return Err(Error(
            StatusCode::CONFLICT,
            "run changed during computation; retry".into(),
        ));
    }
    state
        .outputs
        .lock()
        .await
        .insert((scope.run_id, id, epoch), output);
    let _ = state.computed.send(scope.run_id);
    Ok(Json(view(&state, scope.run_id).await?))
}
async fn envelope(state: &Bridge, id: Uuid, kind: &str) -> Result<Event> {
    let snapshot = view(state, id).await?;
    let event = wire::EventEnvelope {
        id: format!(
            "platform:{}",
            state.sequence.fetch_add(1, Ordering::Relaxed)
        ),
        event_type: kind.into(),
        run_id: id.to_string(),
        scenario_id: (snapshot.scenario.mode != wire::DataMode::Live).then(|| id.to_string()),
        epoch: snapshot.scenario.epoch,
        event_time: snapshot.scenario.current_at,
        emitted_at: chrono::Utc::now(),
        payload: json!(snapshot),
    };
    Ok(Event::default()
        .id(&event.id)
        .event(kind)
        .json_data(event)
        .expect("wire event"))
}
async fn events(
    State(state): State<Bridge>,
    Query(scope): Query<Scope>,
) -> Result<Sse<impl futures::Stream<Item = std::result::Result<Event, Infallible>>>> {
    let mut subscription = state.platform.subscribe(scope.run_id, None).await?;
    let mut computed = state.computed.subscribe();
    let initial = envelope(&state, scope.run_id, "run.reset").await?;
    let stream = async_stream::stream! {
        yield Ok(initial);
        loop {
            let kind=tokio::select! {
                event=subscription.receiver.recv()=>match event {
                    Ok(event) if event.run_id==scope.run_id=>event.event_type,
                    Ok(_)=>continue,
                    Err(broadcast::error::RecvError::Lagged(_))=>"run.reset".into(),
                    Err(_)=>break,
                },
                run=computed.recv()=>match run {
                    Ok(id) if id==scope.run_id=>"drone.plan".into(),Ok(_)=>continue,
                    Err(broadcast::error::RecvError::Lagged(_))=>"run.reset".into(),Err(_)=>break,
                }
            };
            match envelope(&state,scope.run_id,&kind).await {Ok(event)=>yield Ok(event),Err(_)=>break}
        }
    };
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    ))
}
