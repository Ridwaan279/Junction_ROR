use crate::{
    fusion::{fuse, visible_evidence},
    store::Store,
};
use chrono::{DateTime, Duration, Utc};
use emberwatch_domain::platform::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
use tokio::sync::{Mutex, broadcast};
use uuid::Uuid;

const HISTORY_LIMIT: usize = 512;
const MAX_RECORDS: usize = 20_000;

fn content_digest(item: &Evidence) -> Result<Vec<u8>, PlatformError> {
    let mut content = item.clone();
    content.id = Uuid::nil();
    let fixed = DateTime::from_timestamp(0, 0).expect("epoch");
    content.captured_at = fixed;
    content.available_at = fixed;
    Ok(
        Sha256::digest(serde_json::to_vec(&content).map_err(|e| PlatformError::Storage(e.into()))?)
            .to_vec(),
    )
}

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0} not found")]
    NotFound(String),
    #[error("source cannot run until configured and validated")]
    SourceUnavailable,
    #[error("state storage failed")]
    Storage(#[source] anyhow::Error),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub sequence: u64,
    pub runs: BTreeMap<Uuid, Scenario>,
    pub sources: BTreeMap<String, SourceDescriptor>,
    pub events: VecDeque<EventEnvelope>,
}

#[derive(Clone)]
pub struct Platform {
    state: Arc<Mutex<Snapshot>>,
    store: Store,
    events: broadcast::Sender<EventEnvelope>,
    adapters: Arc<BTreeMap<String, Arc<dyn Source>>>,
    source_lock: Arc<Mutex<()>>,
}

pub struct StreamSubscription {
    pub backlog: Vec<EventEnvelope>,
    pub receiver: broadcast::Receiver<EventEnvelope>,
}

impl Platform {
    pub async fn open(store: Store, adapters: Vec<Arc<dyn Source>>) -> Result<Self, PlatformError> {
        let mut snapshot = store
            .load()
            .await
            .map_err(PlatformError::Storage)?
            .unwrap_or_default();
        // A restart does not unexpectedly resume time-dependent simulations.
        let mut restarted = Vec::new();
        for run in snapshot.runs.values_mut() {
            if run.playing {
                run.epoch += 1;
                restarted.push(run.id);
            }
            run.playing = false;
        }
        for id in restarted {
            let run = &snapshot.runs[&id];
            snapshot.sequence += 1;
            snapshot.events.push_back(EventEnvelope {
                id: snapshot.sequence, event_type: "run.reset".into(), run_id: id,
                scenario_id: Some(id), epoch: run.epoch, event_time: run.cursor,
                emitted_at: Utc::now(),
                payload: json!({"reason":"server_restart","snapshot_url":format!("/platform/api/runs/{id}")}),
            });
        }
        while snapshot.events.len() > HISTORY_LIMIT {
            snapshot.events.pop_front();
        }
        let now = Utc::now();
        snapshot
            .runs
            .entry(live_run_id())
            .or_insert_with(|| Scenario {
                id: live_run_id(),
                spec: ScenarioSpec {
                    name: "Live Portugal".into(),
                    mode: RunMode::Live,
                    bbox: PORTUGAL_BBOX,
                    starts_at: now,
                    ends_at: now + Duration::days(3650),
                    speed: 1.0,
                    seed: 0,
                },
                cursor: now,
                playing: false,
                epoch: 0,
                evidence: vec![],
                weather: vec![],
                incidents: vec![],
            });
        let mut registry = BTreeMap::new();
        for adapter in adapters {
            let mut descriptor = adapter.descriptor();
            if let Some(previous) = snapshot.sources.get(&descriptor.id)
                && descriptor.validated
                && previous.validated
            {
                descriptor.enabled = previous.enabled;
                descriptor.health = previous.health;
                descriptor.last_attempt = previous.last_attempt;
                descriptor.last_success = previous.last_success;
                descriptor.last_error = previous.last_error.clone();
                descriptor.coverage = previous.coverage.clone();
            }
            if !descriptor.validated {
                descriptor.enabled = false;
            }
            snapshot
                .sources
                .insert(descriptor.id.clone(), descriptor.clone());
            registry.insert(descriptor.id, adapter);
        }
        for source in snapshot.sources.values_mut() {
            if source.validated && !registry.contains_key(&source.id) {
                source.validated = false;
                source.enabled = false;
                source.health = SourceHealth::Unconfigured;
            }
        }
        for (id, name) in [
            ("fogos-context", "Fogos.pt via Context.dev"),
            ("ipma", "IPMA (Agent B adapter)"),
        ] {
            snapshot
                .sources
                .entry(id.into())
                .or_insert(SourceDescriptor {
                    id: id.into(),
                    name: name.into(),
                    fetch: if id == "ipma" {
                        "api".into()
                    } else {
                        "context_scrape".into()
                    },
                    origin_group: if id == "ipma" {
                        "ipma".into()
                    } else {
                        "anepc".into()
                    },
                    enabled: false,
                    cadence_s: if id == "ipma" { 3600 } else { 300 },
                    health: SourceHealth::NeedsValidation,
                    validated: false,
                    replayable: false,
                    last_attempt: None,
                    last_success: None,
                    last_error: None,
                    coverage: None,
                });
        }
        store
            .save(&snapshot)
            .await
            .map_err(PlatformError::Storage)?;
        let (events, _) = broadcast::channel(HISTORY_LIMIT);
        Ok(Self {
            state: Arc::new(Mutex::new(snapshot)),
            store,
            events,
            adapters: Arc::new(registry),
            source_lock: Arc::new(Mutex::new(())),
        })
    }

