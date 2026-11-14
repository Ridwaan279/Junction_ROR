use emberwatch_domain::{DataMode, FixedClock, RunContext, Snapshot};
use emberwatch_response::*;
use std::sync::Arc;

fn fixture() -> (Snapshot, RunContext) {
    let snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../contracts/fixtures/snapshot.json")).unwrap();
    let scenario = &snapshot.scenario;
    let context = RunContext {
        run_id: scenario.run_id.clone(),
        scenario_id: Some(scenario.id.clone()),
        mode: DataMode::Synthetic,
        epoch: 3,
        bbox: emberwatch_domain::PORTUGAL_BBOX,
        window_start: scenario.start_at,
        window_end: scenario.end_at,
        clock: Arc::new(FixedClock(scenario.current_at)),
    };
    (snapshot, context)
}

#[test]
fn forecast_retains_scope_epoch_inputs_and_illustrative_arrival_geometry() {
    let (snapshot, context) = fixture();
    let grid = Grid::new(2, 1, 500.0, vec![Cell::default(); 2]).unwrap();
    let spread = simulate_spread(
        &grid,
        &SpreadInput {
            seed: 760,
            ignitions: vec![CellPosition { x: 0, y: 0 }],
            steps: 1,
            step_seconds: 60,
            base_probability: 1.0,
            wind: WindVector::default(),
        },
    )
    .unwrap();
    let forecast = build_forecast(
        &context,
        &snapshot.incidents[0],
        &grid,
        LocalProjection {
            origin_lon_lat: [-8.37, 40.73],
        },
        &spread,
        &snapshot.evidence,
        &snapshot.weather,
    )
    .unwrap();
    assert_eq!(forecast.run_id, "synthetic-portugal");
    assert_eq!(forecast.epoch, 3);
    assert_eq!(forecast.input_evidence_ids, vec!["training-report-1"]);
    assert!(forecast.illustrative);
    assert_eq!(forecast.geometry["type"], "FeatureCollection");
    assert_eq!(forecast.geometry["features"].as_array().unwrap().len(), 2);
    assert_eq!(
        forecast.geometry["features"][1]["properties"]["arrival_seconds"],
        60
    );
    let first_lon = forecast.geometry["features"][0]["geometry"]["coordinates"][0][0][0]
        .as_f64()
        .unwrap();
    assert!((first_lon + 8.37).abs() < 1e-8);
}

#[test]
fn future_or_other_run_evidence_cannot_leak_into_response_or_briefing() {
    let (mut snapshot, context) = fixture();
    snapshot.evidence[0].provenance.available_at =
        context.clock.now() + chrono::Duration::seconds(1);
    assert!(build_briefing(&context, &snapshot.incidents[0], &snapshot.evidence, None).is_err());
    snapshot.evidence[0].provenance.available_at = context.clock.now();
    snapshot.evidence[0].run_id = "different-run".into();
    assert!(build_briefing(&context, &snapshot.incidents[0], &snapshot.evidence, None).is_err());
}

#[test]
fn bilingual_briefing_has_real_input_citations_text_fallback_and_no_invented_status() {
    let (snapshot, context) = fixture();
    let briefing =
        build_briefing(&context, &snapshot.incidents[0], &snapshot.evidence, None).unwrap();
    assert!(briefing.text_pt.contains("Simulação"));
    assert!(briefing.text_en.contains("Simulation"));
    assert!(briefing.text_en.contains("reported"));
    assert_eq!(
        briefing.citations,
        vec!["https://example.org/emberwatch/synthetic/report-1"]
    );
    assert_eq!(briefing.audio_url, None);
    assert!(briefing.simulated);
}

#[test]
fn synthetic_demo_bundle_produces_cited_geometry_route_and_briefing_but_refuses_live_inputs() {
    let (mut snapshot, mut context) = fixture();
    let (forecast, plan, briefing) = demo_bundle(
        &context,
        &snapshot.incidents[0],
        &snapshot.evidence,
        &snapshot.weather,
    )
    .unwrap();
    assert!(forecast.illustrative && plan.simulated && briefing.simulated);
    assert!(forecast.geometry["features"].as_array().unwrap().len() > 1);
    assert!(plan.route.len() > 2 && !plan.return_route.is_empty());
    assert_eq!(plan.route.last(), plan.return_route.first());
    assert_eq!(plan.route.first(), plan.return_route.last());
    assert!(plan.reserve_pct >= 20.0);
    assert!(!briefing.citations.is_empty());
    assert!(forecast.limitations.iter().any(|l| l.contains("synthetic")));
    context.mode = DataMode::Live;
    snapshot.incidents[0].mode = DataMode::Live;
    snapshot.evidence[0].mode = DataMode::Live;
    assert!(
        demo_bundle(
            &context,
            &snapshot.incidents[0],
            &snapshot.evidence,
            &snapshot.weather
        )
        .is_err()
    );
}
