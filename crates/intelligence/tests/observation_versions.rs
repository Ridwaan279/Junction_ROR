//! Archive keys must identify immutable observations, not mutable source entities.
use chrono::{TimeZone, Utc};
use emberwatch_domain::{DataMode, FixedClock, PORTUGAL_BBOX, RunContext};
use emberwatch_intelligence::{
    context::{ScrapeResponse, SearchResponse},
    fogos::normalize_fogos,
    ipma::{TimeBasis, normalize_fire_risk, normalize_observations},
    news::normalize_news,
};
use serde_json::json;
use std::sync::Arc;

fn context(minute: u32) -> RunContext {
    let at = Utc.with_ymd_and_hms(2026, 1, 5, 12, minute, 0).unwrap();
    RunContext {
        run_id: "version-fixture".into(),
        scenario_id: None,
        mode: DataMode::Live,
        epoch: 0,
        bbox: PORTUGAL_BBOX,
        window_start: at - chrono::Duration::hours(2),
        window_end: at,
        clock: Arc::new(FixedClock(at)),
    }
}

fn fogos_page(status: &str, visible_id: bool) -> ScrapeResponse {
    let external_id = visible_id.then_some("visible-fixture-1");
    let excerpt = format!(
        "{} Local sintético — Águeda — Incêndio rural — 2026-01-05T11:00:00Z — {status}",
        external_id.unwrap_or_default()
    );
    serde_json::from_value(json!({
        "url":"https://fogos.pt/pt", "request_id":"same-capture-fixture",
        "markdown":{"requested":true,"success":true,"data":excerpt},
        "json":{"requested":true,"success":true,"data":{
            "incidents":[{"external_id":external_id,"locality":"Local sintético","municipality":"Águeda", "raw_type":"Incêndio rural", "raw_status":status,"start_text":"2026-01-05T11:00:00Z","visible_excerpt":excerpt}],
            "coverage":"partial","coverage_reason":"synthetic fixture","explicit_empty_text":null}}
    })).unwrap()
}

#[test]
fn fogos_versions_changed_capture_and_body_but_keeps_visible_identity_and_retry() {
    let page = fogos_page("Em Curso", true);
    let original = normalize_fogos(&context(0), &page)
        .unwrap()
        .evidence
        .remove(0);
    let retry = normalize_fogos(&context(0), &page)
        .unwrap()
        .evidence
        .remove(0);
    let next_capture = normalize_fogos(&context(5), &page)
        .unwrap()
        .evidence
        .remove(0);
    let corrected = normalize_fogos(&context(0), &fogos_page("Em Resolução", true))
        .unwrap()
        .evidence
        .remove(0);
    assert_eq!(
        original, retry,
        "A retry of the same capture must preserve IDs and all timestamps"
    );
    assert_ne!(
        original.id, next_capture.id,
        "A later capture must not overwrite the first archive entry"
    );
    assert_ne!(
        original.id, corrected.id,
        "A source correction must preserve the original observation"
    );
    assert_eq!(
        original.provenance.external_id.as_deref(),
        Some("visible-fixture-1")
    );
    assert_eq!(
        original.provenance.external_id,
        corrected.provenance.external_id
    );
    assert_eq!(original.provenance.event_at, corrected.provenance.event_at);
}

