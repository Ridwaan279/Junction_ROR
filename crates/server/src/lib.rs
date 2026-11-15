use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{Method, StatusCode},
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use emberwatch_platform::{Engine, persistence::PgStore, *};
use futures::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex, broadcast};
use tokio_stream::wrappers::BroadcastStream;
use tower_http::cors::CorsLayer;
pub mod firecast_api;
pub mod live_api;
pub mod platform_api;
pub mod platform_ui;
mod webhook;

#[derive(Clone)]
struct AppState {
    engine: Arc<Mutex<Engine>>,
    store: Option<PgStore>,
    events: broadcast::Sender<EventEnvelope>,
    sequence: Arc<AtomicU64>,
    compute: emberwatch_response::ComputePool,
    firecast: firecast_api::FirecastState,
}
#[derive(Deserialize)]
struct RunQuery {
    run_id: String,
}
type ApiResult<T> = Result<T, ApiError>;
struct ApiError(StatusCode, String);
impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.0, Json(json!({"error":self.1}))).into_response()
    }
}
fn scope(engine: &Engine, run_id: &str) -> ApiResult<()> {
    if engine.scenario.run_id != run_id {
        return Err(ApiError(StatusCode::NOT_FOUND, "run not found".into()));
    }
    Ok(())
}

/// Router with manual, offline forecasting (tests and embedded use).
pub fn router(engine: Engine, store: Option<PgStore>) -> Router {
    router_with(engine, store, firecast_api::FirecastOptions::offline())
}

pub fn router_with(
    engine: Engine,
    store: Option<PgStore>,
    firecast: firecast_api::FirecastOptions,
) -> Router {
    router_with_live_options(engine, store, firecast, live_api::LiveOptions::offline())
}
pub fn router_with_live_options(
    engine: Engine,
    store: Option<PgStore>,
    firecast: firecast_api::FirecastOptions,
    live: live_api::LiveOptions,
) -> Router {
    let (events, _) = broadcast::channel(64);
    let state = AppState {
        engine: Arc::new(Mutex::new(engine)),
        store,
        events,
        sequence: Arc::new(AtomicU64::new(1)),
        compute: emberwatch_response::ComputePool::new(2).expect("bounded compute pool"),
        firecast: firecast_api::FirecastState::new(firecast),
    };
    let ticker = state.clone();
    tokio::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_secs(1));
        timer.tick().await;
        let mut previous = tokio::time::Instant::now();
        let mut ticks: u64 = 0;
        loop {
            timer.tick().await;
            ticks += 1;
            if ticks % 3 == 1 {
                firecast_api::auto_trigger(&ticker).await;
            }
            let elapsed = previous.elapsed();
            previous = tokio::time::Instant::now();
            let mut engine = ticker.engine.lock().await;
            if !engine.scenario.playing {
                continue;
            }
            let mut candidate = engine.clone();
            candidate.advance(chrono::Duration::from_std(elapsed).unwrap_or_default());
            if let Some(store) = &ticker.store
                && let Err(error) = store.save(&candidate).await
            {
                tracing::error!(%error,"tick persistence failed");
                engine.scenario.playing = false;
                continue;
            }
            *engine = candidate;
            publish(&ticker, &engine, "scenario.tick");
        }
    });
    let origins = match std::env::var("WEB_ORIGIN") {
        Ok(origin) => vec![
            origin
                .parse::<axum::http::HeaderValue>()
                .expect("valid WEB_ORIGIN"),
        ],
        Err(_) => vec![
            "http://localhost:3000".parse().unwrap(),
            "http://127.0.0.1:3000".parse().unwrap(),
        ],
    };
    Router::new()
        .route("/healthz", get(health))
        .route("/api/scenarios", get(scenarios))
        .route("/api/snapshot", get(snapshot))
        .route("/api/incidents", get(incidents))
        .route("/api/incidents/{id}", get(incident))
        .route("/api/incidents/{id}/timeline", get(timeline))
        .route("/api/incidents/{id}/forecast", get(forecast))
        .route("/api/incidents/{id}/dispatch", post(dispatch))
        .route("/api/incidents/{id}/firecast", post(firecast_api::handler))
        .route("/api/sources", get(sources))
        .route("/api/stations", get(stations))
        .route("/api/drones", get(drones))
        .route("/api/briefings", get(briefings))
        .route("/api/events", get(sse_events))
        .route("/api/scenarios/{id}/{command}", post(command))
        .route(
            "/webhooks/context",
            post(webhook::receive).layer(axum::extract::DefaultBodyLimit::max(1024 * 1024)),
        )
        .with_state(state)
        .merge(live_api::router(live))
        .layer(
            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods([Method::GET, Method::POST])
                .allow_headers([axum::http::header::CONTENT_TYPE]),
        )
}

