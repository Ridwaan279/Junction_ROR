//! On-demand public-page checks; single-flight, cached and bounded. No replay data here.
use axum::{Json, Router, extract::State, routing::get};
use chrono::Utc;
use emberwatch_intelligence::{context::ContextClient, live::LiveStatus};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct LiveOptions {
    pub enabled: bool,
    pub source_checks_passed: bool,
    pub cache_seconds: u64,
    pub hourly_credits: f64,
}
impl LiveOptions {
    pub fn offline() -> Self {
        Self {
            enabled: false,
            source_checks_passed: false,
            cache_seconds: 300,
            hourly_credits: 10.0,
        }
    }
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("EMBERWATCH_LIVE_ENABLED").as_deref() == Ok("1"),
            source_checks_passed: std::env::var("FOGOS_PUBLIC_PAGE_CHECKS_PASSED").as_deref()
                == Ok("1"),
            cache_seconds: std::env::var("LIVE_STATUS_CACHE_SECONDS")
                .ok()
                .and_then(|s| s.parse().ok())
                .filter(|n: &u64| (30..=3600).contains(n))
                .unwrap_or(300),
            hourly_credits: std::env::var("CONTEXT_LIVE_HOURLY_CREDITS")
                .ok()
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|n| n.is_finite() && *n >= 0.)
                .unwrap_or(10.0),
        }
    }
}
struct Cache {
    options: LiveOptions,
    client: Option<ContextClient>,
    budget_started: Instant,
    last_attempt: Option<Instant>,
    status: Option<LiveStatus>,
}
#[derive(Clone)]
struct LiveState(Arc<Mutex<Cache>>);
pub fn router(options: LiveOptions) -> Router {
    let state = LiveState(Arc::new(Mutex::new(Cache {
        options,
        client: None,
        budget_started: Instant::now(),
        last_attempt: None,
        status: None,
    })));
    Router::new()
        .route("/api/live/status", get(status))
        .with_state(state)
}
async fn status(State(state): State<LiveState>) -> Json<LiveStatus> {
    let mut cache = state.0.lock().await;
    if cache
        .last_attempt
        .is_some_and(|t| t.elapsed() < Duration::from_secs(cache.options.cache_seconds))
        && let Some(result) = &cache.status
    {
        return Json(result.clone());
    }
    let last_success = cache
        .status
        .as_ref()
        .and_then(|s| s.sources.first())
        .and_then(|s| s.last_successful_capture_at);
    let result = if !cache.options.enabled {
        LiveStatus::failed(Utc::now(), "LIVE_DISABLED", last_success)
    } else if !cache.options.source_checks_passed {
        LiveStatus::failed(Utc::now(), "SOURCE_CHECKS_REQUIRED", last_success)
    } else {
        if cache.budget_started.elapsed() >= Duration::from_secs(3600) {
            cache.client = None;
            cache.budget_started = Instant::now();
        }
        if cache.client.is_none() {
            cache.client = ContextClient::from_env()
                .and_then(|c| c.with_budget(cache.options.hourly_credits))
                .ok();
        }
        if let Some(client) = &cache.client {
            let capture = client
                .scrape_fogos_markdown(emberwatch_intelligence::live::FOGOS_LIST)
                .await;
            let capture = match capture {
                Ok(first)
                    if first.markdown.success == Some(true)
                        && !first.is_partial
                        && first
                            .markdown
                            .data
                            .as_deref()
                            .is_some_and(|md| md.contains("https://fogos.pt/pt/fogo/")) =>
                {
                    client
                        .scrape_fogos(emberwatch_intelligence::live::FOGOS_LIST)
                        .await
                }
                other => other,
            };
            match capture {
                Ok(response) => {
                    let mut result = LiveStatus::from_capture(&response, Utc::now());
                    if result.sources[0].last_successful_capture_at.is_none() {
                        result.sources[0].last_successful_capture_at = last_success;
                    }
                    result
                }
                Err(error) => LiveStatus::failed(Utc::now(), &error.code, last_success),
            }
        } else {
            LiveStatus::failed(Utc::now(), "UNCONFIGURED", last_success)
        }
    };
    cache.last_attempt = Some(Instant::now());
    cache.status = Some(result.clone());
    Json(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    #[tokio::test]
    async fn offline_endpoint_matches_shape_without_claiming_empty() {
        let response = router(LiveOptions::offline())
            .oneshot(
                Request::builder()
                    .uri("/api/live/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let bytes = axum::body::to_bytes(response.into_body(), 100000)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["mode"], "live");
        assert_eq!(value["state"], "check_failed");
        assert!(value["statement"].is_null());
        assert_eq!(value["sources"][0]["capture_succeeded"], false);
        assert_eq!(value["sources"][0]["positively_empty"], false);
        for key in [
            "checked_at",
            "state",
            "statement",
            "statement_en",
            "active",
            "sources",
            "origin",
        ] {
            assert!(value.get(key).is_some(), "missing fixture field {key}");
        }
    }
}
