use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use chrono::Duration;
use emberwatch_domain::*;
use emberwatch_platform::{Platform, Store};
use emberwatch_server::{DEMO_DATASET, OperatorAccess, router};
use futures_util::StreamExt;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn app() -> (Router, Platform, Scenario) {
    let platform = Platform::open(Store::memory(), vec![]).await.unwrap();
    let dataset: ScenarioDataset = serde_json::from_str(DEMO_DATASET).unwrap();
    let run = platform.load_dataset(dataset).await.unwrap();
    (
        router(platform.clone(), OperatorAccess::default()),
        platform,
        run,
    )
}
async fn json_response(
    app: Router,
    method: &str,
    path: &str,
    input: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(path);
    let body = if let Some(value) = input {
        request = request.header("content-type", "application/json");
        Body::from(value.to_string())
    } else {
        Body::empty()
    };
    let response = app.oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn api_scopes_incidents_and_hides_future_evidence() {
    let (app, _, run) = app().await;
    let (status, value) =
        json_response(app.clone(), "GET", &format!("/api/runs/{}", run.id), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["evidence"].as_array().unwrap().len(), 1);
    assert_eq!(value["weather"].as_array().unwrap().len(), 1);
    let id = run.incidents[0].id;
    let (status, _) =
        json_response(app.clone(), "GET", &format!("/api/incidents/{id}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND); // defaults to live, not the scenario
    let (status, _) = json_response(
        app.clone(),
        "GET",
        &format!("/api/incidents/{id}?run_id={}", run.id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = json_response(app, "GET", "/api/incidents?bbox=1,2,3", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn paid_mutations_require_operator_but_replay_controls_work() {
    let (app, _, run) = app().await;
    let (status, _) = json_response(
        app.clone(),
        "POST",
        "/api/scenarios",
        Some(serde_json::to_value(&run.spec).unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = json_response(
        app.clone(),
        "PATCH",
        "/api/sources/fogos-context",
        Some(json!({"enabled":true})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, seek) = json_response(
        app.clone(),
        "POST",
        &format!("/api/scenarios/{}/seek", run.id),
        Some(json!({"at":run.cursor + Duration::minutes(20)})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(seek["epoch"], 1);
    assert_eq!(seek["evidence"].as_array().unwrap().len(), 3);
    let (status, _) = json_response(
        app,
        "POST",
        &format!("/api/scenarios/{}/seek", live_run_id()),
        Some(json!({"at":run.cursor})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn authorized_operator_can_create_and_load_a_scenario() {
    let (_, platform, run) = app().await;
    let app = router(
        platform,
        OperatorAccess {
            token: Some("test-operator".into()),
            allow_local_mutations: false,
        },
    );
    let request = Request::builder()
        .method("POST")
        .uri("/api/scenarios")
        .header("authorization", "Bearer test-operator")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&run.spec).unwrap()))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn sse_delivers_scoped_reset_envelope_without_buffering_forever() {
    let (app, _, run) = app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/events?run_id={}", run.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let mut stream = response.into_body().into_data_stream();
    let bytes = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(text.contains("event: run.reset"));
    assert!(text.contains(&run.id.to_string()));
    assert!(text.contains("snapshot_url"));
}

#[tokio::test]
async fn unimplemented_response_capabilities_are_explicit() {
    let (app, _, _) = app().await;
    let (status, value) = json_response(app, "GET", "/api/drones", None).await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(value["error"]["code"], "agent_c_pending");
}
