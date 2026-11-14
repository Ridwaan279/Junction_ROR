//! Physics fire-spread and smoke forecasts per incident.
//!
//! `POST /api/incidents/{id}/firecast?run_id=` gathers the fire environment
//! (terrain, land cover, weather field, Context.dev intelligence, IPMA warnings),
//! runs the ensemble on the bounded compute pool and stores a run/epoch-scoped
//! Forecast. With `auto` enabled, every newly visible incident with a position is
//! forecast once per epoch without a click.
use crate::{ApiError, ApiResult, AppState, RunQuery, find_incident, publish, scope};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use emberwatch_firecast::{
    AcquireRequest, Basis, ContextDev, FirecastConfig, FirecastResult, HttpSources, acquire,
    compute, to_domain,
};
use emberwatch_platform::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct FirecastOptions {
    /// Forecast new incidents automatically.
    pub auto: bool,
    /// Allow external data (weather, terrain, land cover, Context.dev, IPMA).
    pub network: bool,
    /// Force explicitly synthetic weather (e.g. offline demos).
    pub weather_synthetic: bool,
    /// Context.dev credit cap per incident forecast.
    pub max_context_credits: f64,
}

impl FirecastOptions {
    /// Manual, offline: used by tests and by `router()`.
    pub fn offline() -> Self {
        Self {
            auto: false,
            network: false,
            weather_synthetic: true,
            max_context_credits: 0.0,
        }
    }
    /// `FIRECAST_AUTO` (default on), `FIRECAST_NETWORK` (default on),
    /// `FIRECAST_WEATHER=synthetic`, `FIRECAST_CONTEXT_MAX_CREDITS` (default 25).
    pub fn from_env() -> Self {
        let flag = |k: &str, default: bool| {
            std::env::var(k).map_or(default, |v| !matches!(v.as_str(), "0" | "false" | "off"))
        };
        Self {
            auto: flag("FIRECAST_AUTO", true),
            network: flag("FIRECAST_NETWORK", true),
            weather_synthetic: std::env::var("FIRECAST_WEATHER").is_ok_and(|v| v == "synthetic"),
            max_context_credits: std::env::var("FIRECAST_CONTEXT_MAX_CREDITS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(25.0),
        }
    }
}

type Key = (String, u64);

#[derive(Clone)]
pub(crate) struct FirecastState {
    pub options: FirecastOptions,
    sources: Option<HttpSources>,
    context: Option<ContextDev>,
    cache: Arc<Mutex<HashMap<Key, Arc<FirecastResult>>>>,
    inflight: Arc<Mutex<HashSet<Key>>>,
    failures: Arc<Mutex<HashMap<Key, Instant>>>,
}

impl FirecastState {
    pub fn new(options: FirecastOptions) -> Self {
        let sources = options.network.then(|| HttpSources::new().ok()).flatten();
        let context = options.network.then(ContextDev::from_env).flatten();
        Self {
            options,
            sources,
            context,
            cache: Default::default(),
            inflight: Default::default(),
            failures: Default::default(),
        }
    }
    pub async fn cached(&self, incident: &str, epoch: u64) -> Option<Arc<FirecastResult>> {
        self.cache
            .lock()
            .await
            .get(&(incident.to_owned(), epoch))
            .cloned()
    }
}

pub(crate) async fn handler(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
    Path(id): Path<String>,
) -> ApiResult<Json<Snapshot>> {
    run(&state, &query.run_id, &id).await.map(Json)
}

fn place_name(incident: &Incident, evidence: &[Evidence]) -> String {
    evidence
        .iter()
        .filter(|e| incident.evidence_ids.contains(&e.id))
        .find_map(|e| e.municipality.clone())
        .or_else(|| incident.location_name.clone())
        .map(|n| n.split('(').next().unwrap_or("").trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| incident.title.clone())
}

pub(crate) async fn run(state: &AppState, run_id: &str, id: &str) -> ApiResult<Snapshot> {
    let fc = &state.firecast;
    let (context, incident, snapshot) = {
        let engine = state.engine.lock().await;
        scope(&engine, run_id)?;
        let snapshot = engine.snapshot();
        let incident = find_incident(&snapshot, id)?;
        let scenario = &snapshot.scenario;
        let context = RunContext {
            run_id: scenario.run_id.clone(),
            scenario_id: Some(scenario.id.clone()),
            mode: scenario.mode,
            epoch: scenario.epoch,
            bbox: PORTUGAL_BBOX,
            window_start: scenario.start_at,
            window_end: scenario.end_at,
            clock: Arc::new(FixedClock(scenario.current_at)),
        };
        (context, incident, snapshot)
    };
    let position = incident.position.ok_or_else(|| {
        ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "incident has no position to forecast from".into(),
        )
    })?;
    let key = (id.to_owned(), context.epoch);
    if !fc.inflight.lock().await.insert(key.clone()) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "a forecast for this incident is already running".into(),
        ));
    }
    let outcome = forecast(state, &context, &incident, &snapshot, position).await;
    fc.inflight.lock().await.remove(&key);
    match &outcome {
        Ok(_) => {
            fc.failures.lock().await.remove(&key);
        }
        Err(_) => {
            fc.failures.lock().await.insert(key, Instant::now());
        }
    }
    outcome
}

