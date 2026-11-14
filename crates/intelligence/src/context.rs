use emberwatch_domain::{SourceError, Usage};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Output<T> {
    pub requested: bool,
    pub success: Option<bool>,
    pub data: Option<T>,
    pub error_code: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ScrapeResponse {
    pub url: String,
    pub markdown: Output<String>,
    pub json: Output<Value>,
    #[serde(default, rename = "isPartial")]
    pub is_partial: bool,
    pub request_id: Option<String>,
    #[serde(default)]
    pub cache_metadata: Value,
    #[serde(default)]
    pub key_metadata: Value,
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
        })
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
            "timeoutOpts":{"milliseconds":30000,"behavior":"return-partial"},
            "jsonParams":{"schema":fogos_schema(),"instructions":"Extract visible public-page rows only. Missing values are null. Never infer dates, coordinates, IDs, national completeness, or empty results from a loading shell. Visible excerpts must quote the page. Preserve raw Portuguese text. No API or hidden network requests."}});
        self.post("web/scrape", &body).await
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
        self.post("web/search", &json!({"query":format!("incêndio rural {} Portugal",municipality),"numResults":5,
            "includeDomains":["prociv.gov.pt","icnf.pt","agif.pt","rtp.pt","sicnoticias.pt"],
            "languages":["pt"],"country":"PT","queryFanout":false,
            "markdownOptions":{"enabled":true,"maxAgeMs":0,"timeoutOpts":{"milliseconds":20000,"behavior":"return-partial"}}})).await
    }
    async fn post<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T, SourceError> {
        // Do not retry billed POSTs implicitly: a timeout may already have incurred usage.
        // A owns bounded scheduling/backoff and records one source-run identity per attempt.
        let response = self
            .http
            .post(format!("{}/{}", self.base, path))
            .bearer_auth(&self.api_key)
            .json(body)
            .send()
            .await
            .map_err(|_| {
                crate::error(
                    "CONTEXT_NETWORK",
                    "Context request failed or timed out; outcome and credits may be unknown.",
                )
            })?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(crate::error(
                http_error_code(status),
                "Context rejected the request. Check authentication, credit balance, limits, or service health; do not log the response body.",
            ));
        }
        read_json(response, 2 * 1024 * 1024).await
    }
}

pub fn http_error_code(status: u16) -> &'static str {
    match status {
        401 | 403 => "CONTEXT_AUTH",
        402 => "CONTEXT_CREDITS",
        429 => "CONTEXT_RATE_LIMIT",
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
        || !matches!(url.path(), "/pt" | "/pt/lista" | "/pt/tabela")
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
    pub markdown: SearchMarkdown,
}
#[derive(Debug, Clone, Deserialize)]
pub struct SearchMarkdown {
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

    #[tokio::test]
    async fn public_scrape_sends_fresh_nullable_contract_and_checks_outputs() {
        async fn upstream(headers: HeaderMap, Json(body): Json<Value>) -> Json<Value> {
            assert_eq!(
                headers.get("authorization").unwrap(),
                "Bearer fixture_server_key"
            );
            assert_eq!(body["url"], "https://fogos.pt/pt");
            assert_eq!(body["maxAgeMs"], 0);
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