    pub fn storage_kind(&self) -> &'static str {
        self.store.kind()
    }

    async fn change<T>(
        &self,
        edit: impl FnOnce(&mut Snapshot) -> Result<(T, Vec<(Uuid, String, Value)>), PlatformError>,
    ) -> Result<T, PlatformError> {
        let mut current = self.state.lock().await;
        let mut next = current.clone();
        let (result, drafts) = edit(&mut next)?;
        let mut outgoing = Vec::new();
        for (run_id, event_type, payload) in drafts {
            let run = next
                .runs
                .get(&run_id)
                .ok_or_else(|| PlatformError::NotFound("run".into()))?;
            next.sequence += 1;
            let event = EventEnvelope {
                id: next.sequence,
                event_type,
                run_id,
                scenario_id: (run.spec.mode != RunMode::Live).then_some(run_id),
                epoch: run.epoch,
                event_time: run.cursor,
                emitted_at: Utc::now(),
                payload,
            };
            next.events.push_back(event.clone());
            outgoing.push(event);
        }
        while next.events.len() > HISTORY_LIMIT {
            next.events.pop_front();
        }
        self.store
            .save(&next)
            .await
            .map_err(PlatformError::Storage)?;
        *current = next;
        for event in outgoing {
            let _ = self.events.send(event);
        }
        Ok(result)
    }

    /// Public run view never exposes future evidence or weather from a replay.
    pub async fn run(&self, id: Uuid) -> Result<Scenario, PlatformError> {
        let state = self.state.lock().await;
        let mut run = state
            .runs
            .get(&id)
            .cloned()
            .ok_or_else(|| PlatformError::NotFound("run".into()))?;
        run.evidence = visible_evidence(&run.evidence, run.cursor);
        run.weather.retain(|w| w.available_at <= run.cursor);
        Ok(run)
    }

    pub async fn scenarios(&self) -> Vec<Scenario> {
        let ids: Vec<_> = self
            .state
            .lock()
            .await
            .runs
            .keys()
            .copied()
            .filter(|id| *id != live_run_id())
            .collect();
        let mut runs = Vec::new();
        for id in ids {
            if let Ok(run) = self.run(id).await {
                runs.push(run);
            }
        }
        runs
    }

    pub async fn load_dataset(&self, dataset: ScenarioDataset) -> Result<Scenario, PlatformError> {
        dataset.spec.validate().map_err(PlatformError::Invalid)?;
        if dataset.evidence.len() + dataset.weather.len() > MAX_RECORDS {
            return Err(PlatformError::Invalid(
                "dataset exceeds 20000 records".into(),
            ));
        }
        for item in &dataset.evidence {
            item.validate().map_err(PlatformError::Invalid)?;
        }
        for item in &dataset.weather {
            item.validate().map_err(PlatformError::Invalid)?;
        }
        let run = self.create_scenario(dataset.spec).await?;
        self.load_into(run.id, dataset.evidence, dataset.weather)
            .await?;
        self.run(run.id).await
    }

    pub async fn load_into(
        &self,
        id: Uuid,
        evidence: Vec<Evidence>,
        weather: Vec<WeatherSample>,
    ) -> Result<(), PlatformError> {
        let mut batches: BTreeMap<String, SourceBatch> = BTreeMap::new();
        for item in evidence {
            let batch = batches
                .entry(item.source_id.clone())
                .or_insert_with(|| SourceBatch {
                    source_id: item.source_id.clone(),
                    evidence: vec![],
                    weather: vec![],
                    coverage: Coverage {
                        complete: true,
                        explicit_empty: false,
                        note: Some("Loaded dataset".into()),
                    },
                    captured_at: Utc::now(),
                });
            batch.evidence.push(item);
        }
        for item in weather {
            let batch = batches
                .entry(item.source_id.clone())
                .or_insert_with(|| SourceBatch {
                    source_id: item.source_id.clone(),
                    evidence: vec![],
                    weather: vec![],
                    coverage: Coverage {
                        complete: true,
                        explicit_empty: false,
                        note: Some("Loaded dataset".into()),
                    },
                    captured_at: Utc::now(),
                });
            batch.weather.push(item);
        }
        for batch in batches.into_values() {
            self.ingest(id, batch).await?;
        }
        Ok(())
    }

    pub async fn sources(&self) -> Vec<SourceDescriptor> {
        self.state
            .lock()
            .await
            .sources
            .values()
            .cloned()
            .map(|mut source| {
                if matches!(
                    source.health,
                    SourceHealth::Healthy | SourceHealth::EmptyVerified
                ) && source.last_success.is_some_and(|at| {
                    (Utc::now() - at).num_seconds() > source.cadence_s.saturating_mul(2) as i64
                }) {
                    source.health = SourceHealth::Stale;
                }
                source
            })
            .collect()
    }

    pub async fn create_scenario(&self, spec: ScenarioSpec) -> Result<Scenario, PlatformError> {
        spec.validate().map_err(PlatformError::Invalid)?;
        let id = Uuid::new_v4();
        self.change(|state| {
            let run = Scenario {
                id,
                cursor: spec.starts_at,
                spec,
                playing: false,
                epoch: 0,
                evidence: vec![],
                weather: vec![],
                incidents: vec![],
            };
            state.runs.insert(id, run.clone());
            Ok((
                run,
                vec![(id, "run.reset".into(), json!({"reason":"created"}))],
            ))
        })
        .await
    }

    pub async fn ingest(&self, id: Uuid, batch: SourceBatch) -> Result<usize, PlatformError> {
        if batch.coverage.explicit_empty
            && (!batch.coverage.complete || !batch.evidence.is_empty() || !batch.weather.is_empty())
        {
            return Err(PlatformError::Invalid(
                "explicit_empty requires complete coverage with no records".into(),
            ));
        }
        if batch.evidence.len() + batch.weather.len() > MAX_RECORDS {
            return Err(PlatformError::Invalid("batch exceeds 20000 records".into()));
        }
        for evidence in &batch.evidence {
            evidence.validate().map_err(PlatformError::Invalid)?;
            if evidence.source_id != batch.source_id {
                return Err(PlatformError::Invalid(
                    "batch/evidence source mismatch".into(),
                ));
            }
        }
        for weather in &batch.weather {
            weather.validate().map_err(PlatformError::Invalid)?;
            if weather.source_id != batch.source_id {
                return Err(PlatformError::Invalid(
                    "batch/weather source mismatch".into(),
                ));
            }
        }
        self.change(|state| {
            let run = state
                .runs
                .get_mut(&id)
                .ok_or_else(|| PlatformError::NotFound("run".into()))?;
            if run.evidence.len() + run.weather.len() + batch.evidence.len() + batch.weather.len()
                > MAX_RECORDS * 5
            {
                return Err(PlatformError::Invalid(
                    "run exceeds its record limit".into(),
                ));
            }
            if run.spec.mode == RunMode::Live {
                run.cursor = Utc::now();
            }
            let previous = serde_json::to_value(&run.incidents)
                .map_err(|e| PlatformError::Storage(e.into()))?;
            let previous_weather: Vec<_> = run
                .weather
                .iter()
                .filter(|w| w.available_at <= run.cursor)
                .cloned()
                .collect();
            let mut added = 0;
            for mut item in batch.evidence {
                let digest = content_digest(&item)?;
                // Returning to older content after a different state is a new
                // observation. Coalesce only consecutive unchanged content.
                let identity = serde_json::to_vec(&(
                    &item.source_id,
                    &item.external_id,
                    item.captured_at,
                    item.available_at,
                    &digest,
                ))
                .map_err(|e| PlatformError::Storage(e.into()))?;
                item.id = Uuid::new_v5(&id, &identity);
                let previous = run
                    .evidence
                    .iter()
                    .filter(|old| {
                        old.source_id == item.source_id
                            && old.external_id == item.external_id
                            && old.available_at <= item.available_at
                    })
                    .max_by_key(|old| (old.available_at, old.id));
                let unchanged = previous
                    .map(content_digest)
                    .transpose()?
                    .is_some_and(|prior| prior == digest);
                if !run.evidence.iter().any(|old| old.id == item.id) && !unchanged {
                    if let Some(point) = item.point
                        && !point.in_bbox(run.spec.bbox)
                    {
                        continue;
                    }
                    run.evidence.push(item);
                    added += 1;
                }
            }
            for item in batch.weather {
                if !run.weather.iter().any(|old| {
                    old.source_id == item.source_id
                        && old.station_id == item.station_id
                        && old.valid_at == item.valid_at
                        && old.available_at == item.available_at
                }) {
                    run.weather.push(item);
                    added += 1;
                }
            }
            run.incidents = fuse(id, &run.evidence, run.cursor);
            let mut drafts = vec![];
            if added > 0 {
                drafts.push((
                    id,
                    "evidence.new".into(),
                    json!({"source_id":batch.source_id, "added":added}),
                ));
            }
            if previous
                != serde_json::to_value(&run.incidents)
                    .map_err(|e| PlatformError::Storage(e.into()))?
            {
                drafts.push((
                    id,
                    "incident.upsert".into(),
                    json!({"incidents":run.incidents}),
                ));
            }
            let visible_weather: Vec<_> = run
                .weather
                .iter()
                .filter(|w| w.available_at <= run.cursor)
                .cloned()
                .collect();
            if previous_weather != visible_weather {
                drafts.push((
                    id,
                    "weather.update".into(),
                    json!({"samples":visible_weather}),
                ));
            }
            if let Some(source) = state.sources.get_mut(&batch.source_id) {
                source.last_attempt = Some(batch.captured_at);
                source.last_success = Some(batch.captured_at);
                source.last_error = None;
                source.health = if !batch.coverage.complete {
                    SourceHealth::Partial
                } else if batch.coverage.explicit_empty && added == 0 {
                    SourceHealth::EmptyVerified
                } else {
                    SourceHealth::Healthy
                };
                source.coverage = Some(batch.coverage);
                drafts.push((id, "source.run".into(), json!(source)));
            }
            Ok((added, drafts))
        })
        .await
    }

    pub async fn play(&self, id: Uuid, speed: Option<f64>) -> Result<Scenario, PlatformError> {
        self.control(id, None, speed, true).await
    }
    pub async fn pause(&self, id: Uuid) -> Result<Scenario, PlatformError> {
        self.control(id, None, None, false).await
    }
    pub async fn seek(&self, id: Uuid, at: DateTime<Utc>) -> Result<Scenario, PlatformError> {
        self.control(id, Some(at), None, false).await
    }

    async fn control(
        &self,
        id: Uuid,
        at: Option<DateTime<Utc>>,
        speed: Option<f64>,
        playing: bool,
    ) -> Result<Scenario, PlatformError> {
        self.change(|state| {
            let run = state
                .runs
                .get_mut(&id)
                .ok_or_else(|| PlatformError::NotFound("scenario".into()))?;
            if run.spec.mode == RunMode::Live {
                return Err(PlatformError::Invalid(
                    "live clock cannot be controlled".into(),
                ));
            }
            if let Some(speed) = speed {
                if !speed.is_finite() || !(0.1..=3600.0).contains(&speed) {
                    return Err(PlatformError::Invalid(
                        "speed must be within 0.1..3600".into(),
                    ));
                }
                run.spec.speed = speed;
            }
            let reset = at.is_some();
            if let Some(at) = at {
                if at < run.spec.starts_at || at > run.spec.ends_at {
                    return Err(PlatformError::Invalid(
                        "seek outside scenario window".into(),
                    ));
                }
                run.cursor = at;
                run.epoch += 1;
            }
            run.playing = playing && run.cursor < run.spec.ends_at;
            run.incidents = fuse(id, &run.evidence, run.cursor);
            let event = if reset { "run.reset" } else { "scenario.tick" };
            Ok((
                (),
                vec![(
                    id,
                    event.into(),
                    json!({"cursor":run.cursor,"playing":run.playing}),
                )],
            ))
        })
        .await?;
        self.run(id).await
    }

    pub async fn advance(&self, elapsed: Duration) -> Result<(), PlatformError> {
        if elapsed < Duration::zero() {
            return Err(PlatformError::Invalid(
                "elapsed time cannot be negative".into(),
            ));
        }
        if !self.state.lock().await.runs.values().any(|r| r.playing) {
            return Ok(());
        }
        self.change(|state| {
            let mut drafts = Vec::new();
            for run in state
                .runs
                .values_mut()
                .filter(|r| r.playing && r.spec.mode != RunMode::Live)
            {
                let previous_cursor = run.cursor;
                let millis = (elapsed.num_milliseconds() as f64 * run.spec.speed).round() as i64;
                run.cursor = (run.cursor + Duration::milliseconds(millis)).min(run.spec.ends_at);
                run.playing = run.cursor < run.spec.ends_at;
                let previous = run.incidents.clone();
                run.incidents = fuse(run.id, &run.evidence, run.cursor);
                if previous != run.incidents {
                    drafts.push((
                        run.id,
                        "incident.upsert".into(),
                        json!({"incidents":run.incidents}),
                    ));
                }
                if run
                    .weather
                    .iter()
                    .any(|w| w.available_at > previous_cursor && w.available_at <= run.cursor)
                {
                    let weather: Vec<_> = run
                        .weather
                        .iter()
                        .filter(|w| w.available_at <= run.cursor)
                        .collect();
                    drafts.push((run.id, "weather.update".into(), json!({"samples":weather})));
                }
                drafts.push((
                    run.id,
                    "scenario.tick".into(),
                    json!({"cursor":run.cursor,"playing":run.playing}),
                ));
            }
            Ok(((), drafts))
        })
        .await
    }

    pub async fn set_source_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<SourceDescriptor, PlatformError> {
        self.change(|state| {
            let source = state
                .sources
                .get_mut(id)
                .ok_or_else(|| PlatformError::NotFound("source".into()))?;
            if enabled && (!source.validated || !self.adapters.contains_key(id)) {
                return Err(PlatformError::SourceUnavailable);
            }
            source.enabled = enabled;
            Ok((source.clone(), vec![]))
        })
        .await
    }

    pub async fn run_source(&self, id: &str) -> Result<SourceBatch, PlatformError> {
        // Manual and scheduled fetches share the same gate, including cadence.
        let _guard = self.source_lock.lock().await;
        let source = self
            .sources()
            .await
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| PlatformError::NotFound("source".into()))?;
        if !source.enabled || !source.validated {
            return Err(PlatformError::SourceUnavailable);
        }
        if source
            .last_attempt
            .is_some_and(|at| (Utc::now() - at).num_seconds() < source.cadence_s as i64)
        {
            return Err(PlatformError::Invalid(
                "source cadence has not elapsed".into(),
            ));
        }
        let adapter = self
            .adapters
            .get(id)
            .ok_or(PlatformError::SourceUnavailable)?;
        let now = Utc::now();
        // Reserve the attempt before networking, including malformed/failed results.
        self.change(|state| {
            let descriptor = state
                .sources
                .get_mut(id)
                .ok_or_else(|| PlatformError::NotFound("source".into()))?;
            descriptor.last_attempt = Some(now);
            Ok(((), vec![]))
        })
        .await?;
        let result = adapter
            .run(RunContext {
                run_id: live_run_id(),
                mode: RunMode::Live,
                bbox: PORTUGAL_BBOX,
                from: now - Duration::hours(24),
                now,
            })
            .await;
        let result = match result {
            Ok(batch) if batch.source_id != id => Err(SourceError::Invalid(
                "adapter returned a different source_id".into(),
            )),
            Ok(batch) => match self.ingest(live_run_id(), batch.clone()).await {
                Ok(_) => Ok(batch),
                Err(PlatformError::Storage(error)) => return Err(PlatformError::Storage(error)),
                Err(error) => Err(SourceError::Invalid(error.to_string())),
            },
            Err(error) => Err(error),
        };
        match result {
            Ok(batch) => Ok(batch),
            Err(error) => {
                self.change(|state| {
                    let source = state
                        .sources
                        .get_mut(id)
                        .ok_or_else(|| PlatformError::NotFound("source".into()))?;
                    source.last_attempt = Some(now);
                    source.health = if matches!(error, SourceError::Blocked) {
                        SourceHealth::Blocked
                    } else {
                        SourceHealth::Failed
                    };
                    source.last_error = Some(error.to_string());
                    Ok((
                        (),
                        vec![(live_run_id(), "source.run".into(), json!(source))],
                    ))
                })
                .await?;
                Err(PlatformError::SourceUnavailable)
            }
        }
    }

    pub async fn poll_due(&self) {
        for source in self
            .sources()
            .await
            .into_iter()
            .filter(|s| s.enabled && s.validated)
        {
            let due = source
                .last_attempt
                .is_none_or(|at| (Utc::now() - at).num_seconds() >= source.cadence_s as i64);
            if due && let Err(error) = self.run_source(&source.id).await {
                tracing::warn!(source_id = source.id, %error, "source run did not complete");
            }
        }
    }

    pub async fn subscribe(
        &self,
        run_id: Uuid,
        after: Option<u64>,
    ) -> Result<StreamSubscription, PlatformError> {
        let state = self.state.lock().await;
        let run = state
            .runs
            .get(&run_id)
            .ok_or_else(|| PlatformError::NotFound("run".into()))?;
        let receiver = self.events.subscribe();
        let earliest = state.events.front().map_or(state.sequence, |e| e.id);
        let reset = after.is_none_or(|id| id > state.sequence || id.saturating_add(1) < earliest);
        let backlog = if reset {
            vec![EventEnvelope {
                id: state.sequence,
                event_type: "run.reset".into(),
                run_id,
                scenario_id: (run.spec.mode != RunMode::Live).then_some(run_id),
                epoch: run.epoch,
                event_time: run.cursor,
                emitted_at: Utc::now(),
                payload: json!({"reason":"resync","snapshot_url":format!("/platform/api/runs/{run_id}")}),
            }]
        } else {
            state
                .events
                .iter()
                .filter(|event| event.run_id == run_id && event.id > after.unwrap_or(0))
                .cloned()
                .collect()
        };
        Ok(StreamSubscription { backlog, receiver })
    }
}
