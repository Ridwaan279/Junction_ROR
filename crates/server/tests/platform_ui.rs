use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
};
use emberwatch_domain::{Snapshot, platform::ScenarioDataset};
use emberwatch_platform::{Platform, Store};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn app() -> (Router, String, Platform) {
    let platform = Platform::open(Store::memory(), vec![]).await.unwrap();
    let dataset: ScenarioDataset =
        serde_json::from_str(emberwatch_server::platform_api::DEMO_DATASET).unwrap();
    let run = platform.load_dataset(dataset).await.unwrap();
    (
        emberwatch_server::platform_ui::router(platform.clone()),
        run.id.to_string(),
        platform,
    )
}
async fn call(app: Router, path: &str, input: Option<Value>) -> (u16, Value) {
    let request = Request::builder()
        .method(if input.is_some() { "POST" } else { "GET" })
        .uri(path)
        .header("content-type", "application/json")
        .body(input.map_or_else(Body::empty, |v| Body::from(v.to_string())))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let body = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn browser_snapshot_keeps_scope_provenance_and_converts_wind_units() {
    let (app, run, _) = app().await;
    let (status, value) = call(app.clone(), &format!("/api/snapshot?run_id={run}"), None).await;
    assert_eq!(status, 200);
    let snapshot: Snapshot = serde_json::from_value(value).unwrap();
    assert_eq!(snapshot.evidence.len(), 1);
    assert_eq!(snapshot.incidents.len(), 1);
    assert_eq!(snapshot.weather.len(), 1);
    assert_eq!(snapshot.weather[0].wind_speed_mps, Some(30.0 / 3.6));
    assert_eq!(snapshot.weather[0].gust_mps, None);
    assert_eq!(
        snapshot.evidence[0].provenance.origin_group,
        "synthetic-anepc"
    );
    assert_eq!(
        snapshot.evidence[0].provenance.source_url,
        "https://example.invalid/synthetic/report-1"
    );
    assert_eq!(snapshot.scenario.run_id, run);
    assert_eq!(call(app.clone(), "/api/snapshot", None).await.0, 400);
    assert_eq!(
        call(
            app,
            "/api/snapshot?run_id=00000000-0000-0000-0000-000000000001",
            None
        )
        .await
        .0,
        404
    );
}

#[tokio::test]
async fn platform_replay_dispatch_and_reset_work_through_browser_contract() {
    let (app, run, _) = app().await;
    let (_, value) = call(app.clone(), &format!("/api/snapshot?run_id={run}"), None).await;
    let snapshot: Snapshot = serde_json::from_value(value).unwrap();
    let (status, value) = call(
        app.clone(),
        &format!(
            "/api/incidents/{}/dispatch?run_id={run}",
            snapshot.incidents[0].id
        ),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, 200);
    let generated: Snapshot = serde_json::from_value(value).unwrap();
    assert!(generated.forecasts[0].illustrative);
    assert!(generated.drone_plans[0].simulated);
    assert_eq!(
        generated.forecasts[0].input_evidence_ids,
        vec![snapshot.evidence[0].id.clone()]
    );
    let (status, value) = call(
        app.clone(),
        &format!("/api/scenarios/{run}/seek?run_id={run}"),
        Some(json!({"at":"2026-01-01T12:25:00Z"})),
    )
    .await;
    assert_eq!(status, 200);
    let later: Snapshot = serde_json::from_value(value).unwrap();
    assert_eq!(later.evidence.len(), 3);
    assert_eq!(later.incidents.len(), 1);
    assert_eq!(later.incidents[0].evidence_level, "satellite_supported");
    assert!(later.forecasts.is_empty());
    let (_, value) = call(
        app,
        &format!("/api/scenarios/{run}/reset?run_id={run}"),
        Some(json!({})),
    )
    .await;
    let reset: Snapshot = serde_json::from_value(value).unwrap();
    assert_eq!(reset.evidence.len(), 1);
    assert!(reset.drone_plans.is_empty());
    assert_eq!(reset.scenario.epoch, 2);
}

#[tokio::test]
async fn dispatch_preserves_outputs_for_other_incidents_in_the_run() {
    let platform = Platform::open(Store::memory(), vec![]).await.unwrap();
    let mut dataset: ScenarioDataset =
        serde_json::from_str(emberwatch_server::platform_api::DEMO_DATASET).unwrap();
    let mut other = dataset.evidence[0].clone();
    other.external_id = "other-fire".into();
    other.point = Some(emberwatch_domain::platform::Point {
        longitude: -8.8,
        latitude: 39.7,
    });
    other.place_name = "Other synthetic location".into();
    dataset.evidence.push(other);
    let run = platform.load_dataset(dataset).await.unwrap();
    assert_eq!(run.incidents.len(), 2);
    let app = emberwatch_server::platform_ui::router(platform);
    for incident in &run.incidents {
        assert_eq!(
            call(
                app.clone(),
                &format!("/api/incidents/{}/dispatch?run_id={}", incident.id, run.id),
                Some(json!({}))
            )
            .await
            .0,
            200
        );
    }
    let (_, value) = call(app, &format!("/api/snapshot?run_id={}", run.id), None).await;
    let snapshot: Snapshot = serde_json::from_value(value).unwrap();
    assert_eq!(snapshot.forecasts.len(), 2);
    assert_eq!(snapshot.drone_plans.len(), 2);
    assert_eq!(snapshot.briefings.len(), 2);
    for f in &snapshot.forecasts {
        let incident = snapshot
            .incidents
            .iter()
            .find(|i| i.id == f.incident_id)
            .unwrap();
        assert_eq!(f.input_evidence_ids, incident.evidence_ids);
    }
}

#[tokio::test]
async fn mounted_native_sse_recovery_url_resolves_to_its_run() {
    use futures::StreamExt;
    let (_, run, platform) = app().await;
    let app = Router::new().nest(
        "/platform",
        emberwatch_server::platform_api::integrated_router(platform, Default::default()),
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/platform/api/events?run_id={run}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut body = response.into_body().into_data_stream();
    let bytes = tokio::time::timeout(std::time::Duration::from_secs(1), body.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    let data = text.lines().find_map(|l| l.strip_prefix("data: ")).unwrap();
    let event: Value = serde_json::from_str(data).unwrap();
    let url = event["payload"]["snapshot_url"].as_str().unwrap();
    assert_eq!(call(app, url, None).await.0, 200);
}
