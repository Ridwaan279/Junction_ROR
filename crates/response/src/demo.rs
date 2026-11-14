use crate::ComputeError;
use crate::{
    Cell, CellPosition, Grid, LocalProjection, MissionRequest, SpreadInput, WindVector,
    build_briefing, build_drone_plan, build_forecast, plan_mission, simulate_spread,
};
use emberwatch_domain::{Briefing, DronePlan, Evidence, Forecast, Incident, RunContext, Weather};

/// Reproducible synthetic training bundle. This helper refuses live/historical
/// runs; real-data callers must provide explicit grids/model inputs instead.
pub fn demo_bundle(
    context: &RunContext,
    incident: &Incident,
    evidence: &[Evidence],
    weather: &[Weather],
) -> Result<(Forecast, DronePlan, Briefing), ComputeError> {
    if context.mode != emberwatch_domain::DataMode::Synthetic || incident.mode != context.mode {
        return Err(ComputeError::InvalidInput(
            "demo_bundle only accepts explicitly synthetic runs".into(),
        ));
    }
    let center = incident.position.ok_or_else(|| {
        ComputeError::InvalidInput("synthetic incident needs a labelled location".into())
    })?;
    // Validate scope/timestamps before choosing any demonstration weather.
    let _ = build_briefing(context, incident, evidence, None)?;
    let now = context.clock.now();
    if weather
        .iter()
        .any(|w| w.run_id != context.run_id || w.provenance.available_at > now)
    {
        return Err(ComputeError::InvalidInput(
            "cross-run or future weather".into(),
        ));
    }
    let selected = weather.iter().find(|w| {
        w.valid_at <= now
            && now.signed_duration_since(w.valid_at).num_seconds() <= 7200
            && w.wind_speed_mps.is_some()
            && w.wind_from_deg.is_some()
    });
    let wind = if let Some(w) = selected {
        WindVector::from_meteorological(
            w.wind_speed_mps.expect("selected wind"),
            w.wind_from_deg.expect("selected bearing"),
        )?
    } else {
        WindVector {
            east_mps: 4.0,
            north_mps: 0.0,
        }
    };
    let humidity = selected
        .and_then(|w| w.relative_humidity_pct)
        .unwrap_or(20.0)
        / 100.0;
    let mut cells = vec![
        Cell {
            humidity,
            ..Cell::default()
        };
        21 * 21
    ];
    // Explicit synthetic obstacle; not asserted to be real airspace or terrain.
    cells[10 * 21 + 6].blocked = true;
    cells[10 * 21 + 6].fuel = 0.0;
    let grid = Grid::new(21, 21, 500.0, cells)?;
    const R: f64 = 6_371_008.8;
    let projection = LocalProjection {
        origin_lon_lat: [
            center[0] - (5250.0 / (R * center[1].to_radians().cos())).to_degrees(),
            center[1] - (5250.0 / R).to_degrees(),
        ],
    };
    let spread = simulate_spread(
        &grid,
        &SpreadInput {
            seed: 760,
            ignitions: vec![CellPosition { x: 10, y: 10 }],
            steps: 8,
            step_seconds: 60,
            base_probability: 0.3,
            wind,
        },
    )?;
    let used_weather: Vec<_> = selected.into_iter().cloned().collect();
    let mut forecast = build_forecast(
        context,
        incident,
        &grid,
        projection,
        &spread,
        evidence,
        &used_weather,
    )?;
    forecast.limitations.push(
        "Demo seed 760; fuel, terrain, obstacle, model parameters and depot are synthetic.".into(),
    );
    if selected.is_none() {
        forecast.limitations.push("No fresh complete wind input: explicitly synthetic 4 m/s west wind and 20% humidity used.".into());
    } else if selected.is_some_and(|w| w.relative_humidity_pct.is_none()) {
        forecast.limitations.push(
            "Missing humidity replaced with explicitly synthetic 20% for this training run.".into(),
        );
    }
    let req = MissionRequest {
        start: CellPosition { x: 2, y: 10 },
        target: CellPosition { x: 10, y: 10 },
        wind,
        cruise_mps: 12.0,
        max_gust_mps: 25.0,
        gust_mps: selected.and_then(|w| w.gust_mps),
        battery_capacity_seconds: 3600.0,
        battery_pct: 100.0,
        reserve_pct: 20.0,
        task_seconds: 120.0,
        climb_mps: 3.0,
    };
    let mission = plan_mission(&grid, &req)?;
    let plan = build_drone_plan(
        context,
        incident,
        &grid,
        projection,
        &mission,
        req.reserve_pct,
        vec![
            "Synthetic training depot/obstacle; configured 12 m/s cruise and 120-second task."
                .into(),
            if req.gust_mps.is_none() {
                "Gust is unknown; no measured-gust safety claim.".into()
            } else {
                "Supplied gust checked against the synthetic mission limit.".into()
            },
        ],
    )?;
    let briefing = build_briefing(context, incident, evidence, Some(&plan))?;
    Ok((forecast, plan, briefing))
}
