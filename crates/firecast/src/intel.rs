//! Context.dev live-web intelligence about the fire environment.
//!
//! Weather APIs describe the atmosphere; they do not say which way reporters on the
//! ground see the fire running, what is burning, how dry the region is, or which
//! roads and villages lie ahead. Context.dev supplies those from the live web:
//!
//! 1. `POST /web/search` – Portuguese articles with an explicit date window.
//! 2. `POST /web/search` – fresh official/local pages (live runs only).
//! 3. `POST /web/scrape` + JSON schema – structured fire-behaviour facts per article,
//!    each with a verbatim excerpt.
//! 4. `POST /web/answers` – a cited answer on regional conditions (dominant
//!    vegetation, drought class, local wind regime, active warnings, burn scars).
//!
//! Extracted facts change model inputs only through the deterministic rules in
//! [`derive_adjustments`]; every adjustment keeps its source URL.
use crate::FirecastError;
use crate::fuel::{LandCover, land_cover_from_text};
use crate::geo::parse_compass;
use crate::moisture::drought_reduction;
use crate::sources::truncate;
use chrono::{DateTime, Utc};
use emberwatch_intelligence::{
    accuracy::{normalize, number_supported, quote_verified},
    context::{ContextClient, Endpoint},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Domains Context.dev searches and scrapes. Scrape targets outside this list are refused.
pub const PT_DOMAINS: [&str; 18] = [
    "prociv.gov.pt",
    "icnf.pt",
    "ipma.pt",
    "agif.pt",
    "rtp.pt",
    "sicnoticias.pt",
    "cnnportugal.iol.pt",
    "tvi.iol.pt",
    "publico.pt",
    "jn.pt",
    "dn.pt",
    "observador.pt",
    "expresso.pt",
    "tsf.pt",
    "noticiasaominuto.com",
    "cmjornal.pt",
    "sapo.pt",
    "lusa.pt",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntelMode {
    /// Real incident now: news, fresh search, article extraction, regional answer.
    Live,
    /// Past incident: dated news search and article extraction only.
    Historical,
    /// Training incident at a real place: regional conditions only (no incident news).
    WhatIf,
}

type CaptureCache = std::sync::Arc<
    std::sync::Mutex<
        std::collections::BTreeMap<String, (std::time::Instant, Option<ArticleConditions>)>,
    >,
>;
#[derive(Clone)]
pub struct ContextDev {
    inner: ContextClient,
    shared_budget: bool,
    captures: CaptureCache,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArticleConditions {
    #[serde(skip_deserializing)]
    pub verified: bool,
    pub field_quotes: std::collections::BTreeMap<String, String>,
    #[serde(skip_deserializing)]
    pub unverified_fields: std::collections::BTreeMap<String, Value>,
    #[serde(skip_deserializing)]
    pub source_published_at: Option<DateTime<Utc>>,
    #[serde(skip_deserializing)]
    pub source_modified_at: Option<DateTime<Utc>>,
    #[serde(skip_deserializing)]
    pub retrieved_at: Option<DateTime<Utc>>,
    #[serde(skip_deserializing)]
    pub cache_age_ms: Option<u64>,
    #[serde(skip_deserializing)]
    pub request_id: Option<String>,
    pub reported_spread_direction: Option<String>,
    pub reported_spread_toward_place: Option<String>,
    pub wind_description: Option<String>,
    pub wind_speed_kmh: Option<f64>,
    pub wind_from_direction: Option<String>,
    pub active_fronts: Option<i64>,
    pub vegetation: Vec<String>,
    pub terrain_notes: Option<String>,
    pub threatened_places: Vec<String>,
    pub barriers_or_containment_lines: Vec<String>,
    pub evacuations: Vec<String>,
    pub aerial_means: Option<i64>,
    pub ground_personnel: Option<i64>,
    pub recently_burned_area_mentioned: Option<bool>,
    pub visible_excerpt: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Report {
    pub url: String,
    pub title: String,
    pub source: Option<String>,
    pub published_at: Option<String>,
    pub conditions: Option<ArticleConditions>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RegionalContext {
    #[serde(skip_deserializing)]
    pub verified: bool,
    pub dominant_vegetation: Vec<String>,
    pub drought_status: Option<String>,
    pub local_wind_regime: Option<String>,
    pub active_weather_warnings: Vec<String>,
    pub recent_burn_scars: Vec<String>,
    pub notes: Option<String>,
    #[serde(skip_deserializing)]
    pub sources: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct FireIntel {
    pub mode: Option<IntelMode>,
    pub reports: Vec<Report>,
    pub regional: Option<RegionalContext>,
    pub credits_used: f64,
    pub calls: Vec<String>,
    pub errors: Vec<String>,
    pub budget: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Sourced<T> {
    pub value: T,
    pub evidence: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Adjustments {
    pub regional_cover: Option<Sourced<LandCover>>,
    pub drought_live_reduction: Option<Sourced<f64>>,
    pub drought_label: Option<String>,
    pub reported_spread_bearing: Option<Sourced<f64>>,
    pub reported_wind_from_deg: Option<Sourced<f64>>,
    pub reported_wind_kmh: Option<Sourced<f64>>,
    pub threatened_places: Vec<Sourced<String>>,
    pub barriers: Vec<Sourced<String>>,
    pub evacuations: Vec<Sourced<String>>,
    pub burn_scar_mentioned: Option<Sourced<bool>>,
}

pub fn conditions_schema() -> Value {
    let s = json!({"type":["string","null"]});
    let arr = json!({"type":"array","items":{"type":"string"},"maxItems":12});
    json!({"type":"object","additionalProperties":false,"properties":{
        "reported_spread_direction":s,"reported_spread_toward_place":s,"wind_description":s,
        "wind_speed_kmh":{"type":["number","null"],"minimum":0,"maximum":250},"wind_from_direction":s,
        "active_fronts":{"type":["integer","null"],"minimum":0},"vegetation":arr,"terrain_notes":s,
        "threatened_places":arr,"barriers_or_containment_lines":arr,"evacuations":arr,
        "aerial_means":{"type":["integer","null"],"minimum":0},"ground_personnel":{"type":["integer","null"],"minimum":0},
        "recently_burned_area_mentioned":{"type":["boolean","null"]},"visible_excerpt":s,
        "field_quotes":{"type":"object","additionalProperties":{"type":"string"}}},
        "required":["vegetation","threatened_places","barriers_or_containment_lines","evacuations"]})
}

const EXTRACTION_RULES: &str = "Extract only facts stated in this article about the named wildfire. \
Missing facts are null or empty. Keep Portuguese wording. Directions as written (e.g. 'para norte', 'vento de nordeste'). \
Do not infer coordinates, times or numbers. visible_excerpt must be a verbatim sentence supporting the direction or wind facts. \
For each non-null field, put its own exact supporting quote in field_quotes under that field name.";

impl ContextDev {
    pub fn new(api_key: String) -> Result<Self, FirecastError> {
        if api_key.trim().is_empty() {
            return Err(FirecastError::Config("CONTEXT_DEV_API_KEY is empty".into()));
        }
        Ok(Self {
            inner: ContextClient::new(api_key).map_err(|e| FirecastError::Config(e.code))?,
            shared_budget: false,
            captures: Default::default(),
        })
    }
    pub fn from_env() -> Option<Self> {
        std::env::var("CONTEXT_DEV_API_KEY")
            .ok()
            .and_then(|k| Self::new(k).ok())
    }
    pub fn with_shared_budget(&self, limit: f64) -> Result<Self, FirecastError> {
        Ok(Self {
            inner: self
                .inner
                .clone()
                .with_budget(limit)
                .map_err(|e| FirecastError::Config(e.code))?,
            shared_budget: true,
            captures: self.captures.clone(),
        })
    }
    pub fn budget_status(&self) -> emberwatch_intelligence::context::BudgetStatus {
        self.inner.budget_status()
    }

    async fn post(&self, path: &str, body: &Value) -> Result<(Value, f64), FirecastError> {
        let (endpoint, ceiling) = match path {
            "web/scrape" => (Endpoint::Scrape, 5.0),
            "web/search" => (Endpoint::Search, 1.0),
            "news/search" => (Endpoint::News, 1.0),
            "web/answers" => (Endpoint::Answers, 10.0),
            _ => return Err(FirecastError::Config("Unreviewed Context endpoint".into())),
        };
        let value = self
            .inner
            .request(endpoint, body, ceiling)
            .await
            .map_err(|e| FirecastError::Upstream(e.code))?;
        let credits = value
            .pointer("/key_metadata/credits_consumed")
            .and_then(Value::as_f64)
            .ok_or_else(|| {
                FirecastError::Upstream("CONTEXT_USAGE_UNKNOWN; further calls stopped".into())
            })?;
        Ok((value, credits))
    }

    pub async fn news(
        &self,
        query: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<(Vec<Report>, f64), FirecastError> {
        // The provider's news index does not establish Portuguese publisher coverage.
        let query = format!(
            "{query} after:{} before:{}",
            from.format("%Y-%m-%d"),
            (to + chrono::Duration::days(1)).format("%Y-%m-%d")
        );
        self.web_search(&query, false).await
    }

    pub async fn search(&self, query: &str) -> Result<(Vec<Report>, f64), FirecastError> {
        self.web_search(query, true).await
    }
    async fn web_search(
        &self,
        query: &str,
        live: bool,
    ) -> Result<(Vec<Report>, f64), FirecastError> {
        let mut body = json!({"query":query,"numResults":6,"includeDomains":PT_DOMAINS,"languages":["pt"],"country":"pt"});
        if live {
            body["freshness"] = json!("last_24_hours");
        }
        let (value, credits) = self.post("web/search", &body).await?;
        let reports = value
            .get("results")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| {
                        Some(Report {
                            url: r.get("url")?.as_str()?.to_owned(),
                            title: truncate(
                                r.get("title").and_then(Value::as_str).unwrap_or(""),
                                200,
                            ),
                            source: None,
                            published_at: None,
                            conditions: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok((reports, credits))
    }

    pub async fn extract_conditions(
        &self,
        url: &str,
        incident_hint: &str,
    ) -> Result<(Option<ArticleConditions>, f64), FirecastError> {
        validate_scrape_url(url)?;
        let cache_key = json!([url, incident_hint]).to_string();
        {
            let mut cache = self.captures.lock().unwrap();
            cache.retain(|_, (at, _)| at.elapsed() < std::time::Duration::from_secs(3600));
            if let Some((_, conditions)) = cache.get(&cache_key) {
                return Ok((conditions.clone(), 0.));
            }
        }
        let body = json!({"url":url,"formats":{"markdown":true,"json":true},"maxAgeMs":3_600_000,
            "timeoutOpts":{"milliseconds":30000,"behavior":"return-partial"},
            "jsonParams":{"schema":conditions_schema(),"instructions":format!("{EXTRACTION_RULES} Wildfire: {incident_hint}.")}});
        let (value, credits) = self.post("web/scrape", &body).await?;
        let ok = value.pointer("/json/success").and_then(Value::as_bool) == Some(true);
        let ready = ok
            && value.pointer("/markdown/success").and_then(Value::as_bool) == Some(true)
            && value.get("isPartial").and_then(Value::as_bool) != Some(true)
            && value.pointer("/json/isTruncated").and_then(Value::as_bool) != Some(true);
        let data = value.pointer("/json/data").cloned().filter(|_| ready);
        let markdown = value
            .pointer("/markdown/data")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let conditions = data.and_then(|mut data| {
            let mut unverified=std::collections::BTreeMap::new();
            let quotes = data.get("field_quotes").cloned().unwrap_or(json!({}));
            let fallback = data
                .get("visible_excerpt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if let Some(fields) = data.as_object_mut() {
                for (name, value) in fields.iter_mut() {
                    if matches!(
                        name.as_str(),
                        "field_quotes" | "visible_excerpt" | "verified"
                    ) {
                        continue;
                    }
                    let quote = quotes
                        .get(name)
                        .and_then(Value::as_str)
                        .unwrap_or(&fallback);
                    let supported = |v: &Value| {
                        quote_verified(markdown, quote)
                            && !(matches!(name.as_str(),"vegetation"|"evacuations"|"threatened_places"|"barriers_or_containment_lines")
                                && normalize(quote).to_lowercase().split(|c:char|!c.is_alphabetic()).any(|w|matches!(w,"não"|"sem"|"nunca"|"nenhuma"|"nenhum"|"ninguém")))
                            && match v {
                                Value::String(s) => {
                                    !s.is_empty() && normalize(quote).contains(&normalize(s))
                                }
                                Value::Number(n) => {
                                    n.as_f64().is_some_and(|n| number_supported(n, quote))
                                }
                                _ => false,
                            }
                    };
                    let before=value.clone();
                    if let Value::Array(values) = value {
                        values.retain(supported);
                    } else if !value.is_null() && !supported(value) {
                        *value = Value::Null;
                    }
                    if before!=*value {unverified.insert(name.clone(),json!({"value":before,"quote":quote,"reason":"missing_or_unsupported_literal_evidence"}));}
                }
            }
            let mut c: ArticleConditions = serde_json::from_value(data).ok()?;
            c.verified = true;
            c.unverified_fields=unverified;
            c.visible_excerpt=c.visible_excerpt.filter(|q|quote_verified(markdown,q));
            let source_time=|key:&str|value.pointer(key).and_then(Value::as_str).and_then(|s|DateTime::parse_from_rfc3339(s).ok()).map(|t|t.with_timezone(&Utc));
            c.source_published_at=source_time("/metadata/publishedTime");
            c.source_modified_at=source_time("/metadata/modifiedTime");
            c.retrieved_at=Some(Utc::now());
            c.cache_age_ms=value.pointer("/cache_metadata/age_ms").and_then(Value::as_u64);
            c.request_id=value.get("request_id").and_then(Value::as_str).map(str::to_owned);
            Some(c)
        });
        {
            let mut cache = self.captures.lock().unwrap();
            if cache.len() < 128 {
                cache.insert(cache_key, (std::time::Instant::now(), conditions.clone()));
            }
        }
        Ok((conditions, credits))
    }

    pub async fn regional_context(
        &self,
        place: &str,
    ) -> Result<(Option<RegionalContext>, f64), FirecastError> {
        let task = format!(
            "Para a zona de {place}, Portugal, hoje: que vegetação domina (eucalipto, pinheiro-bravo, mato, agrícola)? \
Qual a classe de seca meteorológica segundo o IPMA? Há avisos meteorológicos ativos (vento, tempo quente)? \
Qual o regime de vento local típico (ex. nortada, levante)? Houve grandes incêndios recentes que deixaram áreas ardidas próximas? \
Responda só com factos suportados por fontes."
        );
        let format = json!({"dominant_vegetation":["eucalipto"],"drought_status":"seca moderada","local_wind_regime":"nortada",
            "active_weather_warnings":["aviso amarelo de tempo quente"],"recent_burn_scars":["incêndio de 2017 em ..."],"notes":"..."});
        let body = json!({"task":task,"mode":"fast","json_format":format,"timeoutOpts":{"milliseconds":30000,"behavior":"return-partial"}});
        let (value, credits) = self.post("web/answers", &body).await?;
        let mut regional: Option<RegionalContext> = value
            .get("json_content")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok());
        if let Some(r) = regional.as_mut() {
            r.sources = value
                .get("sources")
                .and_then(Value::as_array)
                .map(|s| {
                    s.iter()
                        .filter_map(|v| {
                            v.as_str()
                                .map(str::to_owned)
                                .or_else(|| v.get("url").and_then(Value::as_str).map(str::to_owned))
                        })
                        .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
                        .take(12)
                        .collect()
                })
                .unwrap_or_default();
        }
        Ok((regional, credits))
    }

    /// Gather intelligence within a credit budget. Never fails: errors are recorded.
    pub async fn gather(
        &self,
        mode: IntelMode,
        place: &str,
        at: DateTime<Utc>,
        max_credits: f64,
    ) -> FireIntel {
        let mut intel = FireIntel {
            mode: Some(mode),
            ..FireIntel::default()
        };
        let scoped = if self.shared_budget {
            Ok(self.inner.clone())
        } else {
            self.inner.clone().with_budget(max_credits)
        };
        let Ok(inner) = scoped else {
            intel.errors.push("Invalid credit budget".into());
            return intel;
        };
        let bounded = Self {
            inner,
            shared_budget: self.shared_budget,
            captures: self.captures.clone(),
        };
        let afford = |intel: &FireIntel, cost: f64| intel.credits_used + cost <= max_credits;
        let query = format!("incêndio {place}");
        if mode != IntelMode::WhatIf && afford(&intel, 1.0) {
            let window = if mode == IntelMode::Live {
                chrono::Duration::hours(24)
            } else {
                chrono::Duration::hours(36)
            };
            intel.calls.push("web/search".into());
            match bounded.news(&query, at - window, at).await {
                Ok((reports, c)) => {
                    intel.credits_used += c;
                    intel.reports.extend(reports);
                }
                Err(e) => intel.errors.push(e.to_string()),
            }
        }
        if mode == IntelMode::Live && afford(&intel, 1.0) {
            intel.calls.push("web/search".into());
            match bounded
                .search(&format!("{query} vento frente progride"))
                .await
            {
                Ok((reports, c)) => {
                    intel.credits_used += c;
                    for r in reports {
                        if !intel.reports.iter().any(|x| x.url == r.url) {
                            intel.reports.push(r);
                        }
                    }
                }
                Err(e) => intel.errors.push(e.to_string()),
            }
        }
        let mut extracted = 0;
        for i in 0..intel.reports.len() {
            if extracted >= 2 || !afford(&intel, 5.0) {
                break;
            }
            let url = intel.reports[i].url.clone();
            if validate_scrape_url(&url).is_err() {
                continue;
            }
            intel.calls.push("web/scrape".into());
            match bounded.extract_conditions(&url, &query).await {
                Ok((conditions, c)) => {
                    intel.credits_used += c;
                    // Today's article capture cannot prove historical publication availability.
                    if mode == IntelMode::Live {
                        let eligible = conditions.as_ref().is_some_and(|c| {
                            c.source_published_at.is_some_and(|published| {
                                published <= at && at - published <= chrono::Duration::hours(24)
                            }) && c.source_modified_at.is_none_or(|modified| modified <= at)
                        });
                        if eligible {
                            intel.reports[i].conditions = conditions;
                        } else {
                            intel.errors.push("Live conditions withheld: publication time missing, stale or in the future".into());
                        }
                    } else {
                        intel.errors.push(
                            "Historical conditions withheld pending capture-time validation".into(),
                        );
                    }
                    extracted += 1;
                }
                Err(e) => intel.errors.push(e.to_string()),
            }
        }
        if mode != IntelMode::Historical && afford(&intel, 10.0) {
            intel.calls.push("web/answers".into());
            match bounded.regional_context(place).await {
                Ok((regional, c)) => {
                    intel.credits_used += c;
                    intel.regional = regional;
                }
                Err(e) => intel.errors.push(e.to_string()),
            }
        }
        intel.budget = Some(serde_json::to_value(bounded.budget_status()).expect("finite budget"));
        intel
    }
}

pub fn parse_news(value: &Value) -> Vec<Report> {
    value
        .get("data")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|r| {
                    Some(Report {
                        url: r.get("url")?.as_str()?.to_owned(),
                        title: truncate(r.get("title").and_then(Value::as_str).unwrap_or(""), 200),
                        source: r
                            .pointer("/source/name")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        published_at: r
                            .get("published_at")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        conditions: None,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn validate_scrape_url(url: &str) -> Result<(), FirecastError> {
    let parsed =
        reqwest::Url::parse(url).map_err(|_| FirecastError::Config("invalid scrape URL".into()))?;
    let host = parsed.host_str().unwrap_or("");
    let allowed = PT_DOMAINS
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")));
    if parsed.scheme() != "https"
        || !allowed
        || parsed.port().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(FirecastError::Config(
            "scrape URL outside the reviewed Portuguese news/official domains".into(),
        ));
    }
    Ok(())
}

/// Deterministic mapping from extracted facts to model inputs.
pub fn derive_adjustments(intel: &FireIntel) -> Adjustments {
    let mut adj = Adjustments::default();
    let mut votes: std::collections::BTreeMap<LandCover, (usize, String, String)> =
        Default::default();
    let mut vote = |text: &str, url: &str| {
        if let Some(c) = land_cover_from_text(text) {
            let e = votes
                .entry(c)
                .or_insert((0, text.to_owned(), url.to_owned()));
            e.0 += 1;
        }
    };
    for report in &intel.reports {
        let Some(c) = &report.conditions else {
            continue;
        };
        if !c.verified {
            continue;
        }
        let quote_for = |field: &str| {
            c.field_quotes
                .get(field)
                .or(c.visible_excerpt.as_ref())
                .cloned()
                .unwrap_or_default()
        };
        let url = report.url.clone();
        c.vegetation.iter().for_each(|v| vote(v, &url));
        if adj.reported_spread_bearing.is_none()
            && let Some(text) = &c.reported_spread_direction
            && let Some(b) = parse_compass(text)
        {
            adj.reported_spread_bearing = Some(Sourced {
                value: b,
                evidence: quote_for("reported_spread_direction"),
                url: url.clone(),
            });
        }
        if adj.reported_wind_from_deg.is_none()
            && let Some(b) = c
                .wind_from_direction
                .as_deref()
                .or(c.wind_description.as_deref())
                .and_then(parse_compass)
        {
            adj.reported_wind_from_deg = Some(Sourced {
                value: b,
                evidence: quote_for(if c.wind_from_direction.is_some() {
                    "wind_from_direction"
                } else {
                    "wind_description"
                }),
                url: url.clone(),
            });
        }
        if adj.reported_wind_kmh.is_none()
            && let Some(k) = c.wind_speed_kmh.filter(|k| (0.0..=250.0).contains(k))
        {
            adj.reported_wind_kmh = Some(Sourced {
                value: k,
                evidence: quote_for("wind_speed_kmh"),
                url: url.clone(),
            });
        }
        for p in &c.threatened_places {
            adj.threatened_places.push(Sourced {
                value: truncate(p, 80),
                evidence: quote_for("threatened_places"),
                url: url.clone(),
            });
        }
        for b in &c.barriers_or_containment_lines {
            adj.barriers.push(Sourced {
                value: truncate(b, 80),
                evidence: quote_for("barriers_or_containment_lines"),
                url: url.clone(),
            });
        }
        for e in &c.evacuations {
            adj.evacuations.push(Sourced {
                value: truncate(e, 80),
                evidence: quote_for("evacuations"),
                url: url.clone(),
            });
        }
        if c.recently_burned_area_mentioned == Some(true) && adj.burn_scar_mentioned.is_none() {
            adj.burn_scar_mentioned = Some(Sourced {
                value: true,
                evidence: String::new(),
                url: url.clone(),
            });
        }
    }
    if let Some(regional) = intel.regional.as_ref().filter(|r| r.verified) {
        let url = regional
            .sources
            .first()
            .cloned()
            .unwrap_or_else(|| "https://api.context.dev/v1/web/answers".into());
        regional
            .dominant_vegetation
            .iter()
            .for_each(|v| vote(v, &url));
        if let Some(status) = &regional.drought_status
            && let Some((reduction, label)) = drought_reduction(status)
        {
            adj.drought_live_reduction = Some(Sourced {
                value: reduction,
                evidence: status.clone(),
                url: url.clone(),
            });
            adj.drought_label = Some(label.into());
        }
    }
    if let Some((cover, (_, text, url))) = votes.into_iter().max_by_key(|(_, (n, ..))| *n) {
        adj.regional_cover = Some(Sourced {
            value: cover,
            evidence: text,
            url,
        });
    }
    adj.threatened_places.dedup_by(|a, b| a.value == b.value);
    adj.barriers.dedup_by(|a, b| a.value == b.value);
    adj
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intel() -> FireIntel {
        FireIntel {
            mode: Some(IntelMode::Live),
            reports: vec![Report {
                url: "https://www.rtp.pt/noticias/pais/incendio".into(),
                title: "Incêndio em Sever do Vouga".into(),
                source: Some("RTP".into()),
                published_at: None,
                conditions: Some(ArticleConditions {
                    verified: true,
                    reported_spread_direction: Some("a progredir para nordeste".into()),
                    wind_description: Some("vento forte de sudoeste".into()),
                    wind_speed_kmh: Some(35.0),
                    vegetation: vec!["eucaliptal".into(), "pinhal".into()],
                    threatened_places: vec!["Talhadas".into()],
                    visible_excerpt: Some("As chamas avançam para nordeste".into()),
                    ..ArticleConditions::default()
                }),
            }],
            regional: Some(RegionalContext {
                verified: true, // manually verified synthetic test input, never trusted from a model
                dominant_vegetation: vec!["eucalipto".into()],
                drought_status: Some("seca severa".into()),
                sources: vec!["https://www.ipma.pt/pt/oclima/observatorio.secas/".into()],
                ..RegionalContext::default()
            }),
            ..FireIntel::default()
        }
    }

    #[test]
    fn adjustments_are_deterministic_and_sourced() {
        let adj = derive_adjustments(&intel());
        assert_eq!(
            adj.regional_cover.as_ref().unwrap().value,
            LandCover::Eucalyptus
        );
        assert_eq!(adj.reported_spread_bearing.as_ref().unwrap().value, 45.0);
        assert_eq!(adj.reported_wind_from_deg.as_ref().unwrap().value, 225.0);
        assert_eq!(adj.drought_live_reduction.as_ref().unwrap().value, 15.0);
        assert!(
            adj.drought_live_reduction
                .as_ref()
                .unwrap()
                .url
                .contains("ipma.pt")
        );
        assert_eq!(adj.threatened_places[0].value, "Talhadas");
        assert_eq!(
            derive_adjustments(&FireIntel::default()),
            Adjustments::default()
        );
    }

    #[test]
    fn scrape_targets_are_restricted() {
        assert!(validate_scrape_url("https://www.publico.pt/2026/08/10/incendio").is_ok());
        assert!(validate_scrape_url("https://fogos.pt/pt").is_err());
        assert!(validate_scrape_url("http://www.rtp.pt/x").is_err());
        assert!(validate_scrape_url("https://evil.example/rtp.pt").is_err());
        assert!(validate_scrape_url("https://rtp.pt.evil.example/").is_err());
    }

    #[tokio::test]
    async fn gather_respects_budget_mode_and_records_calls() {
        use axum::{Json, Router, routing::post};
        let app = Router::new()
            .route("/news/search", post(|| async { Json(json!({"data":[{"url":"https://www.rtp.pt/noticias/a","title":"Incêndio","published_at":"2026-08-10T12:00:00Z","source":{"name":"RTP"}}],"has_more":false,"key_metadata":{"credits_consumed":1}})) }))
            .route("/web/search", post(|| async { Json(json!({"results":[{"url":"https://www.publico.pt/b","title":"Fogo"}],"key_metadata":{"credits_consumed":1}})) }))
            .route("/web/scrape", post(|Json(body): Json<Value>| async move {
                assert_eq!(body["formats"], json!({"markdown":true,"json":true}));
                assert!(body["jsonParams"]["schema"]["properties"]["reported_spread_direction"].is_object());
                Json(json!({"url":body["url"],"metadata":{"publishedTime":"2026-08-10T12:00:00Z"},"markdown":{"requested":true,"success":true,"data":"As chamas avançam para sul, em mato, junto ao IC2."},"json":{"requested":true,"success":true,"data":{"visible_excerpt":"As chamas avançam para sul, em mato, junto ao IC2.","reported_spread_direction":"para sul","vegetation":["mato"],"threatened_places":[],"barriers_or_containment_lines":["IC2"],"evacuations":[]}},"key_metadata":{"credits_consumed":5}}))
            }))
            .route("/web/answers", post(|| async { Json(json!({"json_content":{"dominant_vegetation":["pinhal"],"drought_status":"seca moderada"},"sources":["https://www.ipma.pt/x"],"key_metadata":{"credits_consumed":10}})) }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut client = ContextDev::new("fixture_key".into()).unwrap();
        client.inner = client
            .inner
            .with_local_test_endpoint(format!("http://{addr}"))
            .unwrap();
        let at = DateTime::parse_from_rfc3339("2026-08-10T13:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let live = client
            .gather(IntelMode::Live, "Sever do Vouga", at, 30.0)
            .await;
        assert!(live.errors.is_empty(), "{:?}", live.errors);
        assert_eq!(
            live.calls,
            vec!["web/search", "web/search", "web/scrape", "web/answers"]
        );
        assert_eq!(live.credits_used, 17.0);
        assert!(live.errors.is_empty(), "{:?}", live.errors);
        let adj = derive_adjustments(&live);
        assert_eq!(adj.reported_spread_bearing.unwrap().value, 180.0);
        assert_eq!(adj.barriers[0].value, "IC2");
        let tight = client
            .gather(IntelMode::Live, "Sever do Vouga", at, 7.0)
            .await;
        assert_eq!(tight.calls, vec!["web/search", "web/search", "web/scrape"]);
        let what_if = client
            .gather(IntelMode::WhatIf, "Sever do Vouga", at, 30.0)
            .await;
        assert_eq!(what_if.calls, vec!["web/answers"]);
        assert!(what_if.reports.is_empty());
        let limited = client.with_shared_budget(7.).unwrap();
        let first = limited
            .gather(IntelMode::Live, "Separate budget fixture", at, 7.)
            .await;
        assert_eq!(first.credits_used, 7.);
        let second = limited
            .gather(IntelMode::Live, "Separate budget fixture", at, 7.)
            .await;
        assert_eq!(second.credits_used, 0.);
        assert_eq!(limited.budget_status().actual, 7.);
        server.abort();
    }
}
