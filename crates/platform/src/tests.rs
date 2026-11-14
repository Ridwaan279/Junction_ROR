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
