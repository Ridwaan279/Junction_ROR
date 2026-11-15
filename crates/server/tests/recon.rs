use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use chrono::Utc;
use emberwatch_platform::Engine;
use futures::StreamExt;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn call(app: &Router, method: &str, path: &str, data: Option<Value>) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(data.map_or_else(Body::empty, |d| Body::from(d.to_string())))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}
const Q: &str = "?run_id=synthetic-portugal";

#[tokio::test]
async fn recon_publishes_existing_scoped_event_envelope() {
    let app = emberwatch_server::router(Engine::synthetic().unwrap(), None);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/events{Q}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut stream = response.into_body().into_data_stream();
    let initial = stream.next().await.unwrap().unwrap();
    assert!(
        String::from_utf8(initial.to_vec())
            .unwrap()
            .contains("event: run.reset")
    );
    call(
        &app,
        "POST",
        &format!("/api/recon/tasks{Q}"),
        Some(json!({"incident_id":"incident-training-1", "drone_id":"ember_drone"})),
    )
    .await;
    let chunk = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let text = String::from_utf8(chunk.to_vec()).unwrap();
    assert!(text.contains("event: recon.assigned"));
    let data: Value =
        serde_json::from_str(text.lines().find_map(|l| l.strip_prefix("data: ")).unwrap()).unwrap();
    for key in [
        "id",
        "type",
        "run_id",
        "scenario_id",
        "epoch",
        "event_time",
        "emitted_at",
        "payload",
    ] {
        assert!(data.get(key).is_some(), "{key}");
    }
    assert_eq!(data["run_id"], "synthetic-portugal");
    assert_eq!(data["payload"]["simulation"], true);
}

fn obs(seq: u64, state: &str) -> Value {
    json!({"sequence":seq,"simulation":true,"state":state,"observed_at":Utc::now(),"sim_time_s":2.5,
      "drone_position_enu_m":[-12, -8, 12], "drone_position_lon_lat":null,"scene_is_geographic":false,
      "camera_status":"no_camera","camera_topic":null,"detection_method":"simulator_state","note":"SIMULATED test, no camera"})
}

#[tokio::test]
async fn recon_provenance_transitions_and_epoch_are_enforced() {
    let app = emberwatch_server::router(Engine::synthetic().unwrap(), None);
    let (status, task) = call(
        &app,
        "POST",
        &format!("/api/recon/tasks{Q}"),
        Some(json!({"incident_id":"incident-training-1","drone_id":"ember_drone"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(task["simulation"], true);
    assert!(
        task["reports"][0]["source_url"]
            .as_str()
            .unwrap()
            .contains("synthetic")
    );
    assert!(task["incident"]["first_event_at"].is_null());
    assert!(task["frame"].is_null());
    let path = format!(
        "/api/recon/tasks/{}/observations{Q}",
        task["id"].as_str().unwrap()
    );
    assert_eq!(
        call(&app, "POST", &path, Some(obs(1, "observing"))).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(&app, "POST", &path, Some(obs(1, "travelling")))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "POST", &path, Some(obs(1, "travelling")))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let mut fake = obs(2, "observing");
    fake["detection_method"] = json!("image_detector");
    assert_eq!(
        call(&app, "POST", &path, Some(fake)).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut stale = obs(2, "observing");
    stale["observed_at"] = json!(Utc::now() - chrono::Duration::seconds(60));
    assert_eq!(
        call(&app, "POST", &path, Some(stale)).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        call(&app, "POST", &path, Some(obs(2, "observing"))).await.0,
        StatusCode::OK
    );
    for (seq, state) in [(3, "returning"), (4, "complete")] {
        assert_eq!(
            call(&app, "POST", &path, Some(obs(seq, state))).await.0,
            StatusCode::OK
        );
    }
    assert_eq!(
        call(&app, "POST", &path, Some(obs(5, "travelling")))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/scenarios/portugal-synthetic/reset{Q}"),
            Some(json!({}))
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "GET", &format!("/api/recon/tasks{Q}"), None)
            .await
            .1["tasks"],
        json!([])
    );
    assert_eq!(
        call(&app, "POST", &path, Some(obs(6, "complete"))).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn frames_require_matching_camera_observation_and_current_run() {
    let app = emberwatch_server::router(Engine::synthetic().unwrap(), None);
    let task = call(
        &app,
        "POST",
        &format!("/api/recon/tasks{Q}"),
        Some(json!({"incident_id":"incident-training-1","drone_id":"ember_drone"})),
    )
    .await
    .1;
    let base = format!("/api/recon/tasks/{}", task["id"].as_str().unwrap());
    let mut observation = obs(1, "travelling");
    observation["camera_status"] = json!("available");
    call(
        &app,
        "POST",
        &format!("{base}/observations{Q}"),
        Some(observation),
    )
    .await;
    // Transport validation only; the end-to-end check decodes an actual Gazebo JPEG.
    let req = Request::builder()
        .method("POST")
        .uri(format!("{base}/frame{Q}&sequence=1"))
        .body(Body::from(vec![255, 216, 255, 217]))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(req).await.unwrap().status(),
        StatusCode::OK
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{base}/frame{Q}&sequence=1"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let req = Request::builder()
        .uri(format!("{base}/frame?run_id=another-run&sequence=1"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(req).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
    let req = Request::builder()
        .method("POST")
        .uri(format!("{base}/frame{Q}&sequence=2"))
        .body(Body::from(vec![255, 216, 255, 217]))
        .unwrap();
    assert_eq!(
        app.oneshot(req).await.unwrap().status(),
        StatusCode::CONFLICT
    );
}
