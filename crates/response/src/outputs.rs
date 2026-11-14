use crate::{ComputeError, Grid, Mission, SpreadResult};
use emberwatch_domain::{Briefing, DronePlan, Evidence, Forecast, Incident, RunContext, Weather};
use serde_json::json;

/// Local tangent-plane approximation for a bounded illustration. Its origin is
/// supplied explicitly from evidence; it is never a page-derived geocode.
#[derive(Clone, Copy, Debug)]
pub struct LocalProjection {
    pub origin_lon_lat: [f64; 2],
}

impl LocalProjection {
    pub fn lon_lat(self, east_m: f64, north_m: f64) -> Result<[f64; 2], ComputeError> {
        let [lon, lat] = self.origin_lon_lat;
        let bounds = emberwatch_domain::PORTUGAL_BBOX;
        if !lon.is_finite()
            || !lat.is_finite()
            || !(bounds[0]..=bounds[2]).contains(&lon)
            || !(bounds[1]..=bounds[3]).contains(&lat)
            || !east_m.is_finite()
            || !north_m.is_finite()
            || east_m.abs() > 60_000.0
            || north_m.abs() > 60_000.0
        {
            return Err(ComputeError::InvalidInput(
                "projection must use a Portugal origin and offsets within 60 km".into(),
            ));
        }
        const EARTH_RADIUS_M: f64 = 6_371_008.8;
        let point = [
            lon + (east_m / (EARTH_RADIUS_M * lat.to_radians().cos())).to_degrees(),
            lat + (north_m / EARTH_RADIUS_M).to_degrees(),
        ];
        if !(bounds[0]..=bounds[2]).contains(&point[0])
            || !(bounds[1]..=bounds[3]).contains(&point[1])
        {
            return Err(ComputeError::InvalidInput(
                "projected geometry outside mainland Portugal bounding box".into(),
            ));
        }
        Ok(point)
    }
}

pub fn build_forecast(
    context: &RunContext,
    incident: &Incident,
    grid: &Grid,
    projection: LocalProjection,
    spread: &SpreadResult,
    evidence: &[Evidence],
    weather: &[Weather],
) -> Result<Forecast, ComputeError> {
    validate_inputs(context, incident, evidence, weather)?;
    if spread.arrival_seconds.len() != grid.cells().len()
        || spread.burned_cells.iter().any(|&p| grid.index(p).is_none())
    {
        return Err(ComputeError::InvalidInput(
            "spread result does not match grid".into(),
        ));
    }
    let mut features = Vec::new();
    for (index, arrival) in spread.arrival_seconds.iter().enumerate() {
        let Some(seconds) = arrival else {
            continue;
        };
        if *seconds > spread.horizon_seconds {
            return Err(ComputeError::InvalidInput("arrival beyond horizon".into()));
        }
        let cell = grid.position(index);
        let x = cell.x as f64 * grid.cell_size_m;
        let y = cell.y as f64 * grid.cell_size_m;
        let s = grid.cell_size_m;
        let ring = vec![
            projection.lon_lat(x, y)?,
            projection.lon_lat(x + s, y)?,
            projection.lon_lat(x + s, y + s)?,
            projection.lon_lat(x, y + s)?,
            projection.lon_lat(x, y)?,
        ];
        features.push(
            json!({"type":"Feature", "geometry":{"type":"Polygon","coordinates":[ring]},
            "properties":{"arrival_seconds":seconds,"illustrative":true,"seed":spread.seed}}),
        );
    }
    let mut evidence_ids: Vec<_> = evidence.iter().map(|e| e.id.clone()).collect();
    let mut weather_ids: Vec<_> = weather.iter().map(|w| w.id.clone()).collect();
    evidence_ids.sort();
    evidence_ids.dedup();
    weather_ids.sort();
    weather_ids.dedup();
    Ok(Forecast { id: format!("forecast-{}-{}-{}", incident.id, context.epoch, spread.seed), run_id: context.run_id.clone(),
        incident_id: incident.id.clone(), epoch: context.epoch, generated_at: context.clock.now(), input_evidence_ids: evidence_ids,
        input_weather_ids: weather_ids, model_version: "illustrative-cellular-v1".into(), seed: spread.seed,
        horizon_seconds: spread.horizon_seconds, illustrative: true, geometry: json!({"type":"FeatureCollection","features":features}),
        limitations: vec!["Uncalibrated illustrative cellular simulation; not a measured perimeter or operational forecast.".into(),
            "Cell arrival times are one seeded realization, not probabilities or confidence bounds.".into(),
            "Local tangent-plane geometry is approximate; no terrain clearance or national coverage guarantee.".into()] })
}

