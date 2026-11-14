use chrono::{TimeZone, Utc};
use emberwatch_domain::{DataMode, FixedClock, PORTUGAL_BBOX, RunContext, SourceHealth};
use emberwatch_intelligence::{
    context::ScrapeResponse,
    fogos::normalize_fogos,
    ipma::{TimeBasis, normalize_observations},
};
use serde_json::{Value, json};
use std::sync::Arc;

fn context() -> RunContext {
    let time = Utc.with_ymd_and_hms(2026, 1, 5, 12, 0, 0).unwrap();
    RunContext {
        run_id: "test-live".into(),
        scenario_id: None,
        mode: DataMode::Live,
        epoch: 0,
        bbox: PORTUGAL_BBOX,
        window_start: time - chrono::Duration::hours(1),
        window_end: time,
        clock: Arc::new(FixedClock(time)),
    }
}

fn page(rows: Value, markdown: &str, coverage: &str, empty: Value) -> ScrapeResponse {
    serde_json::from_value(json!({"url":"https://fogos.pt/pt", "markdown":{"requested":true,"success":true,"data":markdown},
      "json":{"requested":true,"success":true,"data":{"incidents":rows,"coverage":coverage,"coverage_reason":"fixture only", "explicit_empty_text":empty}},
      "request_id":"fixture-request","cache_metadata":{"age_ms":42}, "key_metadata":{"credits_consumed":5}})).unwrap()
}

#[test]
fn loading_shell_is_not_zero_fires() {
    let result = normalize_fogos(
        &context(),
        &page(
            json!([]),
            "Mapa de Incêndios\nLocal\nInício\nA carregar",
            "unknown",
            Value::Null,
        ),
    )
    .unwrap();
    assert_eq!(result.health, SourceHealth::Partial);
    assert!(!result.coverage.complete);
    assert!(!result.coverage.explicit_empty);
}

#[test]
fn empty_requires_literal_visible_statement_and_validated_coverage() {
    let response = page(
        json!([]),
        "Sem ocorrências ativas",
        "complete",
        json!("Sem ocorrências ativas"),
    );
    assert_ne!(
        normalize_fogos(&context(), &response).unwrap().health,
        SourceHealth::EmptyVerified
    );
    let result =
        emberwatch_intelligence::fogos::normalize_fogos_with_coverage(&context(), &response, true)
            .unwrap();
    assert_eq!(result.health, SourceHealth::EmptyVerified);
    let invented = page(
        json!([]),
        "A carregar",
        "complete",
        json!("Sem ocorrências ativas"),
    );
    assert_ne!(
        emberwatch_intelligence::fogos::normalize_fogos_with_coverage(&context(), &invented, true)
            .unwrap()
            .health,
        SourceHealth::EmptyVerified
    );
}

#[test]
fn nullable_fields_lineage_usage_and_retry_identity_are_preserved() {
    let row = json!({"external_id":"synthetic-42", "locality":"Local sintético", "municipality":"Águeda", "raw_type":"Incêndio rural", "raw_status":"Em Curso", "visible_excerpt":"Local sintético — Águeda — Em Curso"});
    let response = page(
        json!([row.clone(), row]),
        "Local sintético — Águeda — Em Curso",
        "partial",
        Value::Null,
    );
    let first = normalize_fogos(&context(), &response).unwrap();
    let second = normalize_fogos(&context(), &response).unwrap();
    assert_eq!(first.evidence.len(), 1);
    let evidence = &first.evidence[0];
    assert_eq!(evidence.id, second.evidence[0].id);
    assert!(evidence.position.is_none());
    assert!(evidence.provenance.event_at.is_none());
    assert_eq!(evidence.provenance.origin_group, "anepc");
    assert_eq!(evidence.provenance.source_url, "https://fogos.pt/pt");
    assert_eq!(first.usage.credits, Some(5.0));
    assert_eq!(first.usage.cache_age_ms, Some(42));
}

