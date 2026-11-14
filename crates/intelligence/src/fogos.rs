use crate::context::{ContextClient, ScrapeResponse, latency_ms, validate_fogos_url};
use emberwatch_domain::{
    Evidence, Provenance, RunContext, Source, SourceBatch, SourceError, SourceFuture, SourceHealth,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::HashSet, time::Instant};

#[derive(Debug, Clone, Default)]
pub struct FogosPolicy {
    /// Recurring extraction/reuse checks must be resolved by the operator, not this adapter.
    pub source_checks_passed: bool,
    /// Human validation that pagination, filters and full source coverage were inspected.
    pub coverage_verified: bool,
}
pub struct FogosSource {
    client: ContextClient,
    policy: FogosPolicy,
    url: String,
}
impl FogosSource {
    pub fn new(
        client: ContextClient,
        policy: FogosPolicy,
        url: String,
    ) -> Result<Self, SourceError> {
        validate_fogos_url(&url)?;
        Ok(Self {
            client,
            policy,
            url,
        })
    }
}
impl Source for FogosSource {
    fn id(&self) -> &str {
        "fogos"
    }
    fn fetch<'a>(&'a self, context: &'a RunContext) -> SourceFuture<'a> {
        Box::pin(async move {
            crate::live_only(context)?;
            if !self.policy.source_checks_passed {
                let mut result = crate::batch(context, self.id());
                result.health = SourceHealth::NeedsValidation;
                result.coverage.notes.push("Website rights/crawler/extraction checks unresolved; no upstream request issued.".into());
                return Ok(result);
            }
            let start = Instant::now();
            let response = self.client.scrape_fogos(&self.url).await?;
            let mut result =
                normalize_fogos_with_coverage(context, &response, self.policy.coverage_verified)?;
            result.usage.latency_ms = Some(latency_ms(start));
            Ok(result)
        })
    }
}

#[derive(Debug, Deserialize)]
struct Page {
    incidents: Vec<Value>,
    coverage: String,
    #[serde(default)]
    coverage_reason: String,
    explicit_empty_text: Option<String>,
    #[serde(default)]
    page_update_text: Option<String>,
}

