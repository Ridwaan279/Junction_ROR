//! The retained captures are real; automatic extraction is not a human gold label.
use serde_json::Value;
use sha2::{Digest, Sha256};
#[test]
fn golden_arganil_inventory_does_not_claim_missing_reviews_or_source_text() {
    let manifest: Value =
        serde_json::from_str(include_str!("fixtures/golden/manifest.json")).unwrap();
    let pages: Value = serde_json::from_str(include_str!(
        "../../../data/demo/arganil-early/context_dev/pages.json"
    ))
    .unwrap();
    let cases = manifest["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 25);
    let mut reviewed = 0;
    for case in cases {
        let page = &pages[case["page_index"].as_u64().unwrap() as usize];
        assert_eq!(case["url"], page["url"]);
        assert_eq!(case["request_id"], page["request_id"]);
        if case["human_reviewer"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty())
            && case["expected_file"].is_string()
            && case["raw_response_file"].is_string()
        {
            reviewed += 1;
        }
    }
    println!(
        "golden pages={}, human-reviewed with source text={reviewed}; fabrication/recall/time/place/lineage metrics NOT ASSESSED until labels and raw source text are supplied",
        cases.len()
    );
    assert_eq!(manifest["label_status"], "pending_human_review");
    assert_eq!(
        reviewed, 0,
        "Promoting labels requires implementing and running the full documented threshold evaluator"
    );
}

#[test]
fn golden_private_originals_match_retained_hashes_when_available() {
    let Ok(directory) = std::env::var("EMBERWATCH_GOLDEN_RAW_DIR") else {
        println!(
            "Original-source hash/quote audit not run: set EMBERWATCH_GOLDEN_RAW_DIR to the private captures. Human gold labels remain required."
        );
        return;
    };
    let manifest: Value =
        serde_json::from_str(include_str!("fixtures/golden/manifest.json")).unwrap();
    let mut hashed = 0;
    let mut facts = 0;
    let mut supported = 0;
    let mut numeric = 0;
    let mut numeric_supported = 0;
    for case in manifest["cases"].as_array().unwrap() {
        let Some(filename) = case["raw_response_file"].as_str() else {
            continue;
        };
        assert_eq!(
            std::path::Path::new(filename).components().count(),
            1,
            "Private fixture reference must be a filename"
        );
        let raw: Value = serde_json::from_slice(
            &std::fs::read(std::path::Path::new(&directory).join(filename)).unwrap(),
        )
        .unwrap();
        assert_eq!(raw["request_id"], case["request_id"]);
        let md = raw
            .pointer("/markdown/data")
            .and_then(Value::as_str)
            .unwrap();
        assert_eq!(
            hex::encode(Sha256::digest(md.as_bytes())),
            case["markdown_sha256"].as_str().unwrap()
        );
        hashed += 1;
        if case["url"].as_str().unwrap().contains("fogos.pt/") {
            continue;
        } // present-day counters are not historical evidence
        for fact in raw
            .pointer("/json/data/facts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let quote = fact["quote"].as_str().unwrap_or_default();
            facts += 1;
            if emberwatch_intelligence::accuracy::quote_verified(md, quote) {
                supported += 1;
            }
            if let Some(value) = fact["number"].as_f64() {
                numeric += 1;
                if emberwatch_intelligence::accuracy::number_supported(value, quote) {
                    numeric_supported += 1;
                }
            }
        }
    }
    println!(
        "original Markdown hashes verified={hashed}; candidate quote support={supported}/{facts}; candidate numeric support={numeric_supported}/{numeric}; these are grounding checks, NOT independent recall/fabrication metrics"
    );
    assert_eq!(hashed, 23);
}

#[test]
fn golden_real_fogos_list_proves_only_the_recorded_snapshot() {
    let Ok(directory) = std::env::var("EMBERWATCH_GOLDEN_RAW_DIR") else {
        return;
    };
    let raw: Value = serde_json::from_slice(
        &std::fs::read(std::path::Path::new(&directory).join("live_lista.json")).unwrap(),
    )
    .unwrap();
    let reference: Value = serde_json::from_str(include_str!(
        "../../../data/demo/live/fogos-list-check-2026-11-15.json"
    ))
    .unwrap();
    assert_eq!(raw["request_id"], reference["sources"][0]["request_id"]);
    let response: emberwatch_intelligence::context::ScrapeResponse =
        serde_json::from_value(raw).unwrap();
    let at = chrono::DateTime::parse_from_rfc3339(reference["checked_at"].as_str().unwrap())
        .unwrap()
        .with_timezone(&chrono::Utc);
    let result = emberwatch_intelligence::live::LiveStatus::from_capture(&response, at);
    assert_eq!(result.state, "no_active_fires");
    assert!(result.sources[0].capture_succeeded);
    assert!(result.sources[0].positively_empty);
    assert_eq!(result.statement.as_deref(), reference["statement"].as_str());
    assert_eq!(result.checked_at, at);
}