#[test]
fn blocked_failed_partial_outputs_and_unsupported_rows_are_conservative() {
    let mut response = page(json!([]), "Access denied", "complete", Value::Null);
    response.markdown.success = Some(false);
    response.markdown.error_code = Some("WEBSITE_BLOCKED".into());
    assert_eq!(
        normalize_fogos(&context(), &response).unwrap().health,
        SourceHealth::Blocked
    );
    response.markdown.error_code = Some("REQUEST_TIMEOUT".into());
    assert_eq!(
        normalize_fogos(&context(), &response).unwrap().health,
        SourceHealth::Failed
    );
    let response = page(
        json!([{"locality":"Invented", "visible_excerpt":"Not in page"}]),
        "A carregar",
        "complete",
        Value::Null,
    );
    assert!(
        normalize_fogos(&context(), &response)
            .unwrap()
            .evidence
            .is_empty()
    );
}

#[test]
fn ipma_nodata_directions_units_and_explicit_time_basis() {
    let observations = json!({"2026-01-05T11:00":{"station-1":{"temperatura":-99.0,"humidade":40.0,"intensidadeVento":-99.0,"intensidadeVentoKM":36.0,"idDireccVento":7}}});
    let stations = json!([{"geometry":{"type":"Point","coordinates":[-8.5,40.6]},"properties":{"idEstacao":"station-1","localEstacao":"Synthetic"}}]);
    assert!(
        normalize_observations(&context(), &observations, &stations, TimeBasis::Unknown)
            .unwrap()
            .weather
            .is_empty()
    );
    let result = normalize_observations(
        &context(),
        &observations,
        &stations,
        TimeBasis::UtcConfigured,
    )
    .unwrap();
    assert_eq!(result.weather.len(), 1);
    let weather = &result.weather[0];
    assert_eq!(weather.wind_speed_mps, Some(10.0));
    assert_eq!(weather.wind_from_deg, Some(270.0));
    assert_eq!(weather.temperature_c, None);
    assert_eq!(weather.gust_mps, None);
    assert!(
        weather
            .provenance
            .quality_flags
            .iter()
            .any(|x| x == "timezone_assumed_utc_by_operator")
    );
    for (code, expected) in [
        (0, None),
        (1, Some(0.0)),
        (9, Some(0.0)),
        (2, Some(45.0)),
        (8, Some(315.0)),
        (99, None),
    ] {
        assert_eq!(
            emberwatch_intelligence::ipma::wind_direction(code),
            expected
        );
    }
}

#[test]
fn current_page_is_rejected_for_historical_replay() {
    let mut ctx = context();
    ctx.mode = DataMode::Historical;
    assert!(normalize_fogos(&ctx, &page(json!([]), "A carregar", "unknown", Value::Null)).is_err());
}

#[test]
fn empty_marker_cannot_be_an_arbitrary_visible_heading() {
    let response = page(
        json!([]),
        "Mapa de Incêndios",
        "complete",
        json!("Mapa de Incêndios"),
    );
    let result =
        emberwatch_intelligence::fogos::normalize_fogos_with_coverage(&context(), &response, true)
            .unwrap();
    assert_eq!(result.health, SourceHealth::Partial);
    assert!(!result.coverage.explicit_empty);
}

#[test]
fn unsupported_map_coordinates_do_not_become_incident_position() {
    let response = page(
        json!([{"locality":"Local sintético","visible_excerpt":"Local sintético","longitude":-8.5,"latitude":40.6}]),
        "Map centre: -8.5 40.6\nLocal sintético",
        "partial",
        Value::Null,
    );
    assert_eq!(
        normalize_fogos(&context(), &response).unwrap().evidence[0].position,
        None
    );
}

#[test]
fn ambiguous_rows_without_visible_identity_or_time_do_not_collapse() {
    let response = page(
        json!([{"locality":"Local sintético","visible_excerpt":"Local sintético — Em Curso"},{"locality":"Local sintético","visible_excerpt":"Local sintético — Em Resolução"}]),
        "Local sintético — Em Curso\nLocal sintético — Em Resolução",
        "partial",
        Value::Null,
    );
    assert_eq!(
        normalize_fogos(&context(), &response)
            .unwrap()
            .evidence
            .len(),
        2
    );
}