pub fn normalize_fogos(
    context: &RunContext,
    response: &ScrapeResponse,
) -> Result<SourceBatch, SourceError> {
    normalize_fogos_with_coverage(context, response, false)
}
pub fn normalize_fogos_with_coverage(
    context: &RunContext,
    response: &ScrapeResponse,
    verified: bool,
) -> Result<SourceBatch, SourceError> {
    crate::live_only(context)?;
    validate_fogos_url(&response.url)?;
    let mut result = crate::batch(context, "fogos");
    result.usage = response.usage();
    result.coverage.scope="Fogos.pt reviewed public page, bbox-filtered where coordinates exist; not guaranteed national coverage".into();
    for output_code in [&response.markdown.error_code, &response.json.error_code]
        .into_iter()
        .flatten()
    {
        if matches!(
            output_code.as_str(),
            "WEBSITE_BLOCKED" | "WEBSITE_ACCESS_ERROR" | "ACCESS_DENIED"
        ) {
            result.health = SourceHealth::Blocked;
            result.coverage.notes.push(format!(
                "Context output: {output_code}; stop and retain last good data as stale."
            ));
            return Ok(result);
        }
    }
    if response.markdown.success != Some(true) || response.json.success != Some(true) {
        result.health = SourceHealth::Failed;
        result.coverage.notes.push(
            "Required Markdown/JSON output failed; never interpret this as zero incidents.".into(),
        );
        return Ok(result);
    }
    let markdown = response.markdown.data.as_deref().unwrap_or_default();
    let Some(data) = response.json.data.clone() else {
        result.health = SourceHealth::Failed;
        return Ok(result);
    };
    let page: Page = serde_json::from_value(data).map_err(|_| {
        crate::error(
            "INVALID_EXTRACTION",
            "Fogos extracted JSON does not match the nullable page schema.",
        )
    })?;
    if page.incidents.len() > 500 {
        return Err(crate::error(
            "EXTRACTION_LIMIT",
            "Page exceeds the bounded extraction limit.",
        ));
    }
    result.coverage.notes.push(page.coverage_reason);
    if page.page_update_text.is_some() {
        result.coverage.notes.push(
            "Raw page update text retained on records; no timezone or event time inferred.".into(),
        );
    }
    let mut seen = HashSet::new();
    let mut rejected = 0;
    for row in page.incidents {
        let excerpt = text(&row, "visible_excerpt");
        if excerpt.is_none_or(|value| value.trim().is_empty() || !markdown.contains(value)) {
            rejected += 1;
            continue;
        }
        let excerpt = excerpt.unwrap_or_default();
        // Keep only literal facts supported by the captured page, not plausible AI completion.
        let visible = |key: &str| {
            text(&row, key)
                .filter(|value| !value.trim().is_empty() && excerpt.contains(*value))
                .map(str::to_owned)
        };
        let locality = visible("locality");
        let municipality = visible("municipality");
        let raw_status = visible("raw_status");
        let raw_type = visible("raw_type");
        if locality.is_none() && municipality.is_none() {
            rejected += 1;
            continue;
        }
        let external_id = visible("external_id");
        let detail_url = visible("detail_url").filter(|value| public_detail_link(value));
        let event_at = visible("start_text")
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(&value).ok())
            .map(|time| time.to_utc());
        let raw_start = visible("start_text");
        let mut flags = vec![
            "public_page_extraction".into(),
            "via_context_dev".into(),
            "source_status_unverified".into(),
        ];
        if event_at.is_none() {
            flags.push("event_time_unknown".into());
        }
        let position = match (
            row.get("longitude").and_then(Value::as_f64),
            row.get("latitude").and_then(Value::as_f64),
        ) {
            (Some(lon), Some(lat))
                if excerpt.contains(&lon.to_string()) && excerpt.contains(&lat.to_string()) =>
            {
                Some([lon, lat])
            }
            _ => None,
        };
        if position.is_some_and(|p| !crate::in_bbox(p, context.bbox)) {
            continue;
        }
        if position.is_none() {
            flags.push("location_requires_separate_uncertain_geocode".into());
        }
        let identity = external_id
            .clone()
            .or(detail_url.clone())
            .unwrap_or_else(|| {
                format!(
                    "{}|{}|{}|{}|{}",
                    locality.as_deref().unwrap_or_default(),
                    municipality.as_deref().unwrap_or_default(),
                    raw_type.as_deref().unwrap_or_default(),
                    raw_start.as_deref().unwrap_or_default(),
                    if raw_start.is_none() { excerpt } else { "" }
                )
            });
        let id = crate::hash(format!("{}|fogos|{identity}", context.run_id).as_bytes());
        if !seen.insert(id.clone()) {
            continue;
        }
        let mut attributes = json!({"via":"Context.dev","raw_start_text":raw_start,"page_update_text":page.page_update_text,"raw_type":raw_type,"source_attribution":"Source: Fogos.pt via Context.dev; underlying public reports: ANEPC","resource_counts":{}});
        for key in ["personnel", "vehicles", "aircraft"] {
            // A count requires literal support; retaining null is safer than matching an unrelated page number.
            attributes["resource_counts"][key] = Value::Null;
            if row.get(key).is_some_and(|v| !v.is_null()) {
                flags.push(format!("{key}_requires_row_level_validation"));
            }
        }
        result.evidence.push(Evidence {
            id,
            run_id: context.run_id.clone(),
            mode: context.mode,
            provenance: Provenance {
                source_id: "fogos".into(),
                source_url: detail_url.unwrap_or_else(|| response.url.clone()),
                origin_group: "anepc".into(),
                external_id,
                event_at,
                published_at: None,
                captured_at: result.captured_at,
                available_at: result.captured_at,
                availability_basis: "captured_public_page; no historical availability claim".into(),
                evidence_hash: crate::hash(excerpt.as_bytes()),
                quality_flags: flags,
            },
            position,
            uncertainty_m: None,
            location_name: locality,
            municipality,
            kind: "public_report".into(),
            raw_status,
            title: raw_type.unwrap_or_else(|| "Ocorrência pública reportada".into()),
            body: Some(excerpt.into()),
            attributes,
        });
    }
    result.coverage.complete = verified
        && page.coverage == "complete"
        && !response.is_partial
        && rejected == 0
        && !markdown.trim().is_empty();
    result.coverage.explicit_empty = result.coverage.complete
        && result.evidence.is_empty()
        && page
            .explicit_empty_text
            .as_deref()
            .is_some_and(|text| explicit_empty_statement(text) && markdown.contains(text));
    if rejected > 0 {
        result.coverage.notes.push(format!(
            "Rejected {rejected} rows without literal page evidence."
        ));
    }
    result.health = if result.coverage.explicit_empty {
        SourceHealth::EmptyVerified
    } else if result.coverage.complete && !result.evidence.is_empty() {
        SourceHealth::Healthy
    } else {
        SourceHealth::Partial
    };
    if result.evidence.is_empty() && !result.coverage.explicit_empty {
        result.coverage.complete = false;
        result
            .coverage
            .notes
            .push("No validated rows; source activity is unknown, not zero.".into());
    }
    Ok(result)
}
fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}
fn explicit_empty_statement(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "sem ocorrências ativas",
        "sem ocorrencias ativas",
        "não existem ocorrências",
        "nenhuma ocorrência",
        "sem incêndios ativos",
        "não há incêndios",
        "no active incidents",
        "no active fires",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
}
fn public_detail_link(value: &str) -> bool {
    reqwest::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("fogos.pt")
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url.path().starts_with("/pt/")
            && !url.path().contains("api")
            && !url.path().contains("gaia")
    })
}
