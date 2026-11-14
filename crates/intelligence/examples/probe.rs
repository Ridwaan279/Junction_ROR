//! One bounded probe, no scheduler or persisted third-party capture.
use emberwatch_domain::{DataMode, PORTUGAL_BBOX, RunContext, Source, SystemClock};
use emberwatch_intelligence::{
    context::ContextClient,
    ipma::{IpmaSource, TimeBasis},
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let now = chrono::Utc::now();
    let ctx = RunContext {
        run_id: "manual-live-probe".into(),
        scenario_id: None,
        mode: DataMode::Live,
        epoch: 0,
        bbox: PORTUGAL_BBOX,
        window_start: now - chrono::Duration::hours(24),
        window_end: now,
        clock: Arc::new(SystemClock),
    };
    let kind = std::env::args().nth(1).unwrap_or_else(|| "ipma".into());
    if kind == "fogos" {
        // This command is a single public-page feasibility probe, not approval for recurring reuse.
        let response = ContextClient::from_env()?
            .scrape_fogos("https://fogos.pt/pt")
            .await?;
        let result = emberwatch_intelligence::fogos::normalize_fogos(&ctx, &response)?;
        println!(
            "health={:?} validated_rows={} complete={} request_id={} credits={:?}",
            result.health,
            result.evidence.len(),
            result.coverage.complete,
            result.usage.request_id.as_deref().unwrap_or("unknown"),
            result.usage.credits
        );
    } else if kind == "ipma" {
        let basis = if std::env::var("IPMA_NAIVE_TIMEZONE").as_deref() == Ok("UTC") {
            TimeBasis::UtcConfigured
        } else {
            TimeBasis::Unknown
        };
        let result = IpmaSource::new(basis)?.fetch(&ctx).await?;
        println!(
            "health={:?} weather_records={} notes={:?}",
            result.health,
            result.weather.len(),
            result.coverage.notes
        );
    } else {
        return Err("Expected ipma or fogos".into());
    }
    Ok(())
}