async fn forecast(
    state: &AppState,
    context: &RunContext,
    incident: &Incident,
    snapshot: &Snapshot,
    position: [f64; 2],
) -> ApiResult<Snapshot> {
    let fc = &state.firecast;
    let now = context.clock.now();
    let reported = incident
        .first_event_at
        .or_else(|| {
            snapshot
                .evidence
                .iter()
                .filter(|e| incident.evidence_ids.contains(&e.id))
                .map(|e| e.provenance.event_at.unwrap_or(e.provenance.available_at))
                .min()
        })
        .unwrap_or(now);
    let start = reported.clamp(now - chrono::Duration::hours(6), now);
    let basis = match (
        context.mode,
        fc.options.network && !fc.options.weather_synthetic,
    ) {
        (_, false) => Basis::Synthetic,
        (DataMode::Live, true) => Basis::Live,
        (DataMode::Historical, true) => Basis::Historical,
        (DataMode::Synthetic, true) => Basis::WhatIf,
    };
    let request = AcquireRequest {
        ignition: position,
        uncertainty_m: incident.uncertainty_m.unwrap_or(500.0),
        place: place_name(incident, &snapshot.evidence),
        start,
        now,
        basis,
        max_context_credits: fc.options.max_context_credits,
        network: fc.options.network,
    };
    let env = match tokio::time::timeout(
        Duration::from_secs(90),
        acquire(fc.sources.as_ref(), fc.context.as_ref(), request.clone()),
    )
    .await
    {
        Ok(env) => env,
        Err(_) => {
            tracing::warn!("firecast environment timed out; using synthetic inputs");
            acquire(
                None,
                None,
                AcquireRequest {
                    basis: Basis::Synthetic,
                    network: false,
                    ..request
                },
            )
            .await
        }
    };
    let config = FirecastConfig {
        seed: snapshot.scenario.seed.max(1),
        ..FirecastConfig::default()
    };
    let result = state
        .compute
        .run(move || {
            compute(&env, &config)
                .map_err(|e| emberwatch_response::ComputeError::InvalidInput(e.to_string()))
        })
        .await
        .map_err(|e| {
            ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                format!("forecast compute: {e}"),
            )
        })?;
    let forecast = to_domain(
        context,
        incident,
        &snapshot.evidence,
        &snapshot.weather,
        &result,
    )
    .map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;
    let mut engine = state.engine.lock().await;
    if engine.scenario.run_id != context.run_id || engine.scenario.epoch != context.epoch {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "scenario changed during the forecast; retry".into(),
        ));
    }
    let mut candidate = engine.clone();
    candidate.forecasts.retain(|f| f.incident_id != incident.id);
    candidate.forecasts.push(forecast);
    if let Some(store) = &state.store {
        store.save(&candidate).await.map_err(|_| {
            ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "persistence unavailable".into(),
            )
        })?;
    }
    *engine = candidate;
    fc.cache
        .lock()
        .await
        .insert((incident.id.clone(), context.epoch), Arc::new(result));
    publish(state, &engine, "forecast.ready");
    Ok(engine.snapshot())
}

/// Start forecasts for visible incidents that have none in the current epoch.
pub(crate) async fn auto_trigger(state: &AppState) {
    if !state.firecast.options.auto {
        return;
    }
    let (run_id, epoch, pending) = {
        let engine = state.engine.lock().await;
        let snapshot = engine.snapshot();
        let pending: Vec<String> = snapshot
            .incidents
            .iter()
            .filter(|i| {
                i.position.is_some()
                    && !snapshot
                        .forecasts
                        .iter()
                        .any(|f| f.incident_id == i.id && f.model_version.starts_with("firecast"))
            })
            .map(|i| i.id.clone())
            .collect();
        (snapshot.scenario.run_id, snapshot.scenario.epoch, pending)
    };
    for id in pending {
        let key = (id.clone(), epoch);
        if state.firecast.inflight.lock().await.contains(&key)
            || state
                .firecast
                .failures
                .lock()
                .await
                .get(&key)
                .is_some_and(|t| t.elapsed() < Duration::from_secs(120))
        {
            continue;
        }
        let state = state.clone();
        let run_id = run_id.clone();
        tokio::spawn(async move {
            if let Err(ApiError(status, message)) = run(&state, &run_id, &id).await {
                tracing::warn!(%status, %message, incident = %id, "automatic forecast failed");
            }
        });
    }
}
