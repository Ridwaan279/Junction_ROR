//! Offline forecast on fully synthetic inputs (no network, no keys):
//! `cargo run --release -p emberwatch-firecast --example demo [out.geojson]`
use chrono::{TimeZone, Utc};
use emberwatch_firecast::{AcquireRequest, Basis, FirecastConfig, acquire, compute};

#[tokio::main]
async fn main() {
    let request = AcquireRequest {
        ignition: [-8.367, 40.735],
        uncertainty_m: 600.0,
        place: "Sever do Vouga".into(),
        start: Utc.with_ymd_and_hms(2026, 8, 10, 13, 0, 0).unwrap(),
        now: Utc.with_ymd_and_hms(2026, 8, 10, 14, 30, 0).unwrap(),
        basis: Basis::Synthetic,
        max_context_credits: 0.0,
        network: false,
    };
    let env = acquire(None, None, request).await;
    let result = compute(&env, &FirecastConfig::default()).expect("forecast");
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "runtime_ms": result.summary["runtime_ms"], "head": result.summary["head"], "horizons": result.summary["horizons"],
        "smoke": result.summary["smoke"], "moisture": result.summary["moisture"], "features": result.geojson["features"].as_array().map(|f| f.len()),
    })).unwrap());
    if let Some(path) = std::env::args().nth(1) {
        std::fs::write(&path, serde_json::to_vec(&result.geojson).unwrap()).unwrap();
        eprintln!("wrote {path}");
    }
}
