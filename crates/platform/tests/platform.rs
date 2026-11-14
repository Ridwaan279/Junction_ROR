use chrono::{Duration, Utc};
use emberwatch_domain::platform::*;
use emberwatch_platform::{Platform, Store, sources::parse_open_meteo, uploads::parse_upload};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

fn dataset() -> ScenarioDataset {
    serde_json::from_str(include_str!(
        "../../../contracts/fixtures/portugal-synthetic.json"
    ))
    .unwrap()
}

#[tokio::test]
async fn revised_anchor_keeps_incident_identity_and_latest_status() {
    let (platform, run) = loaded().await;
    let mut revised = dataset().evidence[0].clone();
    revised.available_at += Duration::minutes(15);
    revised.captured_at = revised.available_at;
    revised.status = OperationalStatus::Contained;
    revised.raw_status = Some("Synthetic contained revision".into());
    platform
        .load_into(run.id, vec![revised], vec![])
        .await
        .unwrap();
    let later = platform
        .seek(run.id, run.cursor + Duration::minutes(18))
        .await
        .unwrap();
    assert_eq!(later.incidents.len(), 1);
    assert_eq!(later.incidents[0].id, run.incidents[0].id);
    assert_eq!(later.incidents[0].first_seen, run.cursor);
    assert_eq!(later.incidents[0].status, OperationalStatus::Contained);
}

#[tokio::test]
async fn a_status_reversion_is_a_new_observation_not_an_old_retry() {
    let (platform, run) = loaded().await;
    let original = dataset().evidence[0].clone();
    let mut contained = original.clone();
    contained.status = OperationalStatus::Contained;
    contained.available_at += Duration::minutes(5);
    contained.captured_at = contained.available_at;
    platform
        .load_into(run.id, vec![contained], vec![])
        .await
        .unwrap();
    let mut reverted = original;
    reverted.available_at += Duration::minutes(10);
    reverted.captured_at = reverted.available_at;
    platform
        .load_into(run.id, vec![reverted.clone()], vec![])
        .await
        .unwrap();
    let later = platform.seek(run.id, reverted.available_at).await.unwrap();
    let report = later
        .evidence
        .iter()
        .find(|e| e.source_id == "synthetic-report")
        .unwrap();
    assert_eq!(report.status, OperationalStatus::Active);
    assert_eq!(report.available_at, reverted.available_at);
}

async fn loaded() -> (Platform, Scenario) {
    let platform = Platform::open(Store::memory(), vec![]).await.unwrap();
    let run = platform.load_dataset(dataset()).await.unwrap();
    (platform, run)
}

#[tokio::test]
async fn replay_respects_availability_lineage_pause_and_seek() {
    let (platform, initial) = loaded().await;
    assert_eq!(initial.evidence.len(), 1);
    assert_eq!(initial.weather.len(), 1);
    assert_eq!(initial.incidents[0].evidence_level, EvidenceLevel::Reported);
    let id = initial.id;
    platform.play(id, Some(60.0)).await.unwrap();
    platform.advance(Duration::seconds(10)).await.unwrap();
    let mirror = platform.run(id).await.unwrap();
    assert_eq!(mirror.evidence.len(), 2);
    assert_eq!(mirror.incidents.len(), 1);
    assert_eq!(mirror.incidents[0].evidence_level, EvidenceLevel::Reported);
    assert_eq!(mirror.incidents[0].origin_groups.len(), 1);
    platform.pause(id).await.unwrap();
    platform.advance(Duration::minutes(2)).await.unwrap();
    assert_eq!(platform.run(id).await.unwrap().cursor, mirror.cursor);
    platform.play(id, None).await.unwrap();
    platform.advance(Duration::seconds(15)).await.unwrap();
    let supported = platform.run(id).await.unwrap();
    assert_eq!(
        supported.incidents[0].evidence_level,
        EvidenceLevel::SatelliteSupported
    );
    assert_eq!(supported.weather.len(), 2);
    let reset = platform.seek(id, initial.cursor).await.unwrap();
    assert_eq!(reset.epoch, 1);
    assert!(!reset.playing);
    assert_eq!(reset.evidence.len(), 1);
    assert_eq!(reset.weather.len(), 1);
    assert_eq!(reset.incidents, initial.incidents);
    platform.seek(id, supported.cursor).await.unwrap();
    assert_eq!(
        platform.run(id).await.unwrap().incidents,
        supported.incidents
    );
    assert!(
        platform
            .seek(id, initial.cursor - Duration::seconds(1))
            .await
            .is_err()
    );
    assert!(platform.play(id, Some(f64::INFINITY)).await.is_err());
    assert!(platform.seek(live_run_id(), Utc::now()).await.is_err());
}