#[test]
fn fogos_composite_source_identity_groups_revisions_without_invented_external_id() {
    let original = normalize_fogos(&context(0), &fogos_page("Em Curso", false))
        .unwrap()
        .evidence
        .remove(0);
    let revised = normalize_fogos(&context(5), &fogos_page("Em Resolução", false))
        .unwrap()
        .evidence
        .remove(0);
    assert!(original.provenance.external_id.is_none());
    assert!(
        original.attributes["source_identity"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert_eq!(
        original.attributes["source_identity"],
        revised.attributes["source_identity"]
    );
    assert_ne!(original.id, revised.id);
}

#[test]
fn news_versions_updated_article_and_title_with_same_url_and_preserves_retry() {
    let original_page: SearchResponse = serde_json::from_value(json!({"request_id":"fixture", "results":[{
        "url":"https://www.rtp.pt/noticias/pais/synthetic-fixture", "title":"Synthetic report", "description":"Synthetic fixture only",
        "markdown":{"code":"SUCCESS", "finalDOMState":"loaded", "markdown":"Synthetic ANEPC report: Em Curso"}
    }]})).unwrap();
    let original = normalize_news(&context(0), &original_page)
        .unwrap()
        .evidence
        .remove(0);
    let retry = normalize_news(&context(0), &original_page)
        .unwrap()
        .evidence
        .remove(0);
    let next_capture = normalize_news(&context(5), &original_page)
        .unwrap()
        .evidence
        .remove(0);
    let mut changed_body = original_page.clone();
    changed_body.results[0].markdown.markdown = Some("Synthetic ANEPC report: Em Resolução".into());
    let revised = normalize_news(&context(0), &changed_body)
        .unwrap()
        .evidence
        .remove(0);
    let mut changed_title = original_page.clone();
    changed_title.results[0].title = "Corrected synthetic report".into();
    let retitled = normalize_news(&context(0), &changed_title)
        .unwrap()
        .evidence
        .remove(0);
    assert_eq!(original, retry);
    assert_ne!(original.id, next_capture.id);
    assert_ne!(original.id, revised.id);
    assert_ne!(original.id, retitled.id);
    assert_eq!(
        original.provenance.external_id,
        revised.provenance.external_id
    );
    assert_eq!(
        original.provenance.source_url,
        revised.provenance.source_url
    );
}

#[test]
fn ipma_versions_station_corrections_and_later_captures_without_changing_valid_time() {
    let stations = json!([{"geometry":{"type":"Point","coordinates":[-8.5,40.6]},"properties":{"idEstacao":"station-1"}}]);
    let records = json!({"2026-01-05T11:00:00Z":{"station-1":{"temperatura":20.0,"humidade":40.0,"intensidadeVento":5.0,"idDireccVento":7}}});
    let original = normalize_observations(&context(0), &records, &stations, TimeBasis::Unknown)
        .unwrap()
        .weather
        .remove(0);
    let retry = normalize_observations(&context(0), &records, &stations, TimeBasis::Unknown)
        .unwrap()
        .weather
        .remove(0);
    let next_capture = normalize_observations(&context(5), &records, &stations, TimeBasis::Unknown)
        .unwrap()
        .weather
        .remove(0);
    let mut correction = records.clone();
    correction["2026-01-05T11:00:00Z"]["station-1"]["temperatura"] = json!(21.0);
    let revised = normalize_observations(&context(0), &correction, &stations, TimeBasis::Unknown)
        .unwrap()
        .weather
        .remove(0);
    assert_eq!(original, retry);
    assert_ne!(original.id, next_capture.id);
    assert_ne!(original.id, revised.id);
    assert_eq!(original.valid_at, revised.valid_at);
    assert_eq!(
        original.provenance.external_id.as_deref(),
        Some("station-1")
    );
    assert_eq!(
        original.provenance.external_id,
        revised.provenance.external_id
    );
}

#[test]
fn ipma_versions_daily_risk_revisions_without_changing_municipality_or_day() {
    let risk = json!({"dataPrev":"2026-01-05", "local":{"0101":{"DICO":"0101", "longitude":-8.5,"latitude":40.6,"data":{"rcm":2}}}});
    let original = normalize_fire_risk(&context(0), &risk)
        .unwrap()
        .weather
        .remove(0);
    let retry = normalize_fire_risk(&context(0), &risk)
        .unwrap()
        .weather
        .remove(0);
    let next_capture = normalize_fire_risk(&context(5), &risk)
        .unwrap()
        .weather
        .remove(0);
    let mut correction = risk.clone();
    correction["local"]["0101"]["data"]["rcm"] = json!(3);
    let revised = normalize_fire_risk(&context(0), &correction)
        .unwrap()
        .weather
        .remove(0);
    assert_eq!(original, retry);
    assert_ne!(original.id, next_capture.id);
    assert_ne!(original.id, revised.id);
    assert_eq!(original.valid_at, revised.valid_at);
    assert_eq!(original.provenance.external_id.as_deref(), Some("0101"));
    assert_eq!(
        original.provenance.external_id,
        revised.provenance.external_id
    );
}

#[test]
fn fogos_page_update_revision_cannot_reuse_an_immutable_observation_id() {
    let first_page = fogos_page("Em Curso", true);
    let mut revised_page = first_page.clone();
    revised_page.json.data.as_mut().unwrap()["page_update_text"] =
        json!("Updated in synthetic fixture");
    let original = normalize_fogos(&context(0), &first_page)
        .unwrap()
        .evidence
        .remove(0);
    let revised = normalize_fogos(&context(0), &revised_page)
        .unwrap()
        .evidence
        .remove(0);
    assert_ne!(original.attributes, revised.attributes);
    assert_ne!(original.id, revised.id);
}

#[test]
fn ipma_explicit_timezone_revision_does_not_reuse_assumed_timezone_record_id() {
    let stations = json!([{"geometry":{"type":"Point","coordinates":[-8.5,40.6]},"properties":{"idEstacao":"station-1"}}]);
    let record = json!({"temperatura":20.0,"intensidadeVento":5.0,"idDireccVento":7});
    let assumed = normalize_observations(
        &context(0),
        &json!({"2026-01-05T11:00:00":{"station-1":record}}),
        &stations,
        TimeBasis::UtcConfigured,
    )
    .unwrap()
    .weather
    .remove(0);
    let explicit = normalize_observations(
        &context(0),
        &json!({"2026-01-05T11:00:00Z":{"station-1":record}}),
        &stations,
        TimeBasis::Unknown,
    )
    .unwrap()
    .weather
    .remove(0);
    assert_eq!(assumed.valid_at, explicit.valid_at);
    assert_ne!(
        assumed.provenance.quality_flags,
        explicit.provenance.quality_flags
    );
    assert_ne!(assumed.id, explicit.id);
}
