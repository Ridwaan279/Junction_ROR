//! Regenerate the offline UI fixture from fully synthetic inputs:
//! `cargo run --release -p emberwatch-firecast --example fixture > web/public/firecast-fixture.json`
use chrono::{TimeZone, Utc};
use emberwatch_domain::{DataMode, FixedClock, Incident, RunContext};
use emberwatch_firecast::{AcquireRequest, Basis, FirecastConfig, acquire, compute, to_domain};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // Mirrors contracts/fixtures/snapshot.json (synthetic training incident).
    let at = Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap();
    let env = acquire(
        None,
        None,
        AcquireRequest {
            ignition: [-8.367, 40.735],
            uncertainty_m: 1200.0,
            place: "Sever do Vouga".into(),
            start: at,
            now: at,
            basis: Basis::Synthetic,
            max_context_credits: 0.0,
            network: false,
        },
    )
    .await;
    let result = compute(&env, &FirecastConfig::default()).expect("forecast");
    let context = RunContext {
        run_id: "synthetic-portugal".into(),
        scenario_id: Some("portugal-synthetic".into()),
        mode: DataMode::Synthetic,
        epoch: 0,
        bbox: emberwatch_domain::PORTUGAL_BBOX,
        window_start: at,
        window_end: at + chrono::Duration::hours(1),
        clock: Arc::new(FixedClock(at)),
    };
    let incident = Incident {
        id: "incident-training-1".into(),
        run_id: "synthetic-portugal".into(),
        mode: DataMode::Synthetic,
        title: "Synthetic rural fire report".into(),
        location_name: Some("Sever do Vouga (training location)".into()),
        position: Some([-8.367, 40.735]),
        uncertainty_m: Some(1200.0),
        status: "reported".into(),
        evidence_level: "reported".into(),
        evidence_ids: vec!["training-report-1".into()],
        origin_groups: vec!["synthetic-origin-1".into()],
        first_event_at: None,
        updated_at: at,
    };
    let mut forecast = to_domain(&context, &incident, &[], &[], &result).expect("contract");
    forecast.limitations.insert(
        0,
        "Offline fixture: every input is synthetic; regenerate with the fixture example.".into(),
    );
    println!("{}", serde_json::to_string(&forecast).unwrap());
}
