//! Explicitly billed, operator-run Context capture. Never called by CI or the scheduler.
use emberwatch_intelligence::context::{ContextClient, Endpoint, validate_fogos_url};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Instant};

fn public_url(raw: &str) -> Result<(), Box<dyn std::error::Error>> {
    let url = reqwest::Url::parse(raw)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err("Only public HTTPS URLs without credentials or custom ports".into());
    }
    let host = url.host_str().ok_or("Missing host")?;
    if host.ends_with("fogos.pt") {
        validate_fogos_url(raw)?;
    }
    if host.parse::<std::net::IpAddr>().is_ok()
        || !host.contains('.')
        || [".localhost", ".local", ".internal"]
            .iter()
            .any(|s| host.ends_with(s))
    {
        return Err("Private/IP scrape targets are excluded".into());
    }
    Ok(())
}
fn schema() -> Value {
    let mut properties = serde_json::Map::new();
    for name in ["category", "statement_en", "quote", "time_text", "unit"] {
        properties.insert(name.into(), json!({"type":["string","null"]}));
    }
    properties.insert("number".into(), json!({"type":["number","null"]}));
    properties.insert(
        "places".into(),
        json!({"type":"array","items":{"type":"string"}}),
    );
    json!({"type":"object","properties":{
   "published_text":{"type":["string","null"]},"updated_text":{"type":["string","null"]},"headline":{"type":["string","null"]},
   "facts":{"type":"array","maxItems":20,"items":{"type":"object","properties":properties,"additionalProperties":false}}
 },"additionalProperties":false})
}
fn scrape_body(url: &str) -> Value {
    json!({"url":url,"formats":{"markdown":true,"json":true},"maxAgeMs":0,
 "sharedParams":{"waitFor":3000},"timeoutOpts":{"milliseconds":30000,"behavior":"return-partial"},
 "jsonParams":{"schema":schema(),"instructions":"Extract only facts printed in the main article about the wildfire, nearby people, events, roads and weather. Copy dates and quotes verbatim in Portuguese; missing values are null. Each fact must carry its own contiguous quote, at most 300 characters, supporting every place, number, time and category. Preserve approximate/lower-bound wording. Do not infer coordinates or attendance. Ignore navigation and unrelated stories. Do not describe a historical article as current activity."}})
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "archive") {
        let raw = std::path::Path::new(args.get(1).ok_or(
            "archive <original raw directory> <absolute private destination> <pages.json>",
        )?);
        let private = std::path::Path::new(args.get(2).ok_or("Missing private destination")?);
        let pages = std::path::Path::new(args.get(3).ok_or("Missing retained pages.json path")?);
        println!(
            "{}",
            serde_json::to_string(&emberwatch_intelligence::capture_archive::archive(
                raw, private, pages
            )?)?
        );
        return Ok(());
    }
    let ceiling_pos=args.iter().position(|a|a=="--ceiling").ok_or("Required: --ceiling <maximum credits>. Commands: fogos <url>, scrape <url>, batch <urls.json>, search <municipality>, news <query> <from> <to>, answers <task>")?;
    let ceiling: f64 = args
        .get(ceiling_pos + 1)
        .ok_or("Missing ceiling")?
        .parse()?;
    args.drain(ceiling_pos..=ceiling_pos + 1);
    let command = args.first().ok_or("Missing command")?.as_str();
    let client = ContextClient::from_env()?.with_budget(ceiling)?;
    let target = args.get(1).ok_or("Missing target/query")?;
    let mut jobs = Vec::new();
    match command {
  "fogos"=>{validate_fogos_url(target)?;let mut body=scrape_body(target);body["jsonParams"]["schema"]=emberwatch_intelligence::context::fogos_schema();body["jsonParams"]["instructions"]=json!("Read public incident rows only with exact supporting excerpts. Missing fields null. Never infer national completeness. Copy explicit empty text if present. No rows in a loading shell means coverage unknown. On a historical detail page retain only dated status rows and static nature/coordinates; exclude present-day weather/resource counters.");jobs.push((Endpoint::Scrape,body,5.0));}
  "scrape"=>{public_url(target)?;jobs.push((Endpoint::Scrape,scrape_body(target),5.0));}
  "batch"=>{let urls:Vec<String>=serde_json::from_slice(&std::fs::read(target)?)?;for url in urls{public_url(&url)?;jobs.push((Endpoint::Scrape,scrape_body(&url),5.0));}}
  "capture-plan"=>{
   let plan:Value=serde_json::from_slice(&std::fs::read(target)?)?;
   for search in plan["searches"].as_array().ok_or("Missing searches")? {
    if search["endpoint"]!="POST /v1/web/search" {println!("skipped news-index query: Portuguese coverage not established");continue;}
    let body=search["body"].clone();let n=body["numResults"].as_u64().ok_or("Search needs explicit numResults")?;
    if n==0||n>100{return Err("Search result limit outside reviewed range".into());}
    jobs.push((Endpoint::Search,body,(n as f64/10.).ceil()));
   }
   for url in plan["scrape_urls"].as_array().ok_or("Missing scrape_urls")? {
    let url=url.as_str().ok_or("Invalid scrape URL")?;public_url(url)?;
    jobs.push((Endpoint::Scrape,scrape_body(url),5.0));
   }
  }
  "search"|"news"=>{
   let query=if command=="news" {
    let from=chrono::NaiveDate::parse_from_str(args.get(2).ok_or("Missing from YYYY-MM-DD")?,"%Y-%m-%d")?;
    let to=chrono::NaiveDate::parse_from_str(args.get(3).ok_or("Missing to YYYY-MM-DD")?,"%Y-%m-%d")?;
    if from>to{return Err("Inverted date window".into());}
    format!("{target} after:{from} before:{}",to.succ_opt().ok_or("Date overflow")?)
   } else {format!("incêndio rural {target} Portugal")};
   jobs.push((Endpoint::Search,json!({"query":query,"numResults":5,"languages":["pt"],"country":"pt","includeDomains":["rtp.pt","rr.pt","observador.pt","sicnoticias.pt","prociv.gov.pt"]}),1.0));
  }
  "answers"=>jobs.push((Endpoint::Answers,json!({"task":target,"mode":"fast","json_format":{"answer":"","quotes":[{"url":"","quote":""}]},"timeoutOpts":{"milliseconds":30000,"behavior":"return-partial"}}),10.0)),
  _=>return Err("Unknown command".into()),
 }
    let directory = PathBuf::from("crates/intelligence/tests/fixtures/live");
    std::fs::create_dir_all(&directory)?;
    for (endpoint, body, reservation) in jobs {
        let budget = client.budget_status();
        if budget.outcome_unknown || budget.committed + reservation > budget.limit {
            println!("stopped: credit ceiling or uncertain prior outcome");
            break;
        }
        let started = chrono::Utc::now();
        let clock = Instant::now();
        let raw = match client.request(endpoint, &body, reservation).await {
            Ok(raw) => raw,
            Err(error) => {
                let failure = json!({"request_id":null,"credits":null,"error_code":error.code,"latency_ms":clock.elapsed().as_millis(),"outcome_unknown":client.budget_status().outcome_unknown,"capture_started_at":started});
                let file = directory.join(format!(
                    "{}-{command}-failed.json",
                    started.format("%Y%m%dT%H%M%S%3f")
                ));
                std::fs::write(file, serde_json::to_vec_pretty(&failure)?)?;
                println!("{failure}");
                return Err(error.into());
            }
        };
        let mut capture = client.redacted_capture(&raw);
        capture["capture_started_at"] = json!(started);
        capture["captured_at"] = json!(chrono::Utc::now());
        capture["_captured_at"] = capture["captured_at"].clone();
        capture["_requested_url"] = body.get("url").cloned().unwrap_or(Value::Null);
        capture["latency_ms"] = json!(clock.elapsed().as_millis());
        let digest = hex::encode(Sha256::digest(body.to_string().as_bytes()));
        let file = directory.join(format!(
            "{}-{command}-{}.json",
            started.format("%Y%m%dT%H%M%S%3f"),
            &digest[..12]
        ));
        std::fs::write(&file, serde_json::to_vec_pretty(&capture)?)?;
        println!(
            "{}",
            json!({"request_id":raw["request_id"],"credits":raw.pointer("/key_metadata/credits_consumed"),"latency_ms":clock.elapsed().as_millis(),"cache_age_ms":raw.pointer("/cache_metadata/age_ms"),"markdown_success":raw.pointer("/markdown/success"),"json_success":raw.pointer("/json/success"),"partial":raw.get("isPartial").or(raw.get("partial")),"json_truncated":raw.pointer("/json/isTruncated"),"capture":file})
        );
    }
    if client.budget_status().outcome_unknown {
        return Err(
            "Context usage/outcome is unknown; inspect usage before another billed probe.".into(),
        );
    }
    Ok(())
}
