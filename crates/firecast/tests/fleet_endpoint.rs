//! Compile and exercise the actual server module without the SQLx macro DLL.
#[path = "../../server/src/fleet_api.rs"]
mod fleet_api;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;
#[tokio::test]
async fn live_cannot_fall_back_to_history_and_missing_generation_is_explicit() {
    let _default_configuration = fleet_api::default_router();
    let missing = std::env::temp_dir().join("emberwatch-no-such-generated-replay-test");
    let app = fleet_api::router(missing);
    for (uri, status) in [
        (
            "/api/incidents/arganil-early/fleet?run_id=live&at=2025-08-13T12:00:00Z",
            StatusCode::NOT_FOUND,
        ),
        (
            "/api/incidents/arganil-early/fleet?run_id=historical:arganil-early&at=2025-08-13T12:00:00Z",
            StatusCode::SERVICE_UNAVAILABLE,
        ),
    ] {
        let result = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(result.status(), status);
        assert!(!to_bytes(result.into_body(), 4096).await.unwrap().is_empty());
    }
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/stations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let stations: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 100_000).await.unwrap()).unwrap();
    assert!(stations.as_array().unwrap().iter().all(|s| {
        s["simulated"] == true
            && s["coordinate_source"]
                .as_str()
                .unwrap()
                .contains("openstreetmap.org/node/")
    }));
}
