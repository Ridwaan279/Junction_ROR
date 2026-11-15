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
        let page = std::env::args().nth(2).unwrap_or_else(|| "table".into());
        let url = match page.as_str() {
            "map" => "https://fogos.pt/pt",
            "list" => "https://fogos.pt/pt/lista",
            "table" => "https://fogos.pt/pt/tabela",
            _ => return Err("Expected fogos map, list or table".into()),
        };
        // This command is a single public-page feasibility probe, not approval for recurring reuse.
        let captured_at = chrono::Utc::now();
        let response = ContextClient::from_env()?.scrape_fogos(url).await?;
        let result = emberwatch_intelligence::fogos::normalize_fogos(&ctx, &response)?;
        // Opt-in local diagnostics preserve only page content and safe outcome
        // metadata. Never serialize client credentials or account key metadata.
        if std::env::var("CONTEXT_PROBE_CAPTURE").as_deref() == Ok("1") {
            let directory = std::path::Path::new(".state/context-probe");
            std::fs::create_dir_all(directory)?;
            let capture = serde_json::json!({
                "url":response.url,"capture_started_at":captured_at,
                "capture_finished_at":chrono::Utc::now(),"request_id":response.request_id,
                "is_partial":response.is_partial,
                "markdown":{"success":response.markdown.success,"data":response.markdown.data,"error_code":response.markdown.error_code},
                "json":{"success":response.json.success,"data":response.json.data,"error_code":response.json.error_code},
                "health":result.health,"validated_rows":result.evidence.len(),
                "complete":result.coverage.complete,"explicit_empty":result.coverage.explicit_empty,
                "coverage_notes":result.coverage.notes,"usage":result.usage
            });
            std::fs::write(
                directory.join(format!("{page}.json")),
                serde_json::to_vec_pretty(&capture)?,
            )?;
        }
        println!(
            "url={} health={:?} validated_rows={} complete={} request_id={} credits={:?}",
            url,
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