#[tokio::test]
async fn repeated_ingestion_and_restart_do_not_duplicate_incidents() {
    let store = Store::memory();
    let platform = Platform::open(store.clone(), vec![]).await.unwrap();
    let input = dataset();
    let run = platform.load_dataset(input.clone()).await.unwrap();
    platform
        .load_into(run.id, input.evidence.clone(), input.weather.clone())
        .await
        .unwrap();
    assert_eq!(platform.run(run.id).await.unwrap().evidence.len(), 1);
    platform.play(run.id, None).await.unwrap();
    let before_restart = platform.subscribe(run.id, None).await.unwrap().backlog[0].id;
    let restarted = Platform::open(store, vec![]).await.unwrap();
    let restored = restarted.run(run.id).await.unwrap();
    assert_eq!(restored.incidents, run.incidents);
    assert!(!restored.playing);
    assert_eq!(restored.epoch, 1);
    let backlog = restarted
        .subscribe(run.id, Some(before_restart))
        .await
        .unwrap()
        .backlog;
    assert_eq!(backlog[0].event_type, "run.reset");
    assert_eq!(backlog[0].payload["reason"], "server_restart");
    restarted.seek(run.id, input.spec.ends_at).await.unwrap();
    assert_eq!(restarted.run(run.id).await.unwrap().evidence.len(), 3);
}

#[tokio::test]
async fn source_revisions_do_not_leak_backwards_and_capture_retries_dedupe() {
    let (platform, run) = loaded().await;
    let mut revision = dataset().evidence[0].clone();
    revision.available_at += Duration::minutes(5);
    revision.captured_at = revision.available_at;
    revision.status = OperationalStatus::Contained;
    revision.raw_status = Some("Contained — synthetic revision".into());
    let batch = SourceBatch {
        source_id: revision.source_id.clone(),
        evidence: vec![revision.clone()],
        weather: vec![],
        coverage: Coverage {
            complete: true,
            explicit_empty: false,
            note: None,
        },
        captured_at: revision.captured_at,
    };
    assert_eq!(platform.ingest(run.id, batch.clone()).await.unwrap(), 1);
    assert_eq!(
        platform.run(run.id).await.unwrap().incidents[0].status,
        OperationalStatus::Active
    );
    let mut retry = batch.clone();
    retry.evidence[0].captured_at += Duration::minutes(1);
    retry.evidence[0].available_at += Duration::minutes(1);
    assert_eq!(platform.ingest(run.id, retry).await.unwrap(), 0);
    let updated = platform.seek(run.id, revision.available_at).await.unwrap();
    assert_eq!(updated.evidence.len(), 1);
    assert_eq!(updated.incidents[0].status, OperationalStatus::Contained);
    let original = platform.seek(run.id, run.cursor).await.unwrap();
    assert_eq!(original.incidents[0].status, OperationalStatus::Active);
}

