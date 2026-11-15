//! A successful HTTP call, an empty extraction and a positive empty page are different states.
use crate::{accuracy::normalize, context::ScrapeResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const FOGOS_LIST: &str = "https://fogos.pt/pt/lista";
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceStatus {
    pub name: String,
    pub url: String,
    pub via: String,
    pub request_id: Option<String>,
    pub captured_at: Option<DateTime<Utc>>,
    pub status: String,
    pub is_partial: bool,
    pub credits: Option<f64>,
    pub capture_succeeded: bool,
    pub positively_empty: bool,
    pub last_successful_capture_at: Option<DateTime<Utc>>,
    pub error_code: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiveStatus {
    pub mode: String,
    pub origin: String,
    pub checked_at: DateTime<Utc>,
    pub state: String,
    pub statement: Option<String>,
    pub statement_en: Option<String>,
    pub active: Vec<Value>,
    pub sources: Vec<SourceStatus>,
    pub note: String,
}
impl LiveStatus {
    pub fn failed(at: DateTime<Utc>, code: &str, last_success: Option<DateTime<Utc>>) -> Self {
        Self{mode:"live".into(),origin:"service".into(),checked_at:at,state:"check_failed".into(),statement:None,statement_en:None,active:vec![],
   sources:vec![SourceStatus{name:"Fogos.pt public incident list".into(),url:FOGOS_LIST.into(),via:"Context.dev web/scrape".into(),request_id:None,captured_at:None,status:"failed".into(),is_partial:false,credits:None,capture_succeeded:false,positively_empty:false,last_successful_capture_at:last_success,error_code:Some(code.into())}],
   note:"No verified current all-clear. An empty active array in a failed check does not mean no fires.".into()}
    }
    pub fn from_capture(response: &ScrapeResponse, at: DateTime<Utc>) -> Self {
        if response.url != FOGOS_LIST {
            return Self::failed(at, "SOURCE_URL_MISMATCH", None);
        }
        let mut result = Self::failed(at, "CAPTURE_FAILED", None);
        let source = &mut result.sources[0];
        source.request_id = response.request_id.clone();
        source.credits = response.usage().credits;
        source.captured_at = Some(at);
        source.is_partial =
            response.is_partial || response.markdown.is_truncated || response.json.is_truncated;
        let success = response.markdown.success == Some(true)
            && (!response.json.requested || response.json.success == Some(true));
        source.capture_succeeded = success;
        source.error_code = response
            .markdown
            .error_code
            .clone()
            .or(response.json.error_code.clone());
        if !success {
            return result;
        }
        source.last_successful_capture_at = (!source.is_partial).then_some(at);
        source.status = if source.is_partial { "partial" } else { "ok" }.into();
        result.state = "partial".into();
        let markdown = response.markdown.data.as_deref().unwrap_or_default();
        let rows = response
            .json
            .data
            .as_ref()
            .and_then(|j| j.get("incidents"))
            .and_then(Value::as_array);
        let no_rows = Vec::new();
        let rows = if response.json.requested {
            let Some(rows) = rows else {
                source.is_partial = true;
                source.status = "partial".into();
                return result;
            };
            rows
        } else {
            &no_rows
        };
        let empty = markdown
            .lines()
            .map(|line| normalize(line.trim_start_matches(['#', ' ', '*'])))
            .find(|line| {
                matches!(
                    line.to_lowercase().as_str(),
                    "sem registo de incêndios"
                        | "sem ocorrências ativas"
                        | "sem incêndios ativos"
                        | "no active incidents"
                        | "no active fires"
                )
            });
        let loading = ["a carregar", "aguarde", "loading incidents"]
            .iter()
            .any(|s| markdown.to_lowercase().contains(s));
        if !source.is_partial
            && !loading
            && rows.is_empty()
            && let Some(statement) = empty
        {
            source.positively_empty = true;
            result.state = "no_active_fires".into();
            result.statement = Some(statement);
            result.statement_en =
                Some("No active fires reported on the checked public page".into());
            result.note="This is the checked source's explicit statement at capture time, not independent proof of nationwide activity.".into();
            return result;
        }
        for row in rows {
            let excerpt = row
                .get("visible_excerpt")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !crate::accuracy::quote_verified(markdown, excerpt) {
                continue;
            }
            let field = |key: &str| {
                row.get(key)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty() && normalize(excerpt).contains(&normalize(s)))
            };
            let Some(place) = field("locality").or_else(|| field("municipality")) else {
                continue;
            };
            let Some(status) = field("raw_status") else {
                continue;
            };
            if ["conclu", "resolvid", "extint"]
                .iter()
                .any(|s| status.to_lowercase().contains(s))
            {
                continue;
            }
            let url = field("detail_url")
                .filter(|u| crate::context::validate_fogos_url(u).is_ok())
                .unwrap_or(FOGOS_LIST);
            let id = field("external_id")
                .map(str::to_owned)
                .unwrap_or_else(|| crate::hash(format!("{url}|{place}|{status}").as_bytes()));
            result
                .active
                .push(json!({"id":id,"place":place,"position":null,"status":status,"url":url}));
        }
        // Supported visible rows may be useful while national completeness stays unverified.
        if !result.active.is_empty()
            && !source.is_partial
            && response
                .json
                .data
                .as_ref()
                .and_then(|j| j.get("coverage"))
                .and_then(Value::as_str)
                == Some("complete")
            && result.active.len() == rows.len()
        {
            result.state = "active_fires".into();
        }
        if result.state == "partial" {
            source.status = "partial".into();
            source.is_partial = true;
            source.last_successful_capture_at = None;
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn response(md: &str, partial: bool) -> ScrapeResponse {
        serde_json::from_value(json!({"url":FOGOS_LIST,"markdown":{"requested":true,"success":true,"data":md},"json":{"requested":true,"success":true,"data":{"incidents":[],"coverage":"unknown"}},"isPartial":partial,"request_id":"synthetic-status-test"})).unwrap()
    }
    #[test]
    fn only_positive_complete_capture_allows_clear() {
        let now = Utc::now();
        for (md, partial) in [
            ("A carregar", false),
            ("Local Início Meios 0", false),
            ("#### Sem registo de incêndios", true),
            ("Não é verdade: Sem registo de incêndios", false),
        ] {
            let result = LiveStatus::from_capture(&response(md, partial), now);
            assert_ne!(result.state, "no_active_fires");
            assert!(!result.sources[0].positively_empty);
        }
        let result =
            LiveStatus::from_capture(&response("#### Sem registo de incêndios", false), now);
        assert_eq!(result.state, "no_active_fires");
        assert!(result.sources[0].capture_succeeded);
        assert!(result.sources[0].positively_empty);
    }
    #[test]
    fn failure_never_reuses_an_all_clear() {
        let now = Utc::now();
        let status = LiveStatus::failed(now, "CONTEXT_RATE_LIMIT", Some(now));
        assert_eq!(status.state, "check_failed");
        assert!(!status.sources[0].positively_empty);
        assert!(status.sources[0].captured_at.is_none());
    }
}
