use crate::*;
use chrono::{Duration, TimeZone, Utc};

#[test]
fn seek_does_not_leak_future_and_rebuilds_same_incident() {
    let mut engine = Engine::synthetic().unwrap();
    assert_eq!(engine.snapshot().evidence.len(), 1);
    engine
        .seek(Utc.with_ymd_and_hms(2026, 11, 14, 12, 10, 0).unwrap())
        .unwrap();
    let later = engine.snapshot();
    assert_eq!(later.evidence.len(), 2);
    assert_eq!(later.incidents.len(), 1);
    assert_eq!(later.incidents[0].evidence_level, "reported");
    engine
        .seek(Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap())
        .unwrap();
    assert_eq!(engine.snapshot().evidence.len(), 1);
    assert!(engine.snapshot().forecasts.is_empty());
    engine.seek(later.scenario.current_at).unwrap();
    assert_eq!(engine.snapshot().incidents, later.incidents);
    assert_eq!(engine.scenario.epoch, 3);
}

#[test]
fn failed_batch_preserves_last_good_evidence_and_retry_is_idempotent() {
    let mut engine = Engine::synthetic().unwrap();
    let seed = engine.snapshot();
    let mut batch = SourceBatch {
        run_id: seed.scenario.run_id.clone(),
        source_id: "synthetic-public-page".into(),
        captured_at: seed.scenario.current_at,
        health: SourceHealth::Healthy,
        coverage: seed.sources[0].coverage.clone(),
        evidence: seed.evidence.clone(),
        weather: vec![],
        usage: Usage::default(),
    };
    assert_eq!(engine.ingest(batch.clone()).unwrap(), 0);
    batch.health = SourceHealth::Failed;
    batch.evidence.clear();
    batch.coverage.complete = false;
    engine.ingest(batch).unwrap();
    assert_eq!(engine.snapshot().incidents.len(), 1);
    assert_eq!(engine.snapshot().sources[0].health, SourceHealth::Failed);
}

#[test]
fn wrong_run_and_invalid_coordinates_are_rejected_atomically() {
    let mut engine = Engine::synthetic().unwrap();
    let seed = engine.snapshot();
    let mut evidence = seed.evidence[0].clone();
    evidence.id = "bad-record".into();
    evidence.position = Some([40.0, -8.0]);
    let batch = SourceBatch {
        run_id: seed.scenario.run_id.clone(),
        source_id: "synthetic-public-page".into(),
        captured_at: seed.scenario.current_at,
        health: SourceHealth::Healthy,
        coverage: seed.sources[0].coverage.clone(),
        evidence: vec![evidence],
        weather: vec![],
        usage: Usage::default(),
    };
    assert!(engine.ingest(batch.clone()).is_err());
    let mut wrong_run = batch;
    wrong_run.run_id = "other-run".into();
    assert!(engine.ingest(wrong_run).is_err());
    assert_eq!(engine.snapshot().evidence.len(), 1);
}

#[test]
fn paused_clock_does_not_advance_and_seek_is_bounded() {
    let mut engine = Engine::synthetic().unwrap();
    let start = engine.scenario.current_at;
    engine.advance(Duration::seconds(1));
    assert_eq!(engine.scenario.current_at, start);
    engine.play(60.0).unwrap();
    engine.advance(Duration::seconds(1));
    assert_eq!(engine.scenario.current_at, start + Duration::seconds(60));
    assert!(engine.play(f64::INFINITY).is_err());
    assert!(engine.seek(start - Duration::seconds(1)).is_err());
}

#[tokio::test]
#[ignore = "requires a dedicated PostgreSQL/PostGIS TEST_DATABASE_URL"]
async fn postgres_round_trip_preserves_archive_and_deduplicates_retries() {
    let url = std::env::var("TEST_DATABASE_URL").expect("dedicated TEST_DATABASE_URL");
    let store = persistence::PgStore::connect(&url).await.unwrap();
    let engine = Engine::synthetic().unwrap();
    store.save(&engine).await.unwrap();
    store.save(&engine).await.unwrap();
    let mut restored = store.load("synthetic-portugal").await.unwrap().unwrap();
    assert_eq!(restored.snapshot().evidence.len(), 1);
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM evidence WHERE run_id = $1")
        .bind("synthetic-portugal")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
    restored
        .seek(Utc.with_ymd_and_hms(2026, 11, 14, 12, 10, 0).unwrap())
        .unwrap();
    assert_eq!(restored.snapshot().evidence.len(), 2);
    assert_eq!(
        restored.snapshot().evidence[0].provenance.captured_at,
        Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap()
    );
}

