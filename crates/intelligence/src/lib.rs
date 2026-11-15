//! Public-page intelligence and weather adapters. Scheduling and durable storage belong to platform.
pub mod accuracy;
pub mod capture_archive;
pub mod context;
pub mod fogos;
pub mod ipma;
pub mod live;
pub mod news;
pub mod webhook;

use emberwatch_domain::{
    Coverage, DataMode, RunContext, SourceBatch, SourceError, SourceHealth, Timestamp, Usage,
};
use sha2::{Digest, Sha256};

pub(crate) fn error(code: &str, message: &str) -> SourceError {
    SourceError {
        code: code.into(),
        message: message.into(),
    }
}
pub(crate) fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
/// Immutable capture/revision key, deliberately separate from the upstream entity identity.
/// Reprocessing a stored capture must use its original capture time in RunContext.clock.
pub(crate) fn observation_id(
    run_id: &str,
    source_id: &str,
    source_identity: &str,
    captured_at: Timestamp,
    content_hash: &str,
) -> String {
    // A structured tuple prevents delimiter collisions in upstream IDs/locality text.
    hash(
        serde_json::json!([
            "observation-v1",
            run_id,
            source_id,
            source_identity,
            captured_at,
            content_hash
        ])
        .to_string()
        .as_bytes(),
    )
}
pub(crate) fn live_only(context: &RunContext) -> Result<(), SourceError> {
    if context.mode != DataMode::Live {
        return Err(error(
            "LIVE_ONLY",
            "Present-day sources cannot reconstruct a replay; use archived or synthetic batches.",
        ));
    }
    Ok(())
}
pub(crate) fn batch(context: &RunContext, source_id: &str) -> SourceBatch {
    SourceBatch {
        run_id: context.run_id.clone(),
        source_id: source_id.into(),
        captured_at: context.clock.now(),
        health: SourceHealth::Partial,
        coverage: Coverage {
            complete: false,
            explicit_empty: false,
            scope: "requested source page/window".into(),
            notes: vec![],
        },
        evidence: vec![],
        weather: vec![],
        usage: Usage::default(),
    }
}
pub(crate) fn in_bbox(position: [f64; 2], bbox: [f64; 4]) -> bool {
    position[0].is_finite()
        && position[1].is_finite()
        && position[0] >= bbox[0]
        && position[0] <= bbox[2]
        && position[1] >= bbox[1]
        && position[1] <= bbox[3]
}
