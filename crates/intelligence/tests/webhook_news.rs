use chrono::{TimeZone, Utc};
use emberwatch_domain::{DataMode, FixedClock, PORTUGAL_BBOX, RunContext};
use emberwatch_intelligence::{
    context::SearchResponse,
    news::{cited_summary, normalize_news},
    webhook::*,
};
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;
use std::sync::{Arc, Mutex};

fn receiver() -> WebhookReceiver {
    WebhookReceiver::new(
        "whsec_fixture_only".into(),
        WebhookBinding {
            source_id: "news-monitor".into(),
            expected_upstream_id: "mon_fixture".into(),
            upstream_id_pointer: "/monitor_id".into(),
            allowed_events: vec!["run.completed".into()],
        },
    )
    .unwrap()
}
fn signed(body: &[u8], timestamp: i64) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(b"whsec_fixture_only").unwrap();
    mac.update(format!("{timestamp}.").as_bytes());
    mac.update(body);
    format!(
        "t={timestamp},v1={}",
        hex::encode(mac.finalize().into_bytes())
    )
}
fn event() -> Vec<u8> {
    br#"{ "id": "evt_fixture", "event":"run.completed", "created_at":"2026-01-05T11:00:00Z", "data":{"monitor_id":"mon_fixture","run":{"id":"run_fixture"}} }"#.to_vec()
}
fn request<'a>(body: &'a [u8], signature: &'a str) -> WebhookRequest<'a> {
    WebhookRequest {
        body,
        signature,
        event_id: "evt_fixture",
        event_type: "run.completed",
        now: Utc.with_ymd_and_hms(2026, 1, 5, 12, 0, 0).unwrap(),
    }
}

#[test]
fn verifies_raw_body_and_attempt_time_not_original_event_time() {
    let body = event();
    let signature = signed(&body, 1767614400);
    let verified = receiver().verify(request(&body, &signature)).unwrap();
    assert_eq!(verified.id, "evt_fixture");
    assert_eq!(
        verified.created_at,
        Utc.with_ymd_and_hms(2026, 1, 5, 11, 0, 0).unwrap()
    );
}
#[test]
fn rejects_tamper_stale_future_duplicate_header_and_wrong_source() {
    let body = event();
    let signature = signed(&body, 1767614400);
    let mut tampered = body.clone();
    tampered.push(b' ');
    assert!(receiver().verify(request(&tampered, &signature)).is_err());
    for time in [1767614099, 1767614701] {
        assert!(
            receiver()
                .verify(request(&body, &signed(&body, time)))
                .is_err()
        );
    }
    assert!(
        receiver()
            .verify(request(&body, &format!("{signature},t=1767614400")))
            .is_err()
    );
    let wrong = String::from_utf8(body)
        .unwrap()
        .replace("mon_fixture", "mon_other")
        .into_bytes();
    assert!(
        receiver()
            .verify(request(&wrong, &signed(&wrong, 1767614400)))
            .is_err()
    );
}

struct Inbox(Mutex<Vec<String>>);
impl WebhookInbox for Inbox {
    fn persist_once<'a>(&'a self, event: &'a VerifiedWebhook) -> InboxFuture<'a> {
        Box::pin(async move {
            let mut ids = self.0.lock().unwrap();
            if ids.contains(&event.id) {
                Ok(ReceiptDisposition::Duplicate)
            } else {
                ids.push(event.id.clone());
                Ok(ReceiptDisposition::Inserted)
            }
        })
    }
}
#[tokio::test]
async fn retry_is_deduped_by_verified_event_before_ack() {
    let inbox = Inbox(Mutex::new(vec![]));
    let body = event();
    let signature = signed(&body, 1767614400);
    assert_eq!(
        receiver()
            .receive(request(&body, &signature), &inbox)
            .await
            .unwrap(),
        ReceiptDisposition::Inserted
    );
    assert_eq!(
        receiver()
            .receive(request(&body, &signature), &inbox)
            .await
            .unwrap(),
        ReceiptDisposition::Duplicate
    );
    assert_eq!(inbox.0.lock().unwrap().len(), 1);
}

#[test]
fn copied_anepc_news_preserves_shared_lineage_and_citations() {
    let time = Utc.with_ymd_and_hms(2026, 1, 5, 12, 0, 0).unwrap();
    let ctx = RunContext {
        run_id: "live-fixture".into(),
        scenario_id: None,
        mode: DataMode::Live,
        epoch: 0,
        bbox: PORTUGAL_BBOX,
        window_start: time,
        window_end: time,
        clock: Arc::new(FixedClock(time)),
    };
    let response:SearchResponse=serde_json::from_value(json!({"request_id":"fixture","results":[{"url":"https://www.rtp.pt/noticias/pais/synthetic-fixture","title":"Synthetic rural fire report","description":"not authoritative","markdown":{"code":"SUCCESS","finalDOMState":"loaded","markdown":"Synthetic fixture only. Segundo a ANEPC, há uma ocorrência rural em Águeda."}}]})).unwrap();
    let batch = normalize_news(&ctx, &response).unwrap();
    assert_eq!(batch.evidence.len(), 1);
    assert_eq!(batch.evidence[0].provenance.origin_group, "anepc");
    assert_eq!(batch.evidence[0].position, None);
    assert_eq!(batch.evidence[0].provenance.event_at, None);
    let (pt, en, citations) = cited_summary(&batch.evidence);
    assert!(pt.contains("ANEPC"));
    assert!(en.contains("ANEPC"));
    assert_eq!(
        citations,
        vec!["https://www.rtp.pt/noticias/pais/synthetic-fixture"]
    );
}
