use emberwatch_domain::{SourceError, Usage};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Output<T> {
    pub requested: bool,
    pub success: Option<bool>,
    pub data: Option<T>,
    pub error_code: Option<String>,
    #[serde(default, rename = "isTruncated")]
    pub is_truncated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeResponse {
    pub url: String,
    #[serde(default)]
    pub markdown: Output<String>,
    #[serde(default)]
    pub json: Output<Value>,
    #[serde(default, rename = "isPartial")]
    pub is_partial: bool,
    pub request_id: Option<String>,
    #[serde(default)]
    pub cache_metadata: Value,
    #[serde(default)]
    pub key_metadata: Value,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct BudgetStatus {
    pub limit: f64,
    pub committed: f64,
    pub actual: f64,
    pub outcome_unknown: bool,
}

#[derive(Clone, Copy)]
pub enum Endpoint {
    Scrape,
    Search,
    News,
    Answers,
}
impl Endpoint {
    fn path(self) -> &'static str {
        match self {
            Self::Scrape => "web/scrape",
            Self::Search => "web/search",
            Self::News => "news/search",
            Self::Answers => "web/answers",
        }
    }
}

impl ScrapeResponse {
    pub fn usage(&self) -> Usage {
        Usage {
            request_id: self.request_id.clone(),
            credits: self
                .key_metadata
                .get("credits_consumed")
                .and_then(Value::as_f64),
            cache_age_ms: self.cache_metadata.get("age_ms").and_then(Value::as_u64),
            latency_ms: None,
        }
    }
}

/// No Debug implementation: the API key must never appear in logs.
#[derive(Clone)]
pub struct ContextClient {
    http: reqwest::Client,
    api_key: String,
    base: String,
    budget: Arc<Mutex<BudgetStatus>>,
}

impl ContextClient {
    pub fn new(api_key: String) -> Result<Self, SourceError> {
        if api_key.trim().is_empty() {
            return Err(crate::error(
                "UNCONFIGURED",
                "Set CONTEXT_DEV_API_KEY on the server.",
            ));
        }
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(45))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("EmberWatch/0.1 public-page research")
            .build()
            .map_err(|_| crate::error("HTTP_CLIENT", "Could not initialize HTTP client."))?;
        Ok(Self {
            http,
            api_key,
            base: "https://api.context.dev/v1".into(),
            budget: Arc::new(Mutex::new(BudgetStatus {
                limit: 20.0,
                ..Default::default()
            })),
        })
    }
    pub fn with_budget(mut self, limit: f64) -> Result<Self, SourceError> {
        if !limit.is_finite() || limit < 0.0 {
            return Err(crate::error(
                "INVALID_BUDGET",
                "Use a finite nonnegative credit ceiling.",
            ));
        }
        self.budget = Arc::new(Mutex::new(BudgetStatus {
            limit,
            ..Default::default()
        }));
        Ok(self)
    }
    pub fn budget_status(&self) -> BudgetStatus {
        self.budget.lock().unwrap().clone()
    }
    pub fn redacted_capture(&self, value: &Value) -> Value {
        fn redact(value: &Value, secret: &str) -> Value {
            match value {
                Value::Object(fields) => Value::Object(
                    fields
                        .iter()
                        .filter_map(|(key, value)| {
                            let lower = key.to_lowercase();
                            if lower == "key_metadata" {
                                return Some((
                                    key.clone(),
                                    json!({"credits_consumed":value.get("credits_consumed")}),
                                ));
                            }
                            if [
                                "authorization",
                                "api_key",
                                "apikey",
                                "token",
                                "password",
                                "cookie",
                                "secret",
                                "account_id",
                                "organization_id",
                            ]
                            .iter()
                            .any(|s| lower.contains(s))
                            {
                                return None;
                            }
                            Some((key.clone(), redact(value, secret)))
                        })
                        .collect(),
                ),
                Value::Array(values) => {
                    Value::Array(values.iter().map(|v| redact(v, secret)).collect())
                }
                Value::String(s) => Value::String(s.replace(secret, "[REDACTED]")),
                _ => value.clone(),
            }
        }
        redact(value, &self.api_key)
    }
    /// Local upstream stubs only; never an arbitrary credential destination.
    pub fn with_local_test_endpoint(mut self, base: String) -> Result<Self, SourceError> {
        let url = reqwest::Url::parse(&base)
            .map_err(|_| crate::error("INVALID_TEST_ENDPOINT", "Invalid local stub URL."))?;
        if url.scheme() != "http"
            || !matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
        {
            return Err(crate::error(
                "INVALID_TEST_ENDPOINT",
                "Only loopback stubs are allowed.",
            ));
        }
        self.base = base;
        Ok(self)
    }
    pub fn from_env() -> Result<Self, SourceError> {
        Self::new(
            std::env::var("CONTEXT_DEV_API_KEY").map_err(|_| {
                crate::error("UNCONFIGURED", "Set CONTEXT_DEV_API_KEY on the server.")
            })?,
        )
    }
    pub async fn scrape_fogos(&self, url: &str) -> Result<ScrapeResponse, SourceError> {
        validate_fogos_url(url)?;
        let body = json!({"url":url,"formats":{"markdown":true,"json":true},"maxAgeMs":0,
            "sharedParams":{"waitFor":8000},
            "timeoutOpts":{"milliseconds":30000,"behavior":"return-partial"},
            "jsonParams":{"schema":fogos_schema(),"instructions":"Extract visible public-page rows only. Missing values are null. Never infer dates, coordinates, IDs, national completeness, or empty results from a loading shell. Visible excerpts must be exact contiguous text copied from the Markdown output, including punctuation and formatting; do not rephrase or concatenate separate rows. Preserve raw Portuguese text. If the page explicitly says there are no records, copy that exact text to explicit_empty_text. An empty filtered/table page alone does not establish national completeness. No API or hidden network requests."}});
        self.decode(Endpoint::Scrape, &body, 5.0).await
    }
    pub async fn scrape_fogos_markdown(&self, url: &str) -> Result<ScrapeResponse, SourceError> {
        validate_fogos_url(url)?;
        let body = json!({"url":url,"formats":{"markdown":true},"maxAgeMs":0,"sharedParams":{"waitFor":8000},"timeoutOpts":{"milliseconds":30000,"behavior":"return-partial"}});
        self.decode(Endpoint::Scrape, &body, 1.0).await
    }
    pub async fn search_portuguese_reports(
        &self,
        municipality: &str,
    ) -> Result<SearchResponse, SourceError> {
        if municipality.trim().is_empty()
            || municipality.chars().count() > 120
            || municipality.chars().any(char::is_control)
        {
            return Err(crate::error(
                "INVALID_QUERY",
                "Provide a bounded municipality name.",
            ));
        }
        self.decode(Endpoint::Search, &json!({"query":format!("incêndio rural {} Portugal",municipality),"numResults":5,
            "includeDomains":["prociv.gov.pt","icnf.pt","agif.pt","rtp.pt","sicnoticias.pt"],
            "languages":["pt"],"country":"PT","queryFanout":false,
            "markdownOptions":{"enabled":true,"maxAgeMs":0,"timeoutOpts":{"milliseconds":20000,"behavior":"return-partial"}}}), 6.0).await
    }
    async fn decode<T: DeserializeOwned>(
        &self,
        endpoint: Endpoint,
        body: &Value,
        ceiling: f64,
    ) -> Result<T, SourceError> {
        serde_json::from_value(self.request(endpoint, body, ceiling).await?)
            .map_err(|_| crate::error("INVALID_UPSTREAM_JSON", "Unexpected Context output shape."))
    }
    pub async fn request(
        &self,
        endpoint: Endpoint,
        body: &Value,
        ceiling: f64,
    ) -> Result<Value, SourceError> {
        let minimum = match endpoint {
            Endpoint::Scrape => {
                let url = body
                    .get("url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| crate::error("INVALID_SOURCE_URL", "Missing scrape URL."))?;
                let parsed = reqwest::Url::parse(url)
                    .map_err(|_| crate::error("INVALID_SOURCE_URL", "Invalid scrape URL."))?;
                if parsed
                    .host_str()
                    .is_some_and(|h| h == "fogos.pt" || h.ends_with(".fogos.pt"))
                {
                    validate_fogos_url(url)?;
                }
                if body.pointer("/sharedParams/actions").is_some()
                    || body.pointer("/sharedParams/profile").is_some()
                {
                    return Err(crate::error(
                        "UNBUDGETED_FEATURE",
                        "Browser actions/profiles are outside this bounded client.",
                    ));
                }
                if body
                    .get("formats")
                    .and_then(Value::as_object)
                    .is_some_and(|formats| {
                        formats.iter().any(|(key, value)| {
                            value == &json!(true)
                                && !matches!(key.as_str(), "markdown" | "html" | "json")
                        })
                    })
                {
                    return Err(crate::error(
                        "UNBUDGETED_FORMAT",
                        "Only reviewed text/JSON formats are supported.",
                    ));
                }
                if body.pointer("/formats/json").and_then(Value::as_bool) == Some(true) {
                    5.0
                } else {
                    1.0
                }
            }
            Endpoint::Search => {
                let results = body
                    .get("numResults")
                    .and_then(Value::as_u64)
                    .unwrap_or(10)
                    .clamp(1, 100) as f64;
                (results / 10.0).ceil()
                    + if body
                        .pointer("/markdownOptions/enabled")
                        .and_then(Value::as_bool)
                        == Some(true)
                    {
                        results
                    } else {
                        0.0
                    }
            }
            Endpoint::News => {
                (body.get("limit").and_then(Value::as_u64).unwrap_or(10) as f64 / 10.0).ceil()
            }
            Endpoint::Answers => {
                if body.get("mode").and_then(Value::as_str) != Some("fast") {
                    return Err(crate::error(
                        "UNBUDGETED_MODE",
                        "Only explicitly bounded fast answers are supported.",
                    ));
                }
                10.0
            }
        };
        if ceiling < minimum {
            return Err(crate::error(
                "CONTEXT_BUDGET",
                "Request reservation is below the reviewed maximum charge.",
            ));
        }
        if !ceiling.is_finite() || ceiling <= 0.0 {
            return Err(crate::error(
                "INVALID_BUDGET",
                "A request needs a positive maximum charge.",
            ));
        }
        {
            let mut budget = self.budget.lock().unwrap();
            if budget.outcome_unknown || budget.committed + ceiling > budget.limit {
                return Err(crate::error(
                    "CONTEXT_BUDGET",
                    "Credit ceiling reached or prior outcome unknown; inspect usage before continuing.",
                ));
            }
            budget.committed += ceiling;
        }
        // Do not retry billed POSTs implicitly: a timeout may already have incurred usage.
        // A owns bounded scheduling/backoff and records one source-run identity per attempt.
        let response = self
            .http
            .post(format!("{}/{}", self.base, endpoint.path()))
            .bearer_auth(&self.api_key)
            .json(body)
            .send()
            .await
            .map_err(|_| {
                self.budget.lock().unwrap().outcome_unknown = true;
                crate::error(
                    "CONTEXT_NETWORK",
                    "Context request failed or timed out; outcome and credits may be unknown.",
                )
            })?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            self.budget.lock().unwrap().committed -= ceiling;
            return Err(crate::error(
                http_error_code(status),
                "Context rejected the request. Check authentication, credit balance, limits, or service health; do not log the response body.",
            ));
        }
        let value: Value = match read_json(response, 8 * 1024 * 1024).await {
            Ok(value) => value,
            Err(error) => {
                self.budget.lock().unwrap().outcome_unknown = true;
                return Err(error);
            }
        };
        let mut budget = self.budget.lock().unwrap();
        match value
            .pointer("/key_metadata/credits_consumed")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite() && *v >= 0.0)
        {
            Some(credits) => {
                budget.committed += credits - ceiling;
                budget.actual += credits;
                if credits > ceiling {
                    budget.outcome_unknown = true;
                }
            }
            None => budget.outcome_unknown = true,
        }
        Ok(value)
    }
}

