//! Historical planning is explicitly run/cursor scoped. Live requests can never
//! fall back to a historical frame. No paid calls or model jobs in these handlers.
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    path::{Path as FilePath, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
struct PlanningState {
    root: PathBuf,
    stations: Value,
}
pub fn router(root: PathBuf) -> Router {
    routes(root, true)
}
pub fn stations_value() -> Value {
    serde_json::from_str(include_str!("../../../data/stations/portugal.json"))
        .expect("checked station configuration")
}
fn routes(root: PathBuf, include_stations: bool) -> Router {
    let stations = stations_value();
    let mut router = Router::new()
        .route("/api/incidents/{id}/fleet", post(fleet))
        .route("/api/incidents/{id}/exposures", get(exposures));
    if include_stations {
        router = router.route("/api/stations", get(stations_handler));
    }
    router.with_state(Arc::new(PlanningState { root, stations }))
}
pub fn default_router() -> Router {
    let root = std::env::var_os("EMBERWATCH_GENERATED_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            FilePath::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../web/public/generated/arganil-early")
        });
    routes(root, false)
}
async fn stations_handler(State(state): State<Arc<PlanningState>>) -> Json<Value> {
    Json(state.stations.clone())
}
#[derive(Deserialize)]
struct Cursor {
    run_id: String,
    at: DateTime<Utc>,
}
type ResultJson = Result<Json<Value>, (StatusCode, Json<Value>)>;
fn error(status: StatusCode, message: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"error":message})))
}
async fn frame(
    state: &PlanningState,
    id: &str,
    cursor: &Cursor,
) -> Result<Value, (StatusCode, Json<Value>)> {
    if id != "arganil-early" || cursor.run_id != "historical:arganil-early" {
        return Err(error(
            StatusCode::NOT_FOUND,
            "Historical planning requires incident arganil-early and run_id historical:arganil-early. Live planning is unavailable; no replay fallback.",
        ));
    }
    let index: Value = serde_json::from_slice(
        &tokio::fs::read(state.root.join("index.json"))
            .await
            .map_err(|_| {
                error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Generated historical replay is not installed",
                )
            })?,
    )
    .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "Invalid replay index"))?;
    if index["mode"] != "historical" {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Replay mode mismatch",
        ));
    }
    let parse = |v: &Value| v.as_str().and_then(|s| s.parse::<DateTime<Utc>>().ok());
    let (from, to) = (
        parse(&index["window"]["from"]),
        parse(&index["window"]["to"]),
    );
    if from.is_none_or(|t| cursor.at < t) || to.is_none_or(|t| cursor.at > t) {
        return Err(error(StatusCode::NOT_FOUND, "Cursor outside replay window"));
    }
    let selected = index["frames"]
        .as_array()
        .and_then(|frames| {
            frames
                .iter()
                .filter(|f| parse(&f["t"]).is_some_and(|t| t <= cursor.at))
                .max_by_key(|f| parse(&f["t"]))
        })
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "No frame at or before cursor"))?;
    let file = selected["file"]
        .as_str()
        .ok_or_else(|| error(StatusCode::SERVICE_UNAVAILABLE, "Invalid frame reference"))?;
    let relative = FilePath::new(file);
    if !file.starts_with("frames/")
        || relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Unsafe frame reference",
        ));
    }
    let bytes = tokio::fs::read(state.root.join(relative))
        .await
        .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "Frame unavailable"))?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(error(StatusCode::SERVICE_UNAVAILABLE, "Oversized frame"));
    }
    let frame: Value = serde_json::from_slice(&bytes)
        .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "Invalid frame JSON"))?;
    if frame["mode"] != "historical" || parse(&frame["t"]).is_none_or(|t| t > cursor.at) {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Frame is from a different mode or future cursor",
        ));
    }
    Ok(frame)
}
async fn fleet(
    State(state): State<Arc<PlanningState>>,
    Path(id): Path<String>,
    Query(cursor): Query<Cursor>,
) -> ResultJson {
    let frame = frame(&state, &id, &cursor).await?;
    if !frame["fleet"].is_object() {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Fleet output not generated",
        ));
    }
    Ok(Json(frame["fleet"].clone()))
}
async fn exposures(
    State(state): State<Arc<PlanningState>>,
    Path(id): Path<String>,
    Query(cursor): Query<Cursor>,
) -> ResultJson {
    let frame = frame(&state, &id, &cursor).await?;
    Ok(Json(frame["exposures"].clone()))
}
