//! Bounded, ephemeral, simulation-only reconnaissance. Uses the existing SSE envelope.
use super::*;
use axum::{body::Bytes, extract::DefaultBodyLimit, http::header};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub(crate) struct ReconStore(Arc<Mutex<BTreeMap<String, Task>>>);

#[derive(Clone, Serialize)]
struct Task {
    id: String,
    run_id: String,
    epoch: u64,
    incident: Value,
    reports: Vec<Value>,
    drone_id: String,
    simulation: bool,
    assigned_at: DateTime<Utc>,
    event_stale: bool,
    requested_action: String,
    observation: Option<Observation>,
    frame: Option<Frame>,
    #[serde(skip)]
    jpeg: Option<Bytes>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Observation {
    sequence: u64,
    simulation: bool,
    state: String,
    observed_at: DateTime<Utc>,
    sim_time_s: Option<f64>,
    drone_position_enu_m: Option<[f64; 3]>,
    drone_position_lon_lat: Option<[f64; 2]>,
    scene_is_geographic: bool,
    camera_status: String,
    camera_topic: Option<String>,
    detection_method: String,
    note: String,
}

#[derive(Clone, Serialize)]
struct Frame {
    sequence: u64,
    captured_at: DateTime<Utc>,
    sim_time_s: Option<f64>,
    drone_position_enu_m: Option<[f64; 3]>,
    image_url: String,
    origin: &'static str,
}

#[derive(Deserialize)]
struct Assign {
    incident_id: String,
    drone_id: String,
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/recon/tasks", get(list).post(assign))
        .route("/api/recon/tasks/{id}/return", post(recall))
        .route("/api/recon/tasks/{id}/observations", post(observe))
        .route("/api/recon/tasks/{id}/frame", get(frame).post(upload))
        .layer(DefaultBodyLimit::max(512 * 1024))
}

fn invalid(message: &str) -> ApiError {
    ApiError(StatusCode::UNPROCESSABLE_ENTITY, message.into())
}

fn view(task: &Task) -> Value {
    let mut value = json!(task);
    value["telemetry_stale"] = json!(
        task.observation
            .as_ref()
            .is_none_or(|o| (Utc::now() - o.observed_at).num_seconds() > 10)
    );
    value["frame_stale"] = json!(
        task.frame
            .as_ref()
            .is_none_or(|f| (Utc::now() - f.captured_at).num_seconds() > 10)
    );
    value
}

async fn list(State(state): State<AppState>, Query(q): Query<RunQuery>) -> ApiResult<Json<Value>> {
    let engine = state.engine.lock().await;
    scope(&engine, &q.run_id)?;
    let tasks = state.recon.0.lock().await;
    Ok(Json(
        json!({"simulation": true, "tasks": tasks.values().filter(|t| t.run_id == q.run_id && t.epoch == engine.scenario.epoch).map(view).collect::<Vec<_>>()}),
    ))
}

async fn assign(
    State(state): State<AppState>,
    Query(q): Query<RunQuery>,
    Json(input): Json<Assign>,
) -> ApiResult<Json<Value>> {
    let engine = state.engine.lock().await;
    scope(&engine, &q.run_id)?;
    if engine.scenario.mode != DataMode::Synthetic {
        return Err(invalid("recon demo accepts synthetic events only"));
    }
    if input.drone_id.is_empty()
        || input.drone_id.len() > 64
        || !input
            .drone_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
    {
        return Err(invalid("invalid simulated drone ID"));
    }
    let snapshot = engine.snapshot();
    let incident = find_incident(&snapshot, &input.incident_id)?;
    if incident.position.is_none() {
        return Err(invalid("target coordinates unknown"));
    }
    let reports: Vec<_> = snapshot
        .evidence
        .iter()
        .filter(|e| incident.evidence_ids.contains(&e.id))
        .map(|e| json!(e.provenance))
        .collect();
    if reports.is_empty() {
        return Err(invalid("target has no cited evidence"));
    }
    let mut tasks = state.recon.0.lock().await;
    tasks.retain(|_, t| t.epoch == engine.scenario.epoch && t.run_id == q.run_id);
    if let Some(t) = tasks.values().find(|t| {
        t.drone_id == input.drone_id
            && t.observation
                .as_ref()
                .is_none_or(|o| !matches!(o.state.as_str(), "complete" | "failed"))
    }) {
        if t.incident["id"] != input.incident_id {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "drone already assigned to a different incident".into(),
            ));
        }
        return Ok(Json(view(t)));
    }
    if tasks.len() >= 16 {
        return Err(invalid("demo task capacity reached (16); reset scenario"));
    }
    let task = Task {
        id: uuid::Uuid::new_v4().to_string(),
        run_id: q.run_id,
        epoch: engine.scenario.epoch,
        incident: json!(incident),
        reports,
        drone_id: input.drone_id,
        simulation: true,
        assigned_at: Utc::now(),
        event_stale: (engine.scenario.current_at - incident.updated_at).num_seconds() > 900,
        requested_action: "observe".into(),
        observation: None,
        frame: None,
        jpeg: None,
    };
    let value = view(&task);
    tasks.insert(task.id.clone(), task);
    emit(&state, &engine, "recon.assigned", value.clone());
    Ok(Json(value))
}

