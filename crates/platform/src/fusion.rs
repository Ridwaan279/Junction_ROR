use chrono::{DateTime, Duration, Utc};
use emberwatch_domain::platform::{
    Evidence, EvidenceKind, EvidenceLevel, Incident, OperationalStatus, Point,
};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub fn distance_m(a: Point, b: Point) -> f64 {
    let dlat = (b.latitude - a.latitude).to_radians();
    let dlon = (b.longitude - a.longitude).to_radians();
    let h = (dlat / 2.0).sin().powi(2)
        + a.latitude.to_radians().cos()
            * b.latitude.to_radians().cos()
            * (dlon / 2.0).sin().powi(2);
    6_371_000.0 * 2.0 * h.sqrt().min(1.0).asin()
}

/// Select the last available revision of each upstream record, without hindsight.
pub fn visible_evidence(evidence: &[Evidence], at: DateTime<Utc>) -> Vec<Evidence> {
    let mut latest: BTreeMap<(&str, &str), &Evidence> = BTreeMap::new();
    for item in evidence.iter().filter(|e| e.available_at <= at) {
        let key = (item.source_id.as_str(), item.external_id.as_str());
        if latest
            .get(&key)
            .is_none_or(|old| (item.available_at, item.id) > (old.available_at, old.id))
        {
            latest.insert(key, item);
        }
    }
    let mut result: Vec<_> = latest.values().map(|e| (*e).clone()).collect();
    result.sort_by_key(|e| (e.available_at, e.source_id.clone(), e.external_id.clone()));
    result
}

pub fn fuse(run_id: Uuid, evidence: &[Evidence], at: DateTime<Utc>) -> Vec<Incident> {
    let mut visible = visible_evidence(evidence, at);
    let mut first_available = BTreeMap::new();
    for item in evidence.iter().filter(|e| e.available_at <= at) {
        first_available
            .entry((item.source_id.as_str(), item.external_id.as_str()))
            .and_modify(|first: &mut DateTime<Utc>| *first = (*first).min(item.available_at))
            .or_insert(item.available_at);
    }
    // Revisions retain the original upstream anchor, independent of latest-capture order.
    visible.sort_by_key(|e| {
        (
            first_available[&(e.source_id.as_str(), e.external_id.as_str())],
            e.source_id.clone(),
            e.external_id.clone(),
        )
    });
    let mut groups: Vec<Vec<&Evidence>> = Vec::new();
    for item in &visible {
        let Some(point) = item.point else { continue };
        if item.kind == EvidenceKind::Context {
            continue;
        }
        let matching: Vec<usize> = groups
            .iter()
            .enumerate()
            .filter_map(|(index, group)| {
                let anchor = group[0];
                let anchor_point = anchor.point?;
                let radius = (anchor.radius_m + item.radius_m).clamp(500.0, 3_000.0);
                let item_time = item.observed_at.unwrap_or(item.available_at);
                let anchor_time = anchor.observed_at.unwrap_or(anchor.available_at);
                (distance_m(point, anchor_point) <= radius
                    && (item_time - anchor_time).abs() <= Duration::hours(6))
                .then_some(index)
            })
            .collect();
        // Ambiguous matches remain separate instead of joining two nearby fires.
        if matching.len() == 1 {
            groups[matching[0]].push(item);
        } else {
            groups.push(vec![item]);
        }
    }
    let mut incidents = Vec::new();
    for group in groups {
        let anchor = group[0];
        let origins: BTreeSet<_> = group.iter().map(|e| e.origin_group.clone()).collect();
        let reports: BTreeSet<_> = group
            .iter()
            .filter(|e| e.kind != EvidenceKind::Satellite)
            .map(|e| &e.origin_group)
            .collect();
        let independent_satellite = group.iter().any(|e| {
            e.kind == EvidenceKind::Satellite
                && reports.iter().any(|origin| **origin != e.origin_group)
        });
        let evidence_level = if independent_satellite {
            EvidenceLevel::SatelliteSupported
        } else if origins.len() >= 2 {
            EvidenceLevel::Corroborated
        } else {
            EvidenceLevel::Reported
        };
        let status = group
            .iter()
            .filter(|e| e.kind != EvidenceKind::Satellite && e.status != OperationalStatus::Unknown)
            .max_by_key(|e| (e.available_at, e.id))
            .map_or(OperationalStatus::Unknown, |e| e.status);
        let identity = format!("incident:{}:{}", anchor.source_id, anchor.external_id);
        incidents.push(Incident {
            id: Uuid::new_v5(&run_id, identity.as_bytes()),
            run_id,
            point: anchor.point.expect("only located evidence is grouped"),
            radius_m: group.iter().map(|e| e.radius_m).fold(0.0, f64::max),
            place_name: anchor.place_name.clone(),
            municipality: anchor.municipality.clone(),
            status,
            evidence_level,
            official_source: group.iter().any(|e| e.is_official),
            evidence_ids: group.iter().map(|e| e.id).collect(),
            origin_groups: origins.into_iter().collect(),
            first_seen: group
                .iter()
                .map(|e| first_available[&(e.source_id.as_str(), e.external_id.as_str())])
                .min()
                .expect("nonempty"),
            last_seen: group
                .iter()
                .map(|e| e.available_at)
                .max()
                .expect("nonempty"),
        });
    }
    incidents.sort_by_key(|incident| incident.id);
    incidents
}