#[tokio::test]
async fn sse_backlog_is_scoped_and_seek_emits_reset() {
    let (platform, run) = loaded().await;
    let mut subscription = platform.subscribe(run.id, None).await.unwrap();
    assert_eq!(subscription.backlog[0].event_type, "run.reset");
    let after = subscription.backlog[0].id;
    platform
        .seek(run.id, run.cursor + Duration::minutes(20))
        .await
        .unwrap();
    let event = subscription.receiver.recv().await.unwrap();
    assert_eq!(event.event_type, "run.reset");
    assert_eq!(event.epoch, 1);
    let replayed = platform.subscribe(run.id, Some(after)).await.unwrap();
    assert_eq!(replayed.backlog.len(), 1);
    assert!(replayed.backlog.iter().all(|e| e.run_id == run.id));
    assert!(
        platform
            .subscribe(live_run_id(), Some(after))
            .await
            .unwrap()
            .backlog
            .is_empty()
    );
    let future_cursor = platform.subscribe(run.id, Some(u64::MAX)).await.unwrap();
    assert_eq!(future_cursor.backlog[0].event_type, "run.reset");
}

#[tokio::test]
async fn file_store_reopens_atomically_and_failed_writes_do_not_commit_state() {
    let directory = std::env::temp_dir().join(format!("emberwatch-{}", Uuid::new_v4()));
    let path = directory.join("state.json");
    let platform = Platform::open(Store::file(&path), vec![]).await.unwrap();
    let run = platform.load_dataset(dataset()).await.unwrap();
    let reopened = Platform::open(Store::file(&path), vec![]).await.unwrap();
    assert_eq!(reopened.run(run.id).await.unwrap().incidents, run.incidents);
    // Replace the target with a directory to induce a commit failure.
    tokio::fs::remove_file(&path).await.unwrap();
    tokio::fs::create_dir(&path).await.unwrap();
    assert!(
        platform
            .seek(run.id, run.cursor + Duration::minutes(20))
            .await
            .is_err()
    );
    assert_eq!(platform.run(run.id).await.unwrap().epoch, 0);
    tokio::fs::remove_dir(&path).await.unwrap();
    if path.with_extension("tmp").exists() {
        tokio::fs::remove_file(path.with_extension("tmp"))
            .await
            .unwrap();
    }
    tokio::fs::remove_dir(directory).await.unwrap();
}

#[test]
fn firms_csv_validates_headers_dates_and_coordinates() {
    let csv = "latitude,longitude,acq_date,acq_time,satellite,frp,confidence\n40.735,-8.366,2024-09-15,930,N20,10,h\n";
    let batch = parse_upload("csv", csv, "upload:firms", Utc::now()).unwrap();
    assert_eq!(batch.evidence.len(), 1);
    assert_eq!(
        batch.evidence[0].observed_at.unwrap().to_rfc3339(),
        "2024-09-15T09:30:00+00:00"
    );
    assert_eq!(
        batch.evidence[0].availability_basis,
        AvailabilityBasis::Captured
    );
    assert!(parse_upload("csv", "foo,bar\n1,2", "upload:test", Utc::now()).is_err());
    assert!(
        parse_upload(
            "csv",
            &csv.replace("40.735", "140.735"),
            "upload:test",
            Utc::now()
        )
        .is_err()
    );
    assert!(
        parse_upload(
            "csv",
            &csv.replace("930", "2570"),
            "upload:test",
            Utc::now()
        )
        .is_err()
    );
}

#[test]
fn uploads_reject_nonpoint_geometry_and_invalid_rows() {
    let mut evidence = dataset().evidence[0].clone();
    evidence.point = Some(Point {
        longitude: 0.0,
        latitude: 100.0,
    });
    let jsonl = serde_json::to_string(&evidence).unwrap();
    assert!(parse_upload("jsonl", &jsonl, "upload:test", Utc::now()).is_err());
    let polygon = json!({"type":"FeatureCollection","features":[{
        "type":"Feature","geometry":{"type":"Polygon","coordinates":[]},"properties":{}
    }]});
    assert!(parse_upload("geojson", &polygon.to_string(), "upload:test", Utc::now()).is_err());
    let point = json!({"type":"FeatureCollection","features":[{
        "type":"Feature","geometry":{"type":"Point","coordinates":[-8.366,40.735]},
        "properties":dataset().evidence[0]
    }]});
    let batch = parse_upload("geojson", &point.to_string(), "upload:points", Utc::now()).unwrap();
    assert_eq!(batch.evidence[0].source_id, "upload:points");
}

