//! Bounded Portuguese public reports and grounded, cited text fallback.
use crate::context::{ContextClient, SearchResponse};
use emberwatch_domain::{
    Evidence, Provenance, RunContext, Source, SourceBatch, SourceError, SourceFuture, SourceHealth,
    Usage,
};
use serde_json::json;
use std::collections::HashSet;

pub struct PortugueseNewsSource {
    client: ContextClient,
    municipality: String,
}
impl PortugueseNewsSource {
    pub fn new(client: ContextClient, municipality: String) -> Self {
        Self {
            client,
            municipality,
        }
    }
}
impl Source for PortugueseNewsSource {
    fn id(&self) -> &str {
        "portuguese-news"
    }
    fn fetch<'a>(&'a self, context: &'a RunContext) -> SourceFuture<'a> {
        Box::pin(async move {
            crate::live_only(context)?;
            let response = self
                .client
                .search_portuguese_reports(&self.municipality)
                .await?;
            normalize_news(context, &response)
        })
    }
}
pub fn normalize_news(
    context: &RunContext,
    response: &SearchResponse,
) -> Result<SourceBatch, SourceError> {
    crate::live_only(context)?;
    let mut result = crate::batch(context, "portuguese-news");
    result.usage = Usage {
        request_id: response.request_id.clone(),
        credits: response.key_metadata["credits_consumed"].as_f64(),
        cache_age_ms: response.cache_metadata["age_ms"].as_u64(),
        latency_ms: None,
    };
    result.coverage.scope =
        "At most five allowlisted Portuguese web-search results; not exhaustive reporting".into();
    let mut seen = HashSet::new();
    let mut blocked = false;
    for item in response.results.iter().take(5) {
        let Ok(url) = reqwest::Url::parse(&item.url) else {
            continue;
        };
        let host = url
            .host_str()
            .unwrap_or_default()
            .trim_start_matches("www.");
        if url.scheme() != "https"
            || !matches!(
                host,
                "prociv.gov.pt" | "icnf.pt" | "agif.pt" | "rtp.pt" | "sicnoticias.pt"
            )
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
        {
            continue;
        }
        if item.markdown.code == "WEBSITE_BLOCKED" || item.markdown.code == "WEBSITE_ACCESS_ERROR" {
            blocked = true;
            continue;
        }
        if item.markdown.code != "SUCCESS"
            || item.markdown.final_dom_state.as_deref() != Some("loaded")
        {
            continue;
        }
        let Some(body) = item
            .markdown
            .markdown
            .as_deref()
            .filter(|v| !v.trim().is_empty())
        else {
            continue;
        };
        // An ANEPC mention conservatively shares lineage; unreviewed articles are never independent confirmation.
        let origin = if host == "prociv.gov.pt" || body.to_uppercase().contains("ANEPC") {
            "anepc"
        } else {
            "unreviewed_public_report"
        };
        let mut clean_url = url;
        clean_url.set_fragment(None);
        let source_url = clean_url.to_string();
        if !seen.insert(source_url.clone()) {
            continue;
        }
        let text: String = body.chars().take(6000).collect();
        let content_hash = crate::hash(json!([&item.title, body]).to_string().as_bytes());
        let id = crate::observation_id(
            &context.run_id,
            "portuguese-news",
            &source_url,
            result.captured_at,
            &content_hash,
        );
        result.evidence.push(Evidence{id,run_id:context.run_id.clone(),mode:context.mode,
            provenance:Provenance{source_id:"portuguese-news".into(),source_url:source_url.clone(),origin_group:origin.into(),external_id:Some(source_url),event_at:None,published_at:None,captured_at:result.captured_at,available_at:result.captured_at,
                availability_basis:"captured_article; publication/event availability not inferred from search ranking".into(),evidence_hash:crate::hash(body.as_bytes()),quality_flags:vec!["via_context_dev".into(),"article_not_an_incident_confirmation".into(),"lineage_requires_review".into(),"location_requires_separate_uncertain_geocode".into()]},
            position:None,uncertainty_m:None,location_name:None,municipality:None,kind:"web_report".into(),raw_status:None,title:item.title.chars().take(300).collect(),body:Some(text),attributes:json!({"source":"Portuguese public report via Context.dev","query_scope":"bounded search","geocoding_hints_require_review":true})});
    }
    result.health = if !result.evidence.is_empty() {
        SourceHealth::Partial
    } else if blocked {
        SourceHealth::Blocked
    } else {
        SourceHealth::Partial
    };
    result.coverage.notes.push("No result, inaccessible result, or a partial article is not an all-clear. Upstream origin and incident location require review.".into());
    Ok(result)
}
pub fn cited_summary(evidence: &[Evidence]) -> (String, String, Vec<String>) {
    let mut citations = Vec::new();
    let mut fragments = Vec::new();
    for record in evidence.iter().take(10) {
        let source = &record.provenance.source_url;
        if !citations.contains(source) {
            citations.push(source.clone());
        }
        let number = citations
            .iter()
            .position(|url| url == source)
            .unwrap_or_default()
            + 1;
        let excerpt: String = record
            .body
            .as_deref()
            .unwrap_or(&record.title)
            .chars()
            .take(240)
            .collect();
        fragments.push(format!("[{number}] {excerpt}"));
    }
    let content = fragments.join("\n");
    (
        format!("Relatos públicos citados; sem confirmação independente automática.\n{content}"),
        format!(
            "Cited public reports; no automatic independent confirmation. Original Portuguese excerpts follow.\n{content}"
        ),
        citations,
    )
}
