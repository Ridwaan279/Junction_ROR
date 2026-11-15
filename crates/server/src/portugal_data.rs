//! Bounded on-demand national news/weather collection, independent of active fires.
use axum::{Json, Router, extract::State, routing::get};
use chrono::Utc;
use emberwatch_intelligence::context::{ContextClient, Endpoint};
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

struct Cache {
    value: Option<Value>,
    at: Option<Instant>,
    budget_at: Instant,
    context: Option<ContextClient>,
}
pub fn router() -> Router {
    Router::new()
        .route("/api/live/portugal", get(status))
        .with_state(Arc::new(Mutex::new(Cache {
            value: None,
            at: None,
            budget_at: Instant::now(),
            context: None,
        })))
}
fn clipped(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}
fn source(name: &str, url: &str, category: &str) -> Value {
    json!({"name":name,"url":url,"category":category,"status":"not_run","captured_at":null,
        "request_id":null,"credits":null,"items":[],"error_code":null})
}
async fn scrape(client: &ContextClient, name: &str, url: &str, category: &str) -> Value {
    let mut out = source(name, url, category);
    let body = json!({"url":url,"formats":{"markdown":true},"maxAgeMs":900000,
        "timeoutOpts":{"milliseconds":20000,"behavior":"return-partial"}});
    match client.request(Endpoint::Scrape, &body, 1.).await {
        Ok(v) => {
            let ok = v.pointer("/markdown/success").and_then(Value::as_bool) == Some(true);
            let partial = v["isPartial"] == true
                || v.pointer("/markdown/isTruncated") == Some(&Value::Bool(true));
            out["status"] = json!(if !ok {
                "failed"
            } else if partial {
                "partial"
            } else {
                "ok"
            });
            out["captured_at"] = json!(Utc::now());
            out["request_id"] = v["request_id"].clone();
            out["credits"] = v
                .pointer("/key_metadata/credits_consumed")
                .cloned()
                .unwrap_or(Value::Null);
            out["cache_age_ms"] = v
                .pointer("/cache_metadata/age_ms")
                .cloned()
                .unwrap_or(Value::Null);
            out["published_at"] = v
                .pointer("/metadata/publishedTime")
                .cloned()
                .unwrap_or(Value::Null);
            out["error_code"] = v
                .pointer("/markdown/error_code")
                .cloned()
                .unwrap_or(Value::Null);
            // Short verbatim body excerpts only. Exclude navigation/link lists.
            // No model summary or
            // inferred date, warning, attendance or numeric claim is generated.
            if ok && let Some(md) = v.pointer("/markdown/data").and_then(Value::as_str) {
                out["items"] = json!(
                    md.lines()
                        .map(str::trim)
                        .filter(|l| l.len() > 90
                            && !l.starts_with(['[', '!', '#', '-', '|', '*'])
                            && !l.contains("Ir para navegação")
                            && !l.contains("Aceitar cookies"))
                        .take(4)
                        .map(|line| json!({"quote":clipped(line,180),"url":url}))
                        .collect::<Vec<_>>()
                );
            }
        }
        Err(e) => {
            out["status"] = json!("failed");
            out["error_code"] = json!(e.code);
        }
    }
    out
}
async fn news(client: &ContextClient, domain: &str) -> Vec<Value> {
    let mut discovery = source(
        &format!("{} discovery", domain),
        &format!("https://{domain}"),
        "news_search",
    );
    let target = if domain == "rtp.pt" {
        "site:rtp.pt/noticias -site:rtp.pt/play"
    } else {
        "site:publico.pt -inurl:opiniao"
    };
    let query = format!(
        "{target} Portugal vento chuva tempestade proteção civil after:{}",
        (Utc::now() - chrono::Duration::days(3)).format("%Y-%m-%d")
    );
    let body = json!({"query":query,"numResults":3,"includeDomains":[domain],"languages":["pt"],"country":"pt"});
    let mut sources = Vec::new();
    match client.request(Endpoint::Search, &body, 1.).await {
        Ok(v) => {
            discovery["status"] = json!("ok");
            discovery["captured_at"] = json!(Utc::now());
            discovery["request_id"] = v["request_id"].clone();
            discovery["credits"] = v
                .pointer("/key_metadata/credits_consumed")
                .cloned()
                .unwrap_or(Value::Null);
            let rows=v["results"].as_array().into_iter().flatten().filter_map(|r|{
                let url=r["url"].as_str()?;let parsed=reqwest::Url::parse(url).ok()?;
                let host=parsed.host_str()?;
                (parsed.scheme()=="https"&&parsed.username().is_empty()&&parsed.password().is_none()&&parsed.port().is_none()
                    &&(host==domain||host.ends_with(&format!(".{domain}")))).then(||json!({"title":r["title"],"url":url,"published_at":null,"basis":"search discovery; publication not yet verified"}))
            }).collect::<Vec<_>>();
            discovery["items"] = json!(rows);
            // Read two discovered pages per publisher, without retrying billed POSTs.
            for row in rows.iter().take(2) {
                let mut page =
                    scrape(client, domain, row["url"].as_str().unwrap(), "news_page").await;
                page["headline"] = row["title"].clone();
                sources.push(page);
            }
        }
        Err(e) => {
            discovery["status"] = json!("failed");
            discovery["error_code"] = json!(e.code);
        }
    }
    sources.insert(0, discovery);
    sources
}

