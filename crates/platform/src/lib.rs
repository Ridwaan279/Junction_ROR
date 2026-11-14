//! Run-scoped ingestion, identity, persistence and deterministic scenario replay.
pub mod engine;
pub mod fusion;
pub mod sources;
pub mod store;
pub mod uploads;
use chrono::{Duration, Utc};
pub use emberwatch_domain::*;
pub use engine::{Platform, PlatformError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub use store::Store;
pub mod persistence;

#[derive(Clone, Serialize, Deserialize)]
pub struct Engine {
    pub scenario: Scenario,
    initial: Snapshot,
    evidence: BTreeMap<String, Evidence>,
    weather: BTreeMap<String, Weather>,
    sources: Vec<SourceStatus>,
    #[serde(default)]
    source_history: Vec<(Timestamp, SourceStatus)>,
    receipts: BTreeSet<String>,
    pub forecasts: Vec<Forecast>,
    pub drone_plans: Vec<DronePlan>,
    pub briefings: Vec<Briefing>,
}
impl Engine {
    pub fn synthetic() -> Result<Self, String> {
        let initial: Snapshot =
            serde_json::from_str(include_str!("../../../contracts/fixtures/snapshot.json"))
                .map_err(|e| e.to_string())?;
        let later: Vec<Evidence> = serde_json::from_str(include_str!(
            "../../../contracts/fixtures/replay-evidence.json"
        ))
        .map_err(|e| e.to_string())?;
        let mut evidence: BTreeMap<_, _> = initial
            .evidence
            .iter()
            .cloned()
            .map(|e| (e.id.clone(), e))
            .collect();
        evidence.extend(later.into_iter().map(|e| (e.id.clone(), e)));
        Ok(Self {
            scenario: initial.scenario.clone(),
            sources: initial.sources.clone(),
            source_history: vec![],
            evidence,
            weather: BTreeMap::new(),
            receipts: BTreeSet::new(),
            initial,
            forecasts: vec![],
            drone_plans: vec![],
            briefings: vec![],
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        let at = self.scenario.current_at;
        let mut evidence: Vec<_> = self
            .evidence
            .values()
            .filter(|e| e.provenance.available_at <= at)
            .cloned()
            .collect();
        evidence.sort_by_key(|e| (e.provenance.available_at, e.id.clone()));
        let mut incidents: BTreeMap<String, Incident> = BTreeMap::new();
        for item in &evidence {
            let stable_key = format!(
                "{}:{}",
                item.provenance.source_id,
                item.provenance
                    .external_id
                    .as_deref()
                    .or_else(|| item
                        .attributes
                        .get("source_identity")
                        .and_then(serde_json::Value::as_str))
                    .unwrap_or(&item.id)
            );
            let seed = self
                .initial
                .incidents
                .iter()
                .find(|i| i.evidence_ids.contains(&item.id));
            let incident = incidents
                .entry(stable_key.clone())
                .or_insert_with(|| Incident {
                    id: seed.map_or_else(|| format!("incident-{stable_key}"), |i| i.id.clone()),
                    run_id: item.run_id.clone(),
                    mode: item.mode,
                    title: item.title.clone(),
                    location_name: item.location_name.clone(),
                    position: item.position,
                    uncertainty_m: item.uncertainty_m,
                    status: "reported".into(),
                    evidence_level: "reported".into(),
                    evidence_ids: vec![],
                    origin_groups: vec![],
                    first_event_at: item.provenance.event_at,
                    updated_at: item.provenance.available_at,
                });
            incident.evidence_ids.push(item.id.clone());
            if !incident
                .origin_groups
                .contains(&item.provenance.origin_group)
            {
                incident
                    .origin_groups
                    .push(item.provenance.origin_group.clone());
            }
            incident.updated_at = incident.updated_at.max(item.provenance.available_at);
            // Multiple copied reports never increase the evidence level. Independent
            // corroboration requires a reviewed link, not merely nearby geometry.
        }
        let mut sources = self.initial.sources.clone();
        let mut history: Vec<_> = self
            .source_history
            .iter()
            .filter(|(captured, _)| *captured <= at)
            .collect();
        history.sort_by_key(|(captured, _)| *captured);
        for (_, version) in history {
            if let Some(source) = sources.iter_mut().find(|s| s.id == version.id) {
                let last_success = source.last_success_at;
                *source = version.clone();
                // A delayed failure must not inherit a later successful capture.
                source.last_success_at = if matches!(
                    source.health,
                    SourceHealth::Healthy | SourceHealth::EmptyVerified
                ) {
                    source.captured_at
                } else {
                    last_success
                };
            }
        }
        Snapshot {
            scenario: self.scenario.clone(),
            incidents: incidents.into_values().collect(),
            evidence,
            weather: self
                .weather
                .values()
                .filter(|w| w.provenance.available_at <= at && w.valid_at <= at)
                .cloned()
                .collect(),
            sources,
            forecasts: self
                .forecasts
                .iter()
                .filter(|f| f.epoch == self.scenario.epoch)
                .cloned()
                .collect(),
            drone_plans: self
                .drone_plans
                .iter()
                .filter(|f| f.epoch == self.scenario.epoch)
                .cloned()
                .collect(),
            briefings: self
                .briefings
                .iter()
                .filter(|f| f.epoch == self.scenario.epoch)
                .cloned()
                .collect(),
        }
    }

    pub fn seek(&mut self, at: Timestamp) -> Result<(), String> {
        if at < self.scenario.start_at || at > self.scenario.end_at {
            return Err("seek outside scenario window".into());
        }
        self.scenario.current_at = at;
        self.scenario.epoch += 1;
        self.scenario.playing = false;
        self.forecasts.clear();
        self.drone_plans.clear();
        self.briefings.clear();
        Ok(())
    }

    pub fn ingest(&mut self, batch: SourceBatch) -> Result<usize, String> {
        if batch.run_id != self.scenario.run_id {
            return Err("batch belongs to another run".into());
        }
        for item in &batch.evidence {
            if item.run_id != batch.run_id
                || item.mode != self.scenario.mode
                || item.provenance.source_id != batch.source_id
            {
                return Err("evidence scope/lineage mismatch".into());
            }
            validate_position(item.position)?;
            if self
                .evidence
                .get(&item.id)
                .is_some_and(|prior| prior != item)
            {
                return Err("observation ID reused for different evidence; preserve a new immutable version".into());
            }
            if item.provenance.source_url.is_empty()
                || item.provenance.evidence_hash.is_empty()
                || item.provenance.origin_group.is_empty()
            {
                return Err("missing evidence provenance".into());
            }
        }
        for item in &batch.weather {
            if item.run_id != batch.run_id || item.provenance.source_id != batch.source_id {
                return Err("weather scope mismatch".into());
            }
            validate_position(item.position)?;
            if self
                .weather
                .get(&item.id)
                .is_some_and(|prior| prior != item)
            {
                return Err("observation ID reused for different weather".into());
            }
        }
        let receipt = format!("{}:{}:{}", batch.run_id, batch.source_id, batch.captured_at);
        // A failed retry may update health, but never deletes prior evidence.
        let source = self.sources.iter_mut().find(|s| s.id == batch.source_id);
        if let Some(source) = source {
            source.health = batch.health;
            source.coverage = batch.coverage.clone();
            source.captured_at = Some(batch.captured_at);
            if matches!(
                batch.health,
                SourceHealth::Healthy | SourceHealth::EmptyVerified
            ) {
                source.last_success_at = Some(batch.captured_at);
            }
            self.source_history
                .push((batch.captured_at, source.clone()));
        }
        // Deduplicate immutable observations, not an entire attempt: a failed or
        // partial capture may recover more records while retaining its timestamp.
        let mut inserted = 0;
        for item in batch.evidence {
            if !self.evidence.contains_key(&item.id) {
                self.evidence.insert(item.id.clone(), item);
                inserted += 1;
            }
        }
        for item in batch.weather {
            self.weather.entry(item.id.clone()).or_insert(item);
        }
        self.receipts.insert(receipt);
        Ok(inserted)
    }

    pub fn advance(&mut self, elapsed: Duration) {
        if self.scenario.playing && elapsed > Duration::zero() {
            let step_ms = (elapsed.num_milliseconds() as f64 * self.scenario.speed) as i64;
            self.scenario.current_at = (self.scenario.current_at + Duration::milliseconds(step_ms))
                .min(self.scenario.end_at);
            if self.scenario.current_at == self.scenario.end_at {
                self.scenario.playing = false;
            }
        }
    }

    pub fn play(&mut self, speed: f64) -> Result<(), String> {
        if !speed.is_finite() || !(0.1..=3600.0).contains(&speed) {
            return Err("speed must be between 0.1 and 3600".into());
        }
        self.scenario.speed = speed;
        self.scenario.playing = true;
        Ok(())
    }

    pub fn envelope(&self, sequence: u64, kind: &str, payload: serde_json::Value) -> EventEnvelope {
        EventEnvelope {
            id: format!(
                "{}:{}:{sequence}",
                self.scenario.run_id, self.scenario.epoch
            ),
            event_type: kind.into(),
            run_id: self.scenario.run_id.clone(),
            scenario_id: Some(self.scenario.id.clone()),
            epoch: self.scenario.epoch,
            event_time: self.scenario.current_at,
            emitted_at: Utc::now(),
            payload,
        }
    }
}

fn validate_position(position: Option<[f64; 2]>) -> Result<(), String> {
    if let Some([lon, lat]) = position
        && (!lon.is_finite()
            || !lat.is_finite()
            || !(PORTUGAL_BBOX[0]..=PORTUGAL_BBOX[2]).contains(&lon)
            || !(PORTUGAL_BBOX[1]..=PORTUGAL_BBOX[3]).contains(&lat))
    {
        return Err("position outside mainland Portugal bounding box".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests;