#[test]
fn weather_preserves_nulls_and_rejects_invalid_wind() {
    let weather = json!({"latitude":40.7,"longitude":-8.3,"hourly":{
        "time":[1767268800],"temperature_2m":[28],"relative_humidity_2m":[null],
        "wind_speed_10m":[30],"wind_direction_10m":[270],"wind_gusts_10m":[null]
    }});
    let result = parse_open_meteo(&weather, Utc::now()).unwrap();
    assert_eq!(result[0].humidity_pct, None);
    assert_eq!(result[0].gust_kmh, None);
    let mut bad = weather;
    bad["hourly"]["wind_direction_10m"][0] = json!(360);
    assert_eq!(
        parse_open_meteo(&bad, Utc::now()).unwrap()[0].wind_from_deg,
        Some(0.0)
    );
    bad["hourly"]["wind_direction_10m"][0] = json!(900);
    assert!(parse_open_meteo(&bad, Utc::now()).is_err());
}

#[test]
fn scenario_window_rejects_fractional_day_overflow_and_reversed_boxes() {
    let mut spec = dataset().spec;
    spec.ends_at = spec.starts_at + Duration::days(31) + Duration::seconds(1);
    assert!(spec.validate().is_err());
    spec.ends_at = spec.starts_at + Duration::days(31);
    assert!(spec.validate().is_ok());
    spec.bbox = [-6.1, 36.9, -9.6, 42.2];
    assert!(spec.validate().is_err());
}

struct EmptySource;
impl Source for EmptySource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: "empty-test".into(),
            name: "Explicit empty source".into(),
            fetch: "fixture".into(),
            origin_group: "test".into(),
            enabled: true,
            cadence_s: 60,
            health: SourceHealth::Stale,
            validated: true,
            replayable: true,
            last_attempt: None,
            last_success: None,
            last_error: None,
            coverage: None,
        }
    }
    fn run(&self, _: RunContext) -> SourceFuture<'_> {
        Box::pin(async {
            Ok(SourceBatch {
                source_id: "empty-test".into(),
                evidence: vec![],
                weather: vec![],
                coverage: Coverage {
                    complete: true,
                    explicit_empty: true,
                    note: None,
                },
                captured_at: Utc::now(),
            })
        })
    }
}

#[tokio::test]
async fn source_cadence_and_unvalidated_sources_cannot_be_bypassed() {
    let platform = Platform::open(Store::memory(), vec![Arc::new(EmptySource)])
        .await
        .unwrap();
    platform.run_source("empty-test").await.unwrap();
    assert!(platform.run_source("empty-test").await.is_err());
    let source = platform
        .sources()
        .await
        .into_iter()
        .find(|s| s.id == "empty-test")
        .unwrap();
    assert_eq!(source.health, SourceHealth::EmptyVerified);
    assert!(
        platform
            .set_source_enabled("fogos-context", true)
            .await
            .is_err()
    );
}

struct InvalidSource;
impl Source for InvalidSource {
    fn descriptor(&self) -> SourceDescriptor {
        let mut source = EmptySource.descriptor();
        source.id = "invalid-test".into();
        source
    }
    fn run(&self, _: RunContext) -> SourceFuture<'_> {
        Box::pin(async {
            Ok(SourceBatch {
                source_id: "different-source".into(),
                evidence: vec![],
                weather: vec![],
                coverage: Coverage {
                    complete: true,
                    explicit_empty: true,
                    note: None,
                },
                captured_at: Utc::now(),
            })
        })
    }
}