fn task_mut<'a>(
    tasks: &'a mut BTreeMap<String, Task>,
    id: &str,
    engine: &Engine,
    run: &str,
) -> ApiResult<&'a mut Task> {
    scope(engine, run)?;
    tasks
        .get_mut(id)
        .filter(|t| t.run_id == run && t.epoch == engine.scenario.epoch)
        .ok_or_else(|| {
            ApiError(
                StatusCode::NOT_FOUND,
                "task not in current run/epoch".into(),
            )
        })
}

fn emit(state: &AppState, engine: &Engine, kind: &str, payload: Value) {
    let seq = state.sequence.fetch_add(1, Ordering::Relaxed);
    let _ = state.events.send(engine.envelope(seq, kind, payload));
}

async fn recall(
    State(state): State<AppState>,
    Query(q): Query<RunQuery>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let engine = state.engine.lock().await;
    let mut tasks = state.recon.0.lock().await;
    let task = task_mut(&mut tasks, &id, &engine, &q.run_id)?;
    task.requested_action = "return".into();
    let value = view(task);
    emit(&state, &engine, "recon.return_requested", value.clone());
    Ok(Json(value))
}

fn transition(old: &str, new: &str) -> bool {
    old == new
        || new == "failed" && !matches!(old, "complete" | "failed")
        || matches!(
            (old, new),
            ("assigned", "travelling" | "returning")
                | ("travelling", "observing" | "returning")
                | ("observing", "returning")
                | ("returning", "complete")
        )
}

async fn observe(
    State(state): State<AppState>,
    Query(q): Query<RunQuery>,
    Path(id): Path<String>,
    Json(input): Json<Observation>,
) -> ApiResult<Json<Value>> {
    let age = Utc::now() - input.observed_at;
    if !input.simulation
        || age.num_seconds() < -5
        || age.num_seconds() > 30
        || input.note.len() > 2000
        || input.detection_method != "simulator_state"
        || input.scene_is_geographic
        || input.drone_position_lon_lat.is_some()
        || !matches!(
            input.camera_status.as_str(),
            "available" | "no_camera" | "stale"
        )
        || input
            .drone_position_enu_m
            .is_some_and(|p| p.iter().any(|v| !v.is_finite()))
        || input.sim_time_s.is_some_and(|t| !t.is_finite() || t < 0.)
    {
        return Err(invalid(
            "invalid, stale or unsupported observation; this representative scene has no geographic drone position",
        ));
    }
    let engine = state.engine.lock().await;
    let mut tasks = state.recon.0.lock().await;
    let task = task_mut(&mut tasks, &id, &engine, &q.run_id)?;
    let previous = task.observation.as_ref();
    if previous.is_some_and(|o| input.sequence <= o.sequence)
        || !transition(
            previous.map_or("assigned", |o| o.state.as_str()),
            &input.state,
        )
    {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "out of order sequence or invalid mission transition".into(),
        ));
    }
    task.observation = Some(input);
    let value = view(task);
    emit(&state, &engine, "recon.observation", value.clone());
    Ok(Json(value))
}

#[derive(Deserialize)]
struct FrameQuery {
    run_id: String,
    sequence: u64,
}

async fn upload(
    State(state): State<AppState>,
    Query(q): Query<FrameQuery>,
    Path(id): Path<String>,
    bytes: Bytes,
) -> ApiResult<Json<Value>> {
    if bytes.len() < 4 || !bytes.starts_with(&[255, 216]) || !bytes.ends_with(&[255, 217]) {
        return Err(invalid("expected bounded JPEG bytes"));
    }
    let engine = state.engine.lock().await;
    let mut tasks = state.recon.0.lock().await;
    let task = task_mut(&mut tasks, &id, &engine, &q.run_id)?;
    let obs = task
        .observation
        .as_ref()
        .ok_or_else(|| invalid("observation required before frame"))?;
    if obs.sequence != q.sequence
        || obs.camera_status != "available"
        || (Utc::now() - obs.observed_at).num_seconds() > 10
    {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "frame must match fresh camera observation sequence".into(),
        ));
    }
    task.frame = Some(Frame {
        sequence: q.sequence,
        captured_at: obs.observed_at,
        sim_time_s: obs.sim_time_s,
        drone_position_enu_m: obs.drone_position_enu_m,
        image_url: format!(
            "/api/recon/tasks/{id}/frame?run_id={}&sequence={}",
            task.run_id, q.sequence
        ),
        origin: "gazebo_camera_render",
    });
    task.jpeg = Some(bytes);
    let value = view(task);
    emit(&state, &engine, "recon.frame", value.clone());
    Ok(Json(value))
}

async fn frame(
    State(state): State<AppState>,
    Query(q): Query<FrameQuery>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let engine = state.engine.lock().await;
    let mut tasks = state.recon.0.lock().await;
    let task = task_mut(&mut tasks, &id, &engine, &q.run_id)?;
    if task.frame.as_ref().is_none_or(|f| f.sequence != q.sequence) {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "frame replaced or unavailable; refetch task".into(),
        ));
    }
    let bytes = task
        .jpeg
        .clone()
        .ok_or_else(|| invalid("no camera frame"))?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    ))
}