#[test]
fn revised_observation_keeps_prior_capture_and_fuses_by_external_identity() {
    let mut engine = Engine::synthetic().unwrap();
    let seed = engine.snapshot();
    let mut revision = seed.evidence[0].clone();
    revision.id = "training-revision-3".into();
    revision.provenance.evidence_hash = "changed-content-v2".into();
    revision.provenance.captured_at += Duration::minutes(20);
    revision.provenance.available_at += Duration::minutes(20);
    revision.raw_status = Some("synthetic changed status".into());
    let batch = SourceBatch {
        run_id: seed.scenario.run_id.clone(),
        source_id: revision.provenance.source_id.clone(),
        captured_at: revision.provenance.captured_at,
        health: SourceHealth::Healthy,
        coverage: seed.sources[0].coverage.clone(),
        evidence: vec![revision],
        weather: vec![],
        usage: Usage::default(),
    };
    assert_eq!(engine.ingest(batch.clone()).unwrap(), 1);
    assert_eq!(engine.ingest(batch).unwrap(), 0);
    assert_eq!(engine.snapshot().evidence.len(), 1);
    assert_eq!(
        engine.snapshot().sources[0].captured_at,
        Some(seed.scenario.start_at)
    );
    engine
        .seek(seed.scenario.start_at + Duration::minutes(20))
        .unwrap();
    assert_eq!(engine.snapshot().evidence.len(), 3);
    assert_eq!(engine.snapshot().incidents.len(), 1);
    assert_eq!(engine.snapshot().incidents[0].evidence_ids.len(), 3);
    assert_eq!(
        engine.snapshot().sources[0].captured_at,
        Some(seed.scenario.start_at + Duration::minutes(20))
    );
}

#[test]
fn delayed_source_failure_cannot_reveal_a_future_success() {
    let mut engine = Engine::synthetic().unwrap();
    let seed = engine.snapshot();
    let mut batch = SourceBatch {
        run_id: seed.scenario.run_id.clone(),
        source_id: "synthetic-public-page".into(),
        captured_at: seed.scenario.start_at + Duration::minutes(20),
        health: SourceHealth::Healthy,
        coverage: seed.sources[0].coverage.clone(),
        evidence: vec![],
        weather: vec![],
        usage: Usage::default(),
    };
    engine.ingest(batch.clone()).unwrap();
    batch.captured_at = seed.scenario.start_at + Duration::minutes(10);
    batch.health = SourceHealth::Failed;
    engine.ingest(batch).unwrap();
    engine
        .seek(seed.scenario.start_at + Duration::minutes(10))
        .unwrap();
    assert_eq!(engine.snapshot().sources[0].health, SourceHealth::Failed);
    assert_eq!(
        engine.snapshot().sources[0].last_success_at,
        Some(seed.scenario.start_at)
    );
}

#[test]
fn a_failed_capture_can_recover_records_on_same_timestamp_retry() {
    let mut engine = Engine::synthetic().unwrap();
    let seed = engine.snapshot();
    let captured_at = seed.scenario.start_at + Duration::minutes(5);
    let mut batch = SourceBatch {
        run_id: seed.scenario.run_id.clone(),
        source_id: "synthetic-public-page".into(),
        captured_at,
        health: SourceHealth::Failed,
        coverage: seed.sources[0].coverage.clone(),
        evidence: vec![],
        weather: vec![],
        usage: Usage::default(),
    };
    assert_eq!(engine.ingest(batch.clone()).unwrap(), 0);
    let mut recovered = seed.evidence[0].clone();
    recovered.id = "recovered-observation".into();
    recovered.provenance.captured_at = captured_at;
    recovered.provenance.available_at = captured_at;
    batch.evidence.push(recovered);
    batch.health = SourceHealth::Healthy;
    assert_eq!(engine.ingest(batch.clone()).unwrap(), 1);
    assert_eq!(engine.ingest(batch).unwrap(), 0);
}