#[tokio::test]
async fn malformed_adapter_results_are_failed_and_still_rate_limited() {
    let platform = Platform::open(Store::memory(), vec![Arc::new(InvalidSource)])
        .await
        .unwrap();
    assert!(platform.run_source("invalid-test").await.is_err());
    let source = platform
        .sources()
        .await
        .into_iter()
        .find(|s| s.id == "invalid-test")
        .unwrap();
    assert_eq!(source.health, SourceHealth::Failed);
    assert!(source.last_attempt.is_some());
    assert!(matches!(
        platform.run_source("invalid-test").await,
        Err(emberwatch_platform::PlatformError::Invalid(_))
    ));
}

#[tokio::test]
async fn partial_or_empty_batches_never_close_existing_incidents() {
    let (platform, run) = loaded().await;
    let empty = SourceBatch {
        source_id: "synthetic-report".into(),
        evidence: vec![],
        weather: vec![],
        coverage: Coverage {
            complete: false,
            explicit_empty: false,
            note: Some("Page failed to load".into()),
        },
        captured_at: Utc::now(),
    };
    platform.ingest(run.id, empty.clone()).await.unwrap();
    assert_eq!(platform.run(run.id).await.unwrap().incidents, run.incidents);
    let mut explicit_empty = empty;
    explicit_empty.coverage.complete = true;
    explicit_empty.coverage.explicit_empty = true;
    platform.ingest(run.id, explicit_empty).await.unwrap();
    assert_eq!(
        platform.run(run.id).await.unwrap().incidents[0].status,
        OperationalStatus::Active
    );
}

#[tokio::test]
async fn expired_sse_cursor_requests_snapshot_instead_of_incomplete_history() {
    let (platform, run) = loaded().await;
    let initial_id = platform.subscribe(run.id, None).await.unwrap().backlog[0].id;
    for _ in 0..520 {
        platform.seek(run.id, run.cursor).await.unwrap();
    }
    let subscription = platform.subscribe(run.id, Some(initial_id)).await.unwrap();
    assert_eq!(subscription.backlog.len(), 1);
    assert_eq!(subscription.backlog[0].event_type, "run.reset");
    assert_eq!(subscription.backlog[0].payload["reason"], "resync");
}

struct RegisteredFogosFixture(bool);
impl Source for RegisteredFogosFixture {
    fn descriptor(&self) -> SourceDescriptor {
        let mut descriptor = EmptySource.descriptor();
        descriptor.id = "fogos-context".into();
        descriptor.enabled = false;
        descriptor.validated = self.0;
        descriptor.health = if self.0 {
            SourceHealth::Stale
        } else {
            SourceHealth::Unconfigured
        };
        descriptor
    }
    fn run(&self, _: RunContext) -> SourceFuture<'_> {
        Box::pin(async { Err(SourceError::Unconfigured) })
    }
}

#[tokio::test]
async fn registering_agent_b_replaces_placeholder_but_preserves_operator_choice() {
    let store = Store::memory();
    Platform::open(store.clone(), vec![]).await.unwrap();
    let configured = Platform::open(store.clone(), vec![Arc::new(RegisteredFogosFixture(true))])
        .await
        .unwrap();
    let source = configured
        .set_source_enabled("fogos-context", true)
        .await
        .unwrap();
    assert!(source.validated && source.enabled);
    let restarted = Platform::open(store.clone(), vec![Arc::new(RegisteredFogosFixture(true))])
        .await
        .unwrap();
    assert!(
        restarted
            .sources()
            .await
            .into_iter()
            .find(|s| s.id == "fogos-context")
            .unwrap()
            .enabled
    );
    let revoked = Platform::open(store.clone(), vec![Arc::new(RegisteredFogosFixture(false))])
        .await
        .unwrap();
    assert!(
        revoked
            .set_source_enabled("fogos-context", true)
            .await
            .is_err()
    );
    let removed = Platform::open(store, vec![]).await.unwrap();
    assert!(
        !removed
            .sources()
            .await
            .into_iter()
            .find(|s| s.id == "fogos-context")
            .unwrap()
            .enabled
    );
}