fn publish(state: &AppState, engine: &Engine, kind: &str) {
    let seq = state.sequence.fetch_add(1, Ordering::Relaxed);
    let envelope = engine.envelope(seq, kind, json!(engine.snapshot()));
    let _ = state.events.send(envelope);
}

async fn dispatch(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
    Path(id): Path<String>,
) -> ApiResult<Json<Snapshot>> {
    let engine = state.engine.lock().await;
    scope(&engine, &query.run_id)?;
    let snapshot = engine.snapshot();
    let incident = find_incident(&snapshot, &id)?;
    if snapshot.scenario.mode != DataMode::Synthetic {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "demo response requires a synthetic run".into(),
        ));
    }
    let context = RunContext {
        run_id: snapshot.scenario.run_id.clone(),
        scenario_id: Some(snapshot.scenario.id.clone()),
        mode: snapshot.scenario.mode,
        epoch: snapshot.scenario.epoch,
        bbox: PORTUGAL_BBOX,
        window_start: snapshot.scenario.start_at,
        window_end: snapshot.scenario.end_at,
        clock: Arc::new(FixedClock(snapshot.scenario.current_at)),
    };
    let at = snapshot.scenario.current_at;
    let epoch = snapshot.scenario.epoch;
    drop(engine);
    if let Some(result) = state.firecast.cached(&id, epoch).await {
        // Hazard-aware mission from the physics forecast: predicted fire is no-fly,
        // predicted smoke is penalised, the task point is a flank of the 1-hour front.
        let plan_context = context.clone();
        let plan_incident = incident.clone();
        let evidence = snapshot.evidence.clone();
        let (plan, briefing) = state
            .compute
            .run(move || {
                let plan = emberwatch_firecast::plan_drone(&plan_context, &plan_incident, &result)
                    .map_err(emberwatch_response::ComputeError::InvalidInput)?;
                let briefing = emberwatch_response::build_briefing(
                    &plan_context,
                    &plan_incident,
                    &evidence,
                    Some(&plan),
                )?;
                Ok((plan, briefing))
            })
            .await
            .map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;
        let mut engine = state.engine.lock().await;
        if engine.scenario.epoch != epoch {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "scenario changed during computation; retry".into(),
            ));
        }
        let mut candidate = engine.clone();
        candidate.drone_plans.retain(|f| f.incident_id != id);
        candidate.drone_plans.push(plan);
        candidate.briefings.retain(|f| f.incident_id != id);
        candidate.briefings.push(briefing);
        if let Some(store) = &state.store {
            store.save(&candidate).await.map_err(|_| {
                ApiError(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "persistence unavailable".into(),
                )
            })?;
        }
        *engine = candidate;
        publish(&state, &engine, "drone.plan");
        return Ok(Json(engine.snapshot()));
    }
    let (forecast, plan, briefing) = state
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
        .map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;
    let mut engine = state.engine.lock().await;
    if engine.scenario.epoch != epoch || engine.scenario.current_at != at {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "scenario changed during computation; retry".into(),
        ));
    }
    let mut candidate = engine.clone();
    candidate.forecasts.retain(|f| f.incident_id != id);
    candidate.forecasts.push(forecast);
    candidate.drone_plans.retain(|f| f.incident_id != id);
    candidate.drone_plans.push(plan);
    candidate.briefings.retain(|f| f.incident_id != id);
    candidate.briefings.push(briefing);
    if let Some(store) = &state.store {
        store.save(&candidate).await.map_err(|_| {
            ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "persistence unavailable".into(),
            )
        })?;
    }
    *engine = candidate;
    publish(&state, &engine, "drone.plan");
    Ok(Json(engine.snapshot()))
}
async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(
        json!({"status":"ok", "storage": if state.store.is_some() {"postgres"} else {"memory"}, "live_ingestion":false}),
    )
}
async fn scenarios(State(state): State<AppState>) -> Json<Vec<Scenario>> {
    Json(vec![state.engine.lock().await.scenario.clone()])
}
async fn snapshot(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
) -> ApiResult<Json<Snapshot>> {
    let engine = state.engine.lock().await;
    scope(&engine, &query.run_id)?;
    Ok(Json(engine.snapshot()))
}
async fn incidents(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
) -> ApiResult<Json<Vec<Incident>>> {
    let engine = state.engine.lock().await;
    scope(&engine, &query.run_id)?;
    Ok(Json(engine.snapshot().incidents))
}
async fn sources(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
) -> ApiResult<Json<Vec<SourceStatus>>> {
    let engine = state.engine.lock().await;
    scope(&engine, &query.run_id)?;
    Ok(Json(engine.snapshot().sources))
}
async fn stations(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
) -> ApiResult<Json<Vec<Weather>>> {
    let engine = state.engine.lock().await;
    scope(&engine, &query.run_id)?;
    Ok(Json(engine.snapshot().weather))
}
async fn drones(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
) -> ApiResult<Json<Vec<DronePlan>>> {
    let engine = state.engine.lock().await;
    scope(&engine, &query.run_id)?;
    Ok(Json(engine.snapshot().drone_plans))
}
async fn briefings(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
) -> ApiResult<Json<Vec<Briefing>>> {
    let engine = state.engine.lock().await;
    scope(&engine, &query.run_id)?;
    Ok(Json(engine.snapshot().briefings))
}
fn find_incident(snapshot: &Snapshot, id: &str) -> ApiResult<Incident> {
    snapshot
        .incidents
        .iter()
        .find(|i| i.id == id)
        .cloned()
        .ok_or_else(|| {
            ApiError(
                StatusCode::NOT_FOUND,
                "incident not found in this run".into(),
            )
        })
}
async fn incident(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
    Path(id): Path<String>,
) -> ApiResult<Json<Incident>> {
    let engine = state.engine.lock().await;
    scope(&engine, &query.run_id)?;
    Ok(Json(find_incident(&engine.snapshot(), &id)?))
}
async fn timeline(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<Evidence>>> {
    let engine = state.engine.lock().await;
    scope(&engine, &query.run_id)?;
    let snapshot = engine.snapshot();
    let incident = find_incident(&snapshot, &id)?;
    Ok(Json(
        snapshot
            .evidence
            .into_iter()
            .filter(|e| incident.evidence_ids.contains(&e.id))
            .collect(),
    ))
}
async fn forecast(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<Forecast>>> {
    let engine = state.engine.lock().await;
    scope(&engine, &query.run_id)?;
    let snapshot = engine.snapshot();
    find_incident(&snapshot, &id)?;
    Ok(Json(
        snapshot
            .forecasts
            .into_iter()
            .filter(|f| f.incident_id == id)
            .collect(),
    ))
}
async fn command(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
    Path((id, action)): Path<(String, String)>,
    body: Option<Json<Value>>,
) -> ApiResult<Json<Snapshot>> {
    let mut engine = state.engine.lock().await;
    scope(&engine, &query.run_id)?;
    if engine.scenario.id != id {
        return Err(ApiError(StatusCode::NOT_FOUND, "scenario not found".into()));
    }
    let body = body.map_or(json!({}), |Json(value)| value);
    let mut candidate = engine.clone();
    let invalid = |message| ApiError(StatusCode::BAD_REQUEST, message);
    match action.as_str() {
        "play" => candidate
            .play(
                body.get("speed")
                    .map_or(Ok(candidate.scenario.speed), |v| {
                        v.as_f64().ok_or_else(|| invalid("invalid speed".into()))
                    })?,
            )
            .map_err(invalid)?,
        "pause" => candidate.scenario.playing = false,
        "reset" => candidate
            .seek(candidate.scenario.start_at)
            .map_err(invalid)?,
        "seek" => {
            let at = body
                .get("at")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("seek requires at (RFC3339)".into()))?;
            let at = chrono::DateTime::parse_from_rfc3339(at)
                .map_err(|_| invalid("invalid seek timestamp".into()))?
                .with_timezone(&chrono::Utc);
            candidate.seek(at).map_err(invalid)?;
        }
        _ => {
            return Err(ApiError(
                StatusCode::NOT_FOUND,
                "unknown scenario command".into(),
            ));
        }
    }
    if let Some(store) = &state.store {
        store.save(&candidate).await.map_err(|e| {
            tracing::error!(%e,"persistence failed");
            ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "persistence unavailable".into(),
            )
        })?;
    }
    *engine = candidate;
    publish(&state, &engine, "run.reset");
    Ok(Json(engine.snapshot()))
}
async fn sse_events(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    // Subscribe before snapshot acquisition so an update cannot fall between them.
    let receiver = state.events.subscribe();
    let engine = state.engine.lock().await;
    scope(&engine, &query.run_id)?;
    let initial_sequence = state.sequence.fetch_add(1, Ordering::Relaxed);
    let initial = engine.envelope(initial_sequence, "run.reset", json!(engine.snapshot()));
    let run_id = query.run_id.clone();
    let scenario_id = engine.scenario.id.clone();
    let epoch = engine.scenario.epoch;
    drop(engine);
    let updates = BroadcastStream::new(receiver).filter_map(move |item| {
        let envelope = match item {
            Ok(envelope)
                if envelope.run_id == run_id
                    && envelope
                        .id
                        .rsplit(':')
                        .next()
                        .and_then(|part| part.parse::<u64>().ok())
                        .is_some_and(|sequence| sequence > initial_sequence) =>
            {
                Some(envelope)
            }
            Ok(_) => None,
            Err(_) => Some(EventEnvelope {
                id: "resync".into(),
                event_type: "resync.required".into(),
                run_id: run_id.clone(),
                scenario_id: Some(scenario_id.clone()),
                epoch,
                event_time: chrono::Utc::now(),
                emitted_at: chrono::Utc::now(),
                payload: json!({"reason":"subscriber lagged; fetch /api/snapshot"}),
            }),
        };
        futures::future::ready(envelope.map(|e| Ok(sse_event(e))))
    });
    Ok(
        Sse::new(futures::stream::once(async move { Ok(sse_event(initial)) }).chain(updates))
            .keep_alive(
                KeepAlive::new()
                    .interval(Duration::from_secs(15))
                    .text("heartbeat"),
            ),
    )
}
fn sse_event(envelope: EventEnvelope) -> Event {
    Event::default()
        .id(&envelope.id)
        .event(&envelope.event_type)
        .json_data(&envelope)
        .expect("serializable envelope")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn routes_reject_missing_and_wrong_run_scope() {
        let app = router(Engine::synthetic().unwrap(), None);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/snapshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/snapshot?run_id=someone-else")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn scenario_seek_returns_atomic_snapshot_without_future_observations() {
        let app = router(Engine::synthetic().unwrap(), None);
        for (at, expected_len, epoch) in [
            ("2026-11-14T12:10:00Z", 2, 1),
            ("2026-11-14T12:00:00Z", 1, 2),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/scenarios/portugal-synthetic/seek?run_id=synthetic-portugal")
                        .header("content-type", "application/json")
                        .body(Body::from(format!("{{\"at\":\"{at}\"}}")))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(result["evidence"].as_array().unwrap().len(), expected_len);
            assert_eq!(result["scenario"]["epoch"], epoch);
        }
    }

    #[tokio::test]
    async fn dispatch_is_simulated_and_seek_invalidates_all_derived_outputs() {
        let app = router(Engine::synthetic().unwrap(), None);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/incidents/incident-training-1/dispatch?run_id=synthetic-portugal")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(snapshot.forecasts.len(), 1);
        assert!(snapshot.forecasts[0].illustrative);
        assert!(snapshot.drone_plans[0].simulated);
        assert!(snapshot.briefings[0].simulated);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/scenarios/portugal-synthetic/reset?run_id=synthetic-portugal")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
        assert!(
            snapshot.forecasts.is_empty()
                && snapshot.drone_plans.is_empty()
                && snapshot.briefings.is_empty()
        );
    }

    #[tokio::test]
    async fn webhook_cannot_inject_live_data_into_synthetic_replay() {
        let app = router(Engine::synthetic().unwrap(), None);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhooks/context")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 503);
    }
}