// Geographic coverage samples, not station measurements or surveyed locations.
const POINTS: [(&str, f64, f64); 9] = [
    ("Braga", 41.55, -8.42),
    ("Bragança", 41.81, -6.76),
    ("Porto", 41.15, -8.61),
    ("Viseu", 40.66, -7.91),
    ("Coimbra", 40.21, -8.43),
    ("Castelo Branco", 39.82, -7.49),
    ("Lisbon", 38.72, -9.14),
    ("Évora", 38.57, -7.91),
    ("Faro", 37.02, -7.93),
];
fn weather_rows(raw: &Value) -> Vec<Value> {
    raw.as_array().into_iter().flatten().zip(POINTS).map(|(v,(name,lat,lon))|{
        let units=&v["current_units"];
        let compatible=units["wind_speed_10m"]=="km/h"&&units["wind_gusts_10m"]=="km/h"&&v["utc_offset_seconds"]==0;
        let mut current=v["current"].clone();
        if !compatible {current=Value::Null;}
        let times=v["hourly"]["time"].as_array();
        let forecast=times.into_iter().flatten().enumerate().take(12).map(|(i,t)|json!({"time":t,
            "wind_kmh":v["hourly"]["wind_speed_10m"][i],"gust_kmh":v["hourly"]["wind_gusts_10m"][i],
            "direction_deg":v["hourly"]["wind_direction_10m"][i]})).collect::<Vec<_>>();
        json!({"name":name,"requested_position":[lon,lat],"model_position":[v["longitude"],v["latitude"]],
            "status":if current.is_object(){"ok"}else{"partial"},"current":current,"forecast":forecast,
            "units":units,"basis":"Open-Meteo weather model; not a direct station measurement","time_zone":"UTC"})
    }).collect()
}
async fn weather() -> Value {
    let mut out = source(
        "Open-Meteo national weather and wind",
        "https://open-meteo.com/",
        "weather",
    );
    let lat = POINTS
        .iter()
        .map(|p| p.1.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let lon = POINTS
        .iter()
        .map(|p| p.2.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,relative_humidity_2m,precipitation,wind_speed_10m,wind_direction_10m,wind_gusts_10m&hourly=wind_speed_10m,wind_direction_10m,wind_gusts_10m&forecast_hours=12&wind_speed_unit=kmh&timezone=GMT"
    );
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
    {
        Ok(c) => c,
        Err(_) => return out,
    };
    match client.get(&url).send().await {
        Ok(response) if response.status().is_success() => {
            out["url"] = json!(url);
            match response.json::<Value>().await {
                Ok(raw) => {
                    let rows = weather_rows(&raw);
                    out["status"] = json!(if rows.len() == 9
                        && rows.iter().all(|r| r["status"] == "ok")
                    {
                        "ok"
                    } else {
                        "partial"
                    });
                    out["items"] = json!(rows);
                    out["captured_at"] = json!(Utc::now());
                }
                Err(_) => {
                    out["status"] = json!("failed");
                    out["error_code"] = json!("WEATHER_PARSE");
                }
            }
        }
        _ => {
            out["status"] = json!("failed");
            out["error_code"] = json!("WEATHER_UNAVAILABLE");
        }
    }
    out["attribution"] = json!("Weather data by Open-Meteo.com · CC BY 4.0");
    out
}
async fn status(State(cache): State<Arc<Mutex<Cache>>>) -> Json<Value> {
    let mut cache = cache.lock().await;
    if cache
        .at
        .is_some_and(|t| t.elapsed() < Duration::from_secs(900))
        && let Some(v) = &cache.value
    {
        return Json(v.clone());
    }
    let enabled = std::env::var("EMBERWATCH_PORTUGAL_DATA_ENABLED").as_deref() == Ok("1");
    if !enabled {
        return Json(
            json!({"mode":"live","checked_at":Utc::now(),"status":"disabled","sources":[],"note":"Set EMBERWATCH_PORTUGAL_DATA_ENABLED=1 on Rust to enable national collection."}),
        );
    }
    if cache.budget_at.elapsed() >= Duration::from_secs(3600)
        && cache
            .context
            .as_ref()
            .is_none_or(|c| !c.budget_status().outcome_unknown)
    {
        cache.context = None;
        cache.budget_at = Instant::now();
    }
    if cache.context.is_none() {
        cache.context = ContextClient::from_env()
            .and_then(|c| c.with_budget(20.))
            .ok();
    }
    let mut sources = vec![weather().await];
    if let Some(client) = &cache.context {
        for domain in ["rtp.pt", "publico.pt"] {
            sources.extend(news(client, domain).await);
        }
        sources.push(
            scrape(
                client,
                "IPMA public weather warnings",
                "https://www.ipma.pt/en/otempo/prev-sam/",
                "weather_page",
            )
            .await,
        );
    } else {
        let mut missing = source(
            "Context.dev news and public weather pages",
            "https://www.context.dev",
            "news_search",
        );
        missing["status"] = json!("failed");
        missing["error_code"] = json!("CONTEXT_UNCONFIGURED");
        sources.push(missing);
    }
    let value = json!({"mode":"live","checked_at":Utc::now(),"status":if sources.iter().all(|s|s["status"]=="ok"){"ok"}else{"partial"},
        "sources":sources,"context_budget":cache.context.as_ref().map(ContextClient::budget_status),"cache_seconds":900,
        "note":"National background monitoring, even without fires. Source text/headlines remain in the publisher's original language. News discovery is not proof of incident activity; weather is model data. No provider monitor subscription is created."});
    cache.at = Some(Instant::now());
    cache.value = Some(value.clone());
    Json(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn weather_units_and_nulls_are_not_silently_invented() {
        let rows = weather_rows(
            &json!([{"utc_offset_seconds":0,"current_units":{"wind_speed_10m":"m/s","wind_gusts_10m":"m/s"},"current":{"wind_speed_10m":5}}]),
        );
        assert_eq!(rows[0]["status"], "partial");
        assert!(rows[0]["current"].is_null());
        assert_eq!(rows[0]["requested_position"], json!([-8.42, 41.55]));
    }
}
