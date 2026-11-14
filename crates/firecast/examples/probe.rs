//! One live forecast against real services (needs network; Context.dev optional):
//! `CONTEXT_DEV_API_KEY=… cargo run --release -p emberwatch-firecast --example probe -- -8.367 40.735 "Sever do Vouga" [live|whatif]`
//! Prints every input's provenance/status and the forecast summary. Spends at most
//! `FIRECAST_CONTEXT_MAX_CREDITS` (default 25) Context.dev credits.
use chrono::Utc;
use emberwatch_firecast::{
    AcquireRequest, Basis, ContextDev, FirecastConfig, HttpSources, acquire, compute,
};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let lon: f64 = args.first().and_then(|v| v.parse().ok()).unwrap_or(-8.367);
    let lat: f64 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(40.735);
    let place = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "Sever do Vouga".into());
    let basis = if args.get(3).is_some_and(|m| m == "live") {
        Basis::Live
    } else {
        Basis::WhatIf
    };
    let credits = std::env::var("FIRECAST_CONTEXT_MAX_CREDITS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(25.0);
    let sources = HttpSources::new().expect("http");
    let context = ContextDev::from_env();
    let now = Utc::now();
    let env = acquire(
        Some(&sources),
        context.as_ref(),
        AcquireRequest {
            ignition: [lon, lat],
            uncertainty_m: 500.0,
            place,
            start: now,
            now,
            basis,
            max_context_credits: credits,
            network: true,
        },
    )
    .await;
    for input in &env.inputs {
        println!(
            "[{:>9}] {:<10} {} — {}",
            input.status, input.kind, input.source, input.detail
        );
    }
    let result = compute(&env, &FirecastConfig::default()).expect("forecast");
    let s = &result.summary;
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "basis": s["basis"], "head": s["head"], "horizons": s["horizons"], "weather": s["weather"], "moisture": s["moisture"],
        "fuel": s["fuel"], "smoke": s["smoke"], "intel": {"agreement": s["intel"]["agreement"], "adjustments": s["intel"]["adjustments"], "credits": s["intel"]["credits_used"]},
        "warnings": s["warnings"],
    })).unwrap());
}
