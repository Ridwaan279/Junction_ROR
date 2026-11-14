use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use emberwatch_platform::Engine;
use emberwatch_server::{firecast_api::FirecastOptions, router_with};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn call(app: &Router, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(path);
    let payload = match body {
        Some(v) => {
            request = request.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(request.body(payload).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 32 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn forecast_endpoint_publishes_physics_layers_and_drives_dispatch() {
    let engine = Engine::synthetic().unwrap();
    let run_id = engine.scenario.run_id.clone();
    let scenario_id = engine.scenario.id.clone();
    let app = router_with(engine, None, FirecastOptions::offline());
    let (_, snapshot) = call(&app, "GET", &format!("/api/snapshot?run_id={run_id}"), None).await;
    let incident = snapshot["incidents"][0]["id"].as_str().unwrap().to_owned();

    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/incidents/{incident}/firecast?run_id=wrong"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, snapshot) = call(
        &app,
        "POST",
        &format!("/api/incidents/{incident}/firecast?run_id={run_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{snapshot}");
    let forecast = &snapshot["forecasts"][0];
    assert!(
        forecast["model_version"]
            .as_str()
            .unwrap()
            .starts_with("firecast")
    );
    assert_eq!(forecast["geometry"]["firecast"]["basis"], "synthetic");
    let layers: Vec<&str> = forecast["geometry"]["features"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["properties"]["layer"].as_str())
        .collect();
    for layer in ["spread", "smoke", "wind", "head_vector"] {
        assert!(layers.contains(&layer), "missing {layer}");
    }

    // Dispatch now uses the forecast hazards rather than the toy demo bundle.
    let (status, snapshot) = call(
        &app,
        "POST",
        &format!("/api/incidents/{incident}/dispatch?run_id={run_id}"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{snapshot}");
    assert!(
        snapshot["forecasts"][0]["model_version"]
            .as_str()
            .unwrap()
            .starts_with("firecast")
    );
    let reasons = snapshot["drone_plans"][0]["reasons"].as_array().unwrap();
    assert!(
        reasons
            .iter()
            .any(|r| r.as_str().unwrap().contains("flank")),
        "{reasons:?}"
    );

    // Seek starts a new epoch: derived outputs and the cache no longer apply.
    let at = snapshot["scenario"]["start_at"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, snapshot) = call(
        &app,
        "POST",
        &format!("/api/scenarios/{scenario_id}/seek?run_id={run_id}"),
        Some(json!({"at": at})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{snapshot}");
    assert!(snapshot["forecasts"].as_array().unwrap().is_empty());
}