pub fn build_drone_plan(
    context: &RunContext,
    incident: &Incident,
    grid: &Grid,
    projection: LocalProjection,
    mission: &Mission,
    reserve_pct: f64,
    reasons: Vec<String>,
) -> Result<DronePlan, ComputeError> {
    validate_inputs(context, incident, &[], &[])?;
    if !reserve_pct.is_finite()
        || !(0.0..=100.0).contains(&reserve_pct)
        || mission.battery_after_pct < reserve_pct
        || !mission.total_seconds.is_finite()
        || mission.total_seconds < 0.0
    {
        return Err(ComputeError::InvalidInput(
            "invalid mission reserve or duration".into(),
        ));
    }
    let to_coords = |cells: &[crate::CellPosition]| -> Result<Vec<[f64; 2]>, ComputeError> {
        if cells.is_empty() {
            return Err(ComputeError::InvalidInput("route is empty".into()));
        }
        cells
            .iter()
            .map(|&p| {
                if grid.cell(p).is_none_or(|c| c.blocked) {
                    return Err(ComputeError::InvalidInput(
                        "route outside grid or blocked".into(),
                    ));
                }
                projection.lon_lat(
                    (p.x as f64 + 0.5) * grid.cell_size_m,
                    (p.y as f64 + 0.5) * grid.cell_size_m,
                )
            })
            .collect()
    };
    let mut reasons = reasons;
    reasons.push(
        "Simulated mission; outbound task, independent return route and battery reserve checked."
            .into(),
    );
    reasons.push("Configured obstacles are illustrative, not a complete aviation or terrain-clearance model.".into());
    Ok(DronePlan {
        id: format!("plan-{}-{}", incident.id, context.epoch),
        run_id: context.run_id.clone(),
        incident_id: incident.id.clone(),
        epoch: context.epoch,
        generated_at: context.clock.now(),
        simulated: true,
        route: to_coords(&mission.outbound.cells)?,
        return_route: to_coords(&mission.returning.cells)?,
        distance_m: mission.outbound.distance_m + mission.returning.distance_m,
        duration_seconds: mission.total_seconds,
        reserve_pct,
        reasons,
    })
}

pub fn build_briefing(
    context: &RunContext,
    incident: &Incident,
    evidence: &[Evidence],
    plan: Option<&DronePlan>,
) -> Result<Briefing, ComputeError> {
    validate_inputs(context, incident, evidence, &[])?;
    if let Some(plan) = plan {
        if plan.run_id != context.run_id
            || plan.incident_id != incident.id
            || plan.epoch != context.epoch
            || plan.generated_at > context.clock.now()
        {
            return Err(ComputeError::InvalidInput(
                "plan belongs to another run, incident, epoch or future time".into(),
            ));
        }
    }
    let mut citations: Vec<_> = evidence
        .iter()
        .map(|e| e.provenance.source_url.clone())
        .collect();
    citations.sort();
    citations.dedup();
    let source_note_en = if citations.is_empty() {
        "No source citations supplied."
    } else {
        "Review the supplied source citations and their uncertainty."
    };
    let source_note_pt = if citations.is_empty() {
        "Sem fontes citadas fornecidas."
    } else {
        "Consulte as fontes citadas e a sua incerteza."
    };
    let route_en = plan
        .map(|p| {
            format!(
                " Simulated round-trip task: {:.0} m, {:.0} seconds, configured reserve {:.0}%.",
                p.distance_m, p.duration_seconds, p.reserve_pct
            )
        })
        .unwrap_or_default();
    let route_pt = plan.map(|p| format!(" Missão simulada de ida e volta: {:.0} m, {:.0} segundos, reserva configurada {:.0}%.", p.distance_m, p.duration_seconds, p.reserve_pct)).unwrap_or_default();
    Ok(Briefing {
        id: format!("briefing-{}-{}", incident.id, context.epoch),
        run_id: context.run_id.clone(),
        incident_id: incident.id.clone(),
        epoch: context.epoch,
        generated_at: context.clock.now(),
        text_pt: format!(
            "Simulação — {}. Estado da fonte: {}; nível de evidência: {}. {}{} Não constitui orientação operacional.",
            incident.title, incident.status, incident.evidence_level, source_note_pt, route_pt
        ),
        text_en: format!(
            "Simulation — {}. Source status: {}; evidence level: {}. {}{} This is not operational guidance.",
            incident.title, incident.status, incident.evidence_level, source_note_en, route_en
        ),
        citations,
        audio_url: None,
        simulated: true,
    })
}

fn validate_inputs(
    context: &RunContext,
    incident: &Incident,
    evidence: &[Evidence],
    weather: &[Weather],
) -> Result<(), ComputeError> {
    let now = context.clock.now();
    if incident.run_id != context.run_id
        || incident.mode != context.mode
        || incident.updated_at > now
        || evidence.iter().any(|e| {
            e.run_id != context.run_id
                || e.mode != context.mode
                || e.provenance.available_at > now
                || e.provenance.event_at.is_some_and(|t| t > now)
                || e.provenance.published_at.is_some_and(|t| t > now)
        })
        || weather.iter().any(|w| {
            w.run_id != context.run_id
                || w.provenance.available_at > now
                || (!w.forecast && w.valid_at > now)
        })
    {
        return Err(ComputeError::InvalidInput(
            "cross-run or future inputs cannot enter response outputs".into(),
        ));
    }
    Ok(())
}