pub fn http_error_code(status: u16) -> &'static str {
    match status {
        401 | 403 => "CONTEXT_AUTH",
        402 => "CONTEXT_CREDITS",
        429 => "CONTEXT_RATE_LIMIT",
        408 => "CONTEXT_TIMEOUT",
        500..=599 => "CONTEXT_UPSTREAM",
        _ => "CONTEXT_REQUEST",
    }
}

pub(crate) async fn read_json<T: DeserializeOwned>(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<T, SourceError> {
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err(crate::error(
            "PAYLOAD_TOO_LARGE",
            "Upstream payload exceeds the configured bound.",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| crate::error("UPSTREAM_BODY", "Unable to read upstream payload."))?
    {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(crate::error(
                "PAYLOAD_TOO_LARGE",
                "Upstream payload exceeds the configured bound.",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| {
        crate::error(
            "INVALID_UPSTREAM_JSON",
            "Upstream response does not match the expected JSON contract.",
        )
    })
}

pub fn validate_fogos_url(value: &str) -> Result<(), SourceError> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| crate::error("INVALID_SOURCE_URL", "Invalid source URL."))?;
    if url.scheme() != "https"
        || url.host_str() != Some("fogos.pt")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !(matches!(url.path(), "/pt" | "/pt/lista" | "/pt/tabela")
            || url
                .path()
                .strip_prefix("/pt/fogo/")
                .is_some_and(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit())))
    {
        return Err(crate::error(
            "SOURCE_URL_NOT_ALLOWED",
            "Only reviewed Fogos public list pages are allowed; API/hidden endpoints are excluded.",
        ));
    }
    Ok(())
}

pub fn fogos_schema() -> Value {
    let mut properties = serde_json::Map::new();
    for key in [
        "external_id",
        "detail_url",
        "locality",
        "municipality",
        "raw_type",
        "raw_status",
        "start_text",
        "updated_text",
        "visible_excerpt",
    ] {
        properties.insert(key.into(), json!({"type":["string","null"]}));
    }
    for key in ["personnel", "vehicles", "aircraft"] {
        properties.insert(key.into(), json!({"type":["integer","null"],"minimum":0}));
    }
    for key in ["longitude", "latitude"] {
        properties.insert(key.into(), json!({"type":["number","null"]}));
    }
    json!({"type":"object","properties":{
        "incidents":{"type":"array","maxItems":500,"items":{"type":"object","properties":properties,"additionalProperties":false}},
        "page_update_text":{"type":["string","null"]},"explicit_empty_text":{"type":["string","null"]},
        "coverage":{"type":"string","enum":["complete","partial","unknown"]},"coverage_reason":{"type":"string"}},
        "required":["incidents","coverage","coverage_reason"],"additionalProperties":false})
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub request_id: Option<String>,
    #[serde(default)]
    pub cache_metadata: Value,
    #[serde(default)]
    pub key_metadata: Value,
}
#[derive(Debug, Clone, Deserialize)]
pub struct SearchResult {
    pub url: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub markdown: SearchMarkdown,
}
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SearchMarkdown {
    #[serde(default)]
    pub code: String,
    pub markdown: Option<String>,
    #[serde(rename = "finalDOMState")]
    pub final_dom_state: Option<String>,
}

pub(crate) fn latency_ms(start: Instant) -> u64 {
    start.elapsed().as_millis().min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        http::{HeaderMap, StatusCode},
        routing::post,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn raw_capture_redacts_credentials_and_account_metadata() {
        let client = ContextClient::new("synthetic_private_key".into()).unwrap();
        let raw = json!({"markdown":{"data":"public text synthetic_private_key"},"key_metadata":{"credits_consumed":5,"credits_remaining":900},"organization_id":"private_org","nested":{"Authorization":"Bearer synthetic_private_key","cookie":"private"}});
        let result = client.redacted_capture(&raw);
        let text = result.to_string();
        assert!(!text.contains("synthetic_private_key"));
        assert!(!text.contains("private_org"));
        assert!(!text.contains("credits_remaining"));
        assert_eq!(result["key_metadata"]["credits_consumed"], 5);
    }
    #[tokio::test]
    async fn budget_refuses_a_request_before_any_network_call() {
        let client = ContextClient::new("fixture_key".into())
            .unwrap()
            .with_budget(0.)
            .unwrap();
        assert_eq!(
            client
                .scrape_fogos("https://fogos.pt/pt/lista")
                .await
                .unwrap_err()
                .code,
            "CONTEXT_BUDGET"
        );
        assert_eq!(client.budget_status().committed, 0.);
    }
    #[tokio::test]
    async fn missing_charge_metadata_stops_followup_requests() {
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        let app = Router::new().route(
            "/web/search",
            post(move || {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Json(json!({"results":[],"request_id":"synthetic_no_usage"})) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = ContextClient::new("fixture_key".into())
            .unwrap()
            .with_local_test_endpoint(format!("http://{address}"))
            .unwrap();
        let body = json!({"query":"synthetic","numResults":1});
        client.request(Endpoint::Search, &body, 1.).await.unwrap();
        assert!(client.budget_status().outcome_unknown);
        assert_eq!(
            client
                .request(Endpoint::Search, &body, 1.)
                .await
                .unwrap_err()
                .code,
            "CONTEXT_BUDGET"
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
        task.abort();
    }

    #[tokio::test]
    async fn public_scrape_sends_fresh_nullable_contract_and_checks_outputs() {
        async fn upstream(headers: HeaderMap, Json(body): Json<Value>) -> Json<Value> {
            assert_eq!(
                headers.get("authorization").unwrap(),
                "Bearer fixture_server_key"
            );
            assert_eq!(body["url"], "https://fogos.pt/pt");
            assert_eq!(body["maxAgeMs"], 0);
            assert_eq!(body["sharedParams"]["waitFor"], 8000);
            assert_eq!(body["formats"], json!({"markdown":true,"json":true}));
            assert_eq!(
                body["jsonParams"]["schema"]["properties"]["incidents"]["items"]["properties"]["latitude"]
                    ["type"],
                json!(["number", "null"])
            );
            Json(
                json!({"url":"https://fogos.pt/pt","markdown":{"requested":true,"success":true,"data":"A carregar"},"json":{"requested":true,"success":false,"data":null,"error_code":"REQUEST_TIMEOUT"},"isPartial":true,"request_id":"fixture_request","key_metadata":{"credits_consumed":1}}),
            )
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/web/scrape", post(upstream)))
                .await
                .unwrap()
        });
        let mut client = ContextClient::new("fixture_server_key".into()).unwrap();
        client.base = format!("http://{address}");
        let response = client.scrape_fogos("https://fogos.pt/pt").await.unwrap();
        assert_eq!(response.json.success, Some(false));
        assert!(response.is_partial);
        assert_eq!(response.usage().credits, Some(1.0));
        server.abort();
    }

    #[tokio::test]
    async fn rate_limit_is_actionable_and_billed_post_is_not_blindly_retried() {
        let count = Arc::new(AtomicUsize::new(0));
        let upstream_count = count.clone();
        let app = Router::new().route(
            "/web/scrape",
            post(move || {
                upstream_count.fetch_add(1, Ordering::SeqCst);
                async { StatusCode::TOO_MANY_REQUESTS }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut client = ContextClient::new("fixture_server_key".into()).unwrap();
        client.base = format!("http://{address}");
        assert_eq!(
            client
                .scrape_fogos("https://fogos.pt/pt")
                .await
                .unwrap_err()
                .code,
            "CONTEXT_RATE_LIMIT"
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
        server.abort();
    }

    #[test]
    fn source_allowlist_prevents_api_and_arbitrary_paid_targets() {
        for url in [
            "https://fogos.pt/pt/api",
            "https://api.fogos.pt/",
            "https://fogos.pt/gaia",
            "https://fogos.pt/pt?url=other",
            "http://fogos.pt/pt",
            "https://user:pass@fogos.pt/pt",
            "https://example.com/pt",
        ] {
            assert!(validate_fogos_url(url).is_err(), "{url}");
        }
        assert!(validate_fogos_url("https://fogos.pt/pt/lista").is_ok());
    }
}
