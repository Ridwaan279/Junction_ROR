use chrono::{TimeZone, Utc};
use emberwatch_domain::{DataMode, FixedClock, Incident, RunContext};
use emberwatch_firecast::environment::GRID_CELLS;
use emberwatch_firecast::geo::bearing_delta_deg;
use emberwatch_firecast::{
    AcquireRequest, Basis, FirecastConfig, acquire, compute, plan_drone, to_domain,
};
use std::sync::Arc;

fn request() -> AcquireRequest {
    AcquireRequest {
        ignition: [-8.367, 40.735],
        uncertainty_m: 400.0,
        place: "Sever do Vouga".into(),
        start: Utc.with_ymd_and_hms(2026, 8, 10, 13, 0, 0).unwrap(),
        now: Utc.with_ymd_and_hms(2026, 8, 10, 14, 0, 0).unwrap(),
        basis: Basis::Synthetic,
        max_context_credits: 0.0,
        network: false,
    }
}

fn context() -> RunContext {
    RunContext {
        run_id: "run-test".into(),
        scenario_id: Some("scenario-test".into()),
        mode: DataMode::Synthetic,
        epoch: 3,
        bbox: emberwatch_domain::PORTUGAL_BBOX,
        window_start: request().start,
        window_end: request().now + chrono::Duration::hours(6),
        clock: Arc::new(FixedClock(request().now)),
    }
}

fn incident() -> Incident {
    Incident {
        id: "incident-test".into(),
        run_id: "run-test".into(),
        mode: DataMode::Synthetic,
        title: "Synthetic test fire".into(),
        location_name: Some("Sever do Vouga".into()),
        position: Some([-8.367, 40.735]),
        uncertainty_m: Some(400.0),
        status: "reported".into(),
        evidence_level: "reported".into(),
        evidence_ids: vec![],
        origin_groups: vec![],
        first_event_at: Some(request().start),
        updated_at: request().start,
    }
}

#[tokio::test]
async fn synthetic_forecast_is_physical_deterministic_and_labelled() {
    let env = acquire(None, None, request()).await;
    assert_eq!(env.basis, Basis::Synthetic);
    assert!(
        env.inputs
            .iter()
            .all(|i| i.status == "synthetic" || i.status == "skipped")
    );
    assert_eq!(env.width, GRID_CELLS);
    let a = compute(&env, &FirecastConfig::default()).unwrap();
    let b = compute(&env, &FirecastConfig::default()).unwrap();
    assert_eq!(
        a.geojson["features"], b.geojson["features"],
        "ensemble must be reproducible"
    );

    // Spread runs downwind: synthetic wind is from ~310–325°, so the head heads ~130–145°.
    let head = a.summary["head"]["bearing_deg"].as_f64().unwrap();
    assert!(bearing_delta_deg(head, 137.0) < 35.0, "head bearing {head}");
    let horizons = a.summary["horizons"].as_array().unwrap();
    let p50: Vec<f64> = horizons
        .iter()
        .map(|h| h["area_ha_p50"].as_f64().unwrap())
        .collect();
    assert!(p50.windows(2).all(|w| w[0] <= w[1]), "areas grow: {p50:?}");
    for h in horizons {
        assert!(
            h["area_ha_p10"].as_f64() <= h["area_ha_p50"].as_f64()
                && h["area_ha_p50"].as_f64() <= h["area_ha_p90"].as_f64()
        );
    }
    let features = a.geojson["features"].as_array().unwrap();
    let layer = |name: &str| {
        features
            .iter()
            .filter(|f| f["properties"]["layer"] == name)
            .count()
    };
    assert!(layer("spread") >= 6 && layer("smoke") >= 3 && layer("wind") == 49);
    assert_eq!(layer("head_vector"), 1);
    // Smoke drifts with the transport wind (≈ toward the south-east).
    let axis = features
        .iter()
        .find(|f| f["properties"]["layer"] == "smoke_axis")
        .unwrap();
    assert!(bearing_delta_deg(axis["properties"]["toward_deg"].as_f64().unwrap(), 140.0) < 30.0);
    // Every polygon ring is closed.
    for f in features
        .iter()
        .filter(|f| f["geometry"]["type"] == "MultiPolygon")
    {
        for poly in f["geometry"]["coordinates"].as_array().unwrap() {
            for ring in poly.as_array().unwrap() {
                let r = ring.as_array().unwrap();
                assert!(r.len() >= 4 && r.first() == r.last());
            }
        }
    }
    assert!(a.limitations.iter().any(|l| l.contains("synthetic")));
    assert!(a.summary["references"].as_array().unwrap().len() >= 8);
}

#[tokio::test]
async fn forecast_maps_to_the_shared_contract_and_drives_the_drone_planner() {
    let env = acquire(None, None, request()).await;
    let result = compute(&env, &FirecastConfig::default()).unwrap();
    let forecast = to_domain(&context(), &incident(), &[], &[], &result).unwrap();
    assert_eq!(forecast.run_id, "run-test");
    assert_eq!(forecast.epoch, 3);
    assert!(forecast.illustrative);
    assert_eq!(forecast.geometry["type"], "FeatureCollection");
    assert!(forecast.geometry["firecast"]["head"].is_object());
    let mut other = incident();
    other.run_id = "another-run".into();
    assert!(to_domain(&context(), &other, &[], &[], &result).is_err());

    let plan = plan_drone(&context(), &incident(), &result).unwrap();
    assert!(plan.simulated && plan.route.len() >= 2 && !plan.return_route.is_empty());
    assert!(plan.reasons.iter().any(|r| r.contains("flank")));
    assert!(plan.reserve_pct >= 20.0);
    // No route point lies where the forecast says fire is likely within the hour.
    let burn = result.ensemble.burn_probability(3600.0 + 3600.0);
    for p in plan
        .route
        .iter()
        .skip(1)
        .take(plan.route.len().saturating_sub(2))
    {
        let (x, y) = result.frame.to_local(*p);
        let (i, j) = ((x / result.cell_m) as usize, (y / result.cell_m) as usize);
        assert!(
            burn[j * result.ensemble.width + i] < 0.9,
            "route crosses predicted fire"
        );
    }
}
