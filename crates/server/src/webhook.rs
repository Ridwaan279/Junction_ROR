use super::*;
use axum::{body::Bytes, http::HeaderMap};
use emberwatch_intelligence::webhook::{
    InboxFuture, ReceiptDisposition, VerifiedWebhook, WebhookBinding, WebhookInbox,
    WebhookReceiver, WebhookRequest,
};

struct DurableInbox {
    store: PgStore,
    run_id: String,
}
impl WebhookInbox for DurableInbox {
    fn persist_once<'a>(&'a self, event: &'a VerifiedWebhook) -> InboxFuture<'a> {
        Box::pin(async move {
            let record = serde_json::to_value(event).map_err(|_| SourceError {
                code: "WEBHOOK_PAYLOAD".into(),
                message: "invalid receipt record".into(),
            })?;
            let inserted = self
                .store
                .persist_receipt(
                    &self.run_id,
                    &event.source_id,
                    &event.id,
                    &event.body_hash,
                    record,
                )
                .await
                .map_err(|_| SourceError {
                    code: "WEBHOOK_PERSIST".into(),
                    message: "durable receipt unavailable or identity conflict".into(),
                })?;
            Ok(if inserted {
                ReceiptDisposition::Inserted
            } else {
                ReceiptDisposition::Duplicate
            })
        })
    }
}

pub(super) async fn receive(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let unavailable = || {
        ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "webhook requires a live run, PostgreSQL and a validated receiver binding".into(),
        )
    };
    let engine = state.engine.lock().await;
    if engine.scenario.mode != DataMode::Live {
        return Err(unavailable());
    }
    let run_id = engine.scenario.run_id.clone();
    drop(engine);
    let store = state.store.clone().ok_or_else(unavailable)?;
    let receiver = WebhookReceiver::new(
        std::env::var("CONTEXT_WEBHOOK_SECRET").map_err(|_| unavailable())?,
        WebhookBinding {
            source_id: std::env::var("CONTEXT_WEBHOOK_SOURCE_ID")
                .unwrap_or_else(|_| "fogos".into()),
            expected_upstream_id: std::env::var("CONTEXT_MONITOR_ID").map_err(|_| unavailable())?,
            upstream_id_pointer: std::env::var("CONTEXT_UPSTREAM_ID_POINTER")
                .map_err(|_| unavailable())?,
            allowed_events: vec!["change.detected".into(), "run.completed".into()],
        },
    )
    .map_err(|_| unavailable())?;
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                ApiError(
                    StatusCode::UNAUTHORIZED,
                    "missing webhook authentication headers".into(),
                )
            })
    };
    let request = WebhookRequest {
        body: &body,
        signature: header("x-context-signature")?,
        event_id: header("x-context-id")?,
        event_type: header("x-context-event")?,
        now: chrono::Utc::now(),
    };
    let disposition = receiver
        .receive(request, &DurableInbox { store, run_id })
        .await
        .map_err(|e| {
            let status = if e.code == "WEBHOOK_PERSIST" {
                StatusCode::SERVICE_UNAVAILABLE
            } else {
                StatusCode::UNAUTHORIZED
            };
            ApiError(status, e.message)
        })?;
    Ok((
        if disposition == ReceiptDisposition::Inserted {
            StatusCode::ACCEPTED
        } else {
            StatusCode::OK
        },
        Json(
            json!({"receipt":if disposition==ReceiptDisposition::Inserted {"stored"} else {"duplicate"},"ingested":false}),
        ),
    ))
}
