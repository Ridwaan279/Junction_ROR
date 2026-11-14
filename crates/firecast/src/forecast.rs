//! Run the physics on an assembled environment and publish GeoJSON map layers plus
//! a structured summary. Pure CPU work: call from a blocking worker.
use crate::FirecastError;
use crate::contour::{contours, simplify};
use crate::environment::{Basis, FireEnvironment, HORIZON_H};
use crate::fuel::LandCover;
use crate::geo::{LocalFrame, bearing_delta_deg, bearing_of, from_to_toward_deg, norm_deg};
use crate::moisture::{MoistureInputs, estimate};
use crate::rothermel::suppression_class;
use crate::smoke::{
    CONVECTIVE_FRACTION, PM25_EMISSION_FACTOR_G_PER_KG, PlumeMet, PlumeSource, concentration,
    stability_class, summarise,
};
use crate::spread::{
    EnsembleConfig, EnsembleResult, HourConditions, Landscape, beds_for, cell_spread, run_ensemble,
};
use crate::{solar, weather::HourWeather};
use chrono::{DateTime, Utc};
use emberwatch_domain::{Evidence, Forecast, Incident, RunContext, Weather};
use serde_json::{Value, json};

pub const MODEL_VERSION: &str = "firecast-v1 (Rothermel–Albini surface spread, Anderson ellipse, MTT ensemble; Briggs/Pasquill Gaussian plume)";
pub const SMOKE_THRESHOLDS_UG_M3: [f64; 3] = [35.0, 150.0, 500.0];
pub const PROBABILITIES: [f64; 3] = [0.1, 0.5, 0.9];

#[derive(Clone, Debug)]
pub struct FirecastConfig {
    pub members: u32,
    pub seed: u64,
    pub horizons_h: Vec<u32>,
    pub smoke_cells: usize,
    pub smoke_cell_m: f64,
}

impl Default for FirecastConfig {
    fn default() -> Self {
        Self {
            members: 32,
            seed: 760,
            horizons_h: vec![1, 3, 6],
            smoke_cells: 121,
            smoke_cell_m: 500.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SmokeField {
    pub hour: u32,
    pub frame: LocalFrame,
    pub size: usize,
    pub cell_m: f64,
    pub values: Vec<f64>,
}

impl SmokeField {
    /// Concentration (µg/m³) at a lon/lat point, nearest cell; 0 outside.
    pub fn at(&self, lon_lat: [f64; 2]) -> f64 {
        let (x, y) = self.frame.to_local(lon_lat);
        let (i, j) = ((x / self.cell_m).floor(), (y / self.cell_m).floor());
        if i < 0.0 || j < 0.0 || i >= self.size as f64 || j >= self.size as f64 {
            return 0.0;
        }
        self.values[j as usize * self.size + i as usize]
    }
}

#[derive(Clone, Debug)]
pub struct FirecastResult {
    pub geojson: Value,
    pub summary: Value,
    pub ensemble: EnsembleResult,
    pub frame: LocalFrame,
    pub cell_m: f64,
    pub elevation_m: Vec<f32>,
    pub smoke: Vec<SmokeField>,
    pub ignition_local: (f64, f64),
    pub start: DateTime<Utc>,
    pub head_bearing_deg: f64,
    pub transport_toward_deg: f64,
    pub transport_mps: f64,
    pub gust_mps: Option<f64>,
    pub limitations: Vec<String>,
    pub input_urls: Vec<String>,
}

/// Bilinear wind on the spread grid from a coarse lattice of field samples.
fn wind_grid(env: &FireEnvironment, t: DateTime<Utc>) -> Vec<(f32, f32)> {
    const N: usize = 17;
    let span = env.width as f64 * env.cell_m;
    let lattice: Vec<(f64, f64)> = (0..N * N)
        .map(|k| {
            let (i, j) = (k % N, k / N);
            let p = env.frame.to_lon_lat(
                i as f64 / (N - 1) as f64 * span,
                j as f64 / (N - 1) as f64 * span,
            );
            env.weather
                .sample(p, t)
                .map_or((0.0, 0.0), |h| h.wind_vector())
        })
        .collect();
    let mut out = Vec::with_capacity(env.width * env.height);
    for y in 0..env.height {
        for x in 0..env.width {
            let fx = (x as f64 + 0.5) / env.width as f64 * (N - 1) as f64;
            let fy = (y as f64 + 0.5) / env.height as f64 * (N - 1) as f64;
            let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
            let (x1, y1) = ((x0 + 1).min(N - 1), (y0 + 1).min(N - 1));
            let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
            let v = |i: usize, j: usize| lattice[j * N + i];
            let mix = |f: fn((f64, f64)) -> f64| {
                (f(v(x0, y0)) * (1.0 - tx) + f(v(x1, y0)) * tx) * (1.0 - ty)
                    + (f(v(x0, y1)) * (1.0 - tx) + f(v(x1, y1)) * tx) * ty
            };
            out.push((mix(|w| w.0) as f32, mix(|w| w.1) as f32));
        }
    }
    out
}

fn ring_to_lon_lat(frame: &LocalFrame, cell_m: f64, ring: &[(f64, f64)]) -> Vec<[f64; 2]> {
    let mut coords: Vec<[f64; 2]> = simplify(ring, 0.35)
        .iter()
        .map(|(i, j)| {
            let p = frame.to_lon_lat((i + 0.5) * cell_m, (j + 0.5) * cell_m);
            [(p[0] * 1e6).round() / 1e6, (p[1] * 1e6).round() / 1e6]
        })
        .collect();
    if let Some(first) = coords.first().copied() {
        coords.push(first);
    }
    coords
}

fn multipolygon(
    frame: &LocalFrame,
    cell_m: f64,
    field: &[f64],
    size: usize,
    threshold: f64,
) -> Option<Value> {
    let polys = contours(field, size, size, threshold);
    if polys.is_empty() {
        return None;
    }
    let coords: Vec<Vec<Vec<[f64; 2]>>> = polys
        .iter()
        .map(|p| {
            p.iter()
                .map(|r| ring_to_lon_lat(frame, cell_m, r))
                .filter(|r| r.len() >= 4)
                .collect::<Vec<_>>()
        })
        .filter(|p: &Vec<Vec<[f64; 2]>>| !p.is_empty())
        .collect();
    (!coords.is_empty()).then(|| json!({"type":"MultiPolygon","coordinates":coords}))
}

fn line(frame: &LocalFrame, a: (f64, f64), b: (f64, f64)) -> Value {
    json!({"type":"LineString","coordinates":[frame.to_lon_lat(a.0, a.1), frame.to_lon_lat(b.0, b.1)]})
}

fn round(v: f64, d: i32) -> f64 {
    let f = 10f64.powi(d);
    (v * f).round() / f
}

pub fn compute(
    env: &FireEnvironment,
    config: &FirecastConfig,
) -> Result<FirecastResult, FirecastError> {
    let started = std::time::Instant::now();
    let (w, h, cell) = (env.width, env.height, env.cell_m);
    let start = env.request.start;
    let now = env.request.now.max(start);
    // Simulate from the estimated ignition (first report) up to 6 h past "now".
    let elapsed = ((now - start).num_seconds() as f64).clamp(0.0, HORIZON_H as f64 * 3600.0);
    let total_hours = ((elapsed / 3600.0).ceil() as i64) + HORIZON_H;
    let ignition = env.request.ignition;
    let landscape = Landscape {
        width: w,
        height: h,
        cell_m: cell,
        cover: env.cover.cells.clone(),
        slope: (0..w * h).map(|k| env.dem.slope(k % w, k / w)).collect(),
    };
    let adj = &env.adjustments;
    let mut moisture_basis = Vec::new();
    let mut hours = Vec::new();
    let mut ignition_weather = Vec::new();
    for k in 0..=total_hours {
        let t = start + chrono::Duration::hours(k);
        let wx = env
            .weather
            .sample(ignition, t)
            .ok_or_else(|| FirecastError::Compute("no weather at ignition".into()))?;
        let m = estimate(&MoistureInputs {
            time: t,
            temp_c: wx.temp_c,
            rh_pct: wx.rh_pct,
            precip_last_3h_mm: env.weather.precip_before(ignition, t, 3),
            vpd_kpa: wx.vpd_kpa,
            drought_live_reduction_pct: adj
                .drought_live_reduction
                .as_ref()
                .map_or(0.0, |s| s.value),
            drought_label: adj.drought_label.as_deref(),
        });
        if k == (elapsed / 3600.0) as i64 {
            moisture_basis = m.basis.clone();
        }
        hours.push(HourConditions {
            wind: wind_grid(env, t),
            moisture: m.moisture,
        });
        ignition_weather.push(wx);
    }
    let ignition_local = env.frame.to_local(ignition);
    let sigma = (env.request.uncertainty_m / 2.0).clamp(0.0, 750.0);
    let horizon_s = elapsed + HORIZON_H as f64 * 3600.0;
    let now_hour = ((elapsed / 3600.0) as usize).min(total_hours as usize);
    let ensemble_cfg = EnsembleConfig::new(
        ignition_local,
        sigma,
        horizon_s,
        config.members,
        config.seed,
    );
    let ensemble =
        run_ensemble(&landscape, &hours, &ensemble_cfg).map_err(FirecastError::Compute)?;
    let central = &ensemble.members[0];
    let beds0 = beds_for(&hours, &central.perturbation);
    let head = cell_spread(
        &landscape,
        &hours,
        &beds0,
        &central.perturbation,
        now_hour,
        central.ignition_cell,
    );
    let ignition_cover = landscape.cover[central.ignition_cell];

    let mut features: Vec<Value> = Vec::new();
    let mut horizon_stats = Vec::new();
    let mut run_bearing_3h = None;
    // Horizons are hours after "now"; 0 = modelled current extent when the fire is not new.
    let mut offsets: Vec<u32> = config.horizons_h.clone();
    if elapsed >= 900.0 {
        offsets.insert(0, 0);
    }
    for &hz in &offsets {
        let t = elapsed + f64::from(hz) * 3600.0;
        let p = ensemble.burn_probability(t);
        // Raw ensemble field: smoothing would erode young, small fires.
        for prob in PROBABILITIES {
            if let Some(geometry) = multipolygon(&env.frame, cell, &p, w, prob) {
                features.push(json!({"type":"Feature","geometry":geometry,
                    "properties":{"layer":"spread","horizon_h":hz,"probability":prob}}));
            }
        }
        let areas = ensemble.areas_ha(t);
        let q = |f: f64| areas[((areas.len() - 1) as f64 * f).round() as usize];
        let (mut sx, mut sy, mut n, mut far) = (0.0, 0.0, 0.0, 0.0f64);
        for (k, v) in p.iter().enumerate() {
            if *v >= 0.5 {
                let (x, y) = (((k % w) as f64 + 0.5) * cell, ((k / w) as f64 + 0.5) * cell);
                sx += x;
                sy += y;
                n += 1.0;
                far = far.max((x - ignition_local.0).hypot(y - ignition_local.1));
            }
        }
        let run_bearing = (n > 0.0 && far > cell)
            .then(|| bearing_of(sx / n - ignition_local.0, sy / n - ignition_local.1).to_degrees());
        if hz == 3 || run_bearing_3h.is_none() {
            run_bearing_3h = run_bearing.or(run_bearing_3h);
        }
        horizon_stats.push(json!({"horizon_h":hz,"area_ha_p10":round(q(0.1),1),"area_ha_p50":round(q(0.5),1),
            "area_ha_p90":round(q(0.9),1),"max_run_km_p50":round(far/1000.0,2),"run_bearing_deg":run_bearing.map(|b| round(b,0))}));
    }

    // Smoke at each horizon from the central member's fire state.
    let mut smoke_fields = Vec::new();
    let mut smoke_stats = Vec::new();
    let cell_area = cell * cell;
    let mut first_transport = None;
    for &hz in &offsets {
        let t = elapsed + f64::from(hz) * 3600.0;
        let window = 1800.0f64.min(t);
        let burned: Vec<usize> = (0..w * h)
            .filter(|k| f64::from(central.arrival[*k]) <= t)
            .collect();
        let front: Vec<usize> = burned
            .iter()
            .copied()
            .filter(|k| f64::from(central.arrival[*k]) > t - window)
            .collect();
        let area = burned.len() as f64 * cell_area;
        let growth = front.len() as f64 * cell_area / window;
        let hour = ((t / 3600.0) as usize).min(hours.len() - 1);
        let (mut hpa, mut consumed) = (0.0, 0.0);
        for k in &front {
            if let Some(bed) = beds0[hour][usize::from(landscape.cover[*k].index())] {
                hpa += bed.heat_per_area * 11.3565; // BTU/ft² → kJ/m²
                consumed += bed.consumed_kg_m2;
            }
        }
        let nf = front.len().max(1) as f64;
        let (hpa, consumed) = (hpa / nf, consumed / nf);
        let (cx, cy) = if burned.is_empty() {
            ignition_local
        } else {
            let n = burned.len() as f64;
            (
                burned
                    .iter()
                    .map(|k| ((k % w) as f64 + 0.5) * cell)
                    .sum::<f64>()
                    / n,
                burned
                    .iter()
                    .map(|k| ((k / w) as f64 + 0.5) * cell)
                    .sum::<f64>()
                    / n,
            )
        };
        let source_ll = env.frame.to_lon_lat(cx, cy);
        let at = start + chrono::Duration::seconds(t as i64);
        let Some(wx) = env.weather.sample(source_ll, at) else {
            continue;
        };
        let (elev, _) = solar::position(source_ll, at);
        let stability = stability_class(wx.wind_mps, elev, wx.cloud_pct, wx.shortwave_wm2);
        let (transport_mps, transport_from, transport_basis) =
            match (wx.wind_upper_mps, wx.wind_upper_from_deg) {
                (Some(s), Some(d)) => (s, d, "120 m forecast wind"),
                _ => (
                    wx.wind_mps * 12f64.powf(stability.wind_exponent()),
                    wx.wind_from_deg,
                    "10 m wind, power-law to 120 m",
                ),
            };
        let toward = from_to_toward_deg(transport_from);
        first_transport.get_or_insert((toward, transport_mps, wx.gust_mps));
        let source = PlumeSource {
            x: cx,
            y: cy,
            radius_m: (area / std::f64::consts::PI).sqrt().max(cell / 2.0),
            emission_g_s: growth * consumed * PM25_EMISSION_FACTOR_G_PER_KG,
            heat_release_w: hpa * 1000.0 * growth,
            age_s: t,
        };
        let met = PlumeMet {
            wind10_mps: wx.wind_mps,
            transport_mps,
            transport_toward: toward.to_radians(),
            stability,
            mixing_height_m: stability.default_mixing_height_m(),
        };
        let plume = summarise(&source, &met);
        let size = config.smoke_cells;
        let extent = size as f64 * config.smoke_cell_m;
        let centre_local = (
            cx + 0.35 * extent * met.transport_toward.sin(),
            cy + 0.35 * extent * met.transport_toward.cos(),
        );
        let smoke_frame = LocalFrame::centred(
            env.frame.to_lon_lat(centre_local.0, centre_local.1),
            extent / 2.0,
        );
        let (sx0, sy0) = smoke_frame.to_local(source_ll);
        let local_source = PlumeSource {
            x: sx0,
            y: sy0,
            ..source
        };
        let values: Vec<f64> = (0..size * size)
            .map(|k| {
                let (px, py) = (
                    ((k % size) as f64 + 0.5) * config.smoke_cell_m,
                    ((k / size) as f64 + 0.5) * config.smoke_cell_m,
                );
                concentration(&local_source, &met, &plume, px, py)
            })
            .collect();
        let mut reach_km = Vec::new();
        for threshold in SMOKE_THRESHOLDS_UG_M3 {
            let far = (0..size * size)
                .filter(|k| values[*k] >= threshold)
                .map(|k| {
                    let (px, py) = (
                        ((k % size) as f64 + 0.5) * config.smoke_cell_m,
                        ((k / size) as f64 + 0.5) * config.smoke_cell_m,
                    );
                    (px - sx0).hypot(py - sy0)
                })
                .fold(0.0f64, f64::max);
            reach_km.push(json!({"threshold_ug_m3":threshold,"reach_km":round(far/1000.0,1)}));
            if let Some(geometry) =
                multipolygon(&smoke_frame, config.smoke_cell_m, &values, size, threshold)
            {
                features.push(json!({"type":"Feature","geometry":geometry,
                    "properties":{"layer":"smoke","hour_offset":hz,"threshold_ug_m3":threshold}}));
            }
        }
        let axis_len = plume.max_length_m.min(0.6 * extent);
        features.push(json!({"type":"Feature","geometry":line(&env.frame, (cx, cy), (cx + axis_len * met.transport_toward.sin(), cy + axis_len * met.transport_toward.cos())),
            "properties":{"layer":"smoke_axis","hour_offset":hz,"toward_deg":round(toward,0),"transport_mps":round(transport_mps,1)}}));
        smoke_stats.push(json!({"hour_offset":hz,"valid_at":at,"stability":stability.label(),"solar_elevation_deg":round(elev,1),
            "mixing_height_m":met.mixing_height_m,"transport_from_deg":round(transport_from,0),"transport_toward_deg":round(toward,0),
            "transport_mps":round(transport_mps,1),"transport_basis":transport_basis,"burned_area_ha":round(area/10_000.0,1),
            "area_growth_ha_h":round(growth*3600.0/10_000.0,1),"heat_release_mw":round(source.heat_release_w/1e6,0),
            "emission_pm25_kg_h":round(source.emission_g_s*3.6,0),"buoyancy_flux_m4_s3":round(plume.buoyancy_flux_m4_s3,0),
            "plume_rise_m":round(plume.plume_rise_m,0),"effective_height_m":round(plume.effective_height_m,0),
            "trapped_in_mixing_layer":plume.trapped_in_mixing_layer,"reach":reach_km}));
        smoke_fields.push(SmokeField {
            hour: hz,
            frame: smoke_frame,
            size,
            cell_m: config.smoke_cell_m,
            values,
        });
    }

    // Wind arrows (10 m, now) on a 7 × 7 lattice.
    let span = w as f64 * cell;
    for j in 0..7 {
        for i in 0..7 {
            let (x, y) = ((i as f64 + 0.5) / 7.0 * span, (j as f64 + 0.5) / 7.0 * span);
            if let Some(wx) = env.weather.sample(env.frame.to_lon_lat(x, y), now) {
                let (e, n) = wx.wind_vector();
                let len = 220.0 * wx.wind_mps.min(20.0);
                let norm = wx.wind_mps.max(1e-6);
                features.push(json!({"type":"Feature","geometry":line(&env.frame, (x, y), (x + e / norm * len, y + n / norm * len)),
                    "properties":{"layer":"wind","speed_mps":round(wx.wind_mps,1),"from_deg":round(wx.wind_from_deg,0),"gust_mps":wx.gust_mps.map(|g| round(g,1))}}));
            }
        }
    }
    let head_bearing_deg = head.map_or(0.0, |s| s.head_bearing.to_degrees());
    if let Some(s) = head {
        let len = (s.head_ros_mps * 3600.0).clamp(300.0, 8000.0);
        let b = s.head_bearing;
        features.push(json!({"type":"Feature","geometry":line(&env.frame, ignition_local, (ignition_local.0 + len * b.sin(), ignition_local.1 + len * b.cos())),
            "properties":{"layer":"head_vector","ros_m_min":round(s.head_ros_mps*60.0,1),"bearing_deg":round(head_bearing_deg,0)}}));
    }
    features.push(json!({"type":"Feature","geometry":{"type":"Point","coordinates":ignition},"properties":{"layer":"ignition"}}));

    let wx0: HourWeather = env
        .weather
        .sample(ignition, now)
        .unwrap_or(ignition_weather[now_hour]);
    let (solar_el, solar_az) = solar::position(ignition, now);
    let slope0 = landscape.slope[central.ignition_cell];
    let agreement = match (&adj.reported_spread_bearing, run_bearing_3h) {
        (Some(reported), Some(model)) => {
            let delta = bearing_delta_deg(reported.value, model);
            json!({"reported_bearing_deg":reported.value,"model_run_bearing_deg":round(model,0),"delta_deg":round(delta,0),
                "verdict": if delta <= 30.0 {"consistent"} else if delta <= 60.0 {"partly consistent"} else {"inconsistent: review inputs"},
                "evidence":reported.evidence,"url":reported.url})
        }
        _ => Value::Null,
    };
    let (transport_toward_deg, transport_mps, gust_mps) = first_transport.unwrap_or((
        from_to_toward_deg(wx0.wind_from_deg),
        wx0.wind_mps,
        wx0.gust_mps,
    ));
    let mut limitations = vec![
        "Experimental physics model, not calibrated against Portuguese fires; not operational guidance.".to_string(),
        "Surface fire only: crown fire, ember spotting and suppression effects are not modelled, so extreme runs can be underestimated.".into(),
        "Wind is interpolated from forecast points without terrain channelling (no WindNinja-style downscaling).".into(),
        "Fuel models are standard Anderson equivalents for Portuguese vegetation, not calibrated local fuel models.".into(),
        "Smoke is a steady-state Gaussian plume with default mixing heights; concentrations are order-of-magnitude.".into(),
    ];
    match env.basis {
        Basis::WhatIf => limitations
            .push("What-if: present-day weather applied to a synthetic training incident.".into()),
        Basis::Synthetic => {
            limitations.push("Weather is synthetic; this run demonstrates the method only.".into())
        }
        _ => {}
    }
    if ensemble.edge_reached {
        limitations.push("Fire reached the 19 km model domain edge in at least one ensemble member; area beyond is not simulated.".into());
    }
    if env.request.uncertainty_m > 1500.0 {
        limitations.push("Reported location uncertainty exceeds 1.5 km; ignition sampling is capped at 1.5 km (2σ).".into());
    }
    let input_urls: Vec<String> = env
        .inputs
        .iter()
        .filter_map(|i| i.url.clone())
        .chain(env.intel.reports.iter().map(|r| r.url.clone()))
        .chain(env.intel.regional.iter().flat_map(|r| r.sources.clone()))
        .collect();
    let summary = json!({
        "model": MODEL_VERSION,
        "basis": env.basis,
        "ignition_estimate": start,
        "now": now,
        "elapsed_min": (elapsed / 60.0).round(),
        "members": ensemble.members.len(),
        "seed": config.seed,
        "cell_m": cell,
        "domain_km": round(span / 1000.0, 1),
        "runtime_ms": started.elapsed().as_millis() as u64,
        "edge_reached": ensemble.edge_reached,
        "horizons": horizon_stats,
        "head": head.map(|s| json!({
            "ros_m_min": round(s.head_ros_mps * 60.0, 1), "bearing_deg": round(head_bearing_deg, 0),
            "intensity_kw_m": round(s.intensity_kw_m, 0), "flame_length_m": round(s.flame_length_m, 1),
            "length_to_breadth": round(s.length_to_breadth, 2), "effective_wind_kmh": round(s.effective_wind_mps * 3.6, 1),
            "wind_limited": s.wind_limited, "suppression": suppression_class(s.intensity_kw_m),
        })),
        "ignition": {
            "position": ignition, "uncertainty_m": env.request.uncertainty_m, "elevation_m": round(env.dem.at(central.ignition_cell % w, central.ignition_cell / w), 0),
            "slope_deg": round(slope0.tan.atan().to_degrees(), 1), "upslope_deg": round(norm_deg(slope0.upslope.to_degrees()), 0),
            "fuel": ignition_cover.label(), "fuel_model": ignition_cover.fuel().map(|f| f.code),
        },
        "weather": {
            "valid_at": wx0.time, "temp_c": round(wx0.temp_c, 1), "rh_pct": round(wx0.rh_pct, 0),
            "wind_mps": round(wx0.wind_mps, 1), "wind_from_deg": round(wx0.wind_from_deg, 0), "gust_mps": wx0.gust_mps.map(|g| round(g, 1)),
            "wind_120m_mps": wx0.wind_upper_mps.map(|v| round(v, 1)), "wind_120m_from_deg": wx0.wind_upper_from_deg.map(|v| round(v, 0)),
            "cloud_pct": wx0.cloud_pct.map(|v| round(v, 0)), "shortwave_wm2": wx0.shortwave_wm2.map(|v| round(v, 0)),
            "vpd_kpa": wx0.vpd_kpa.map(|v| round(v, 2)), "solar_elevation_deg": round(solar_el, 1), "solar_azimuth_deg": round(solar_az, 0),
            "hourly": ignition_weather.iter().skip(now_hour).map(|h| json!({"t":h.time,"wind_mps":round(h.wind_mps,1),"from_deg":round(h.wind_from_deg,0),"gust_mps":h.gust_mps.map(|g| round(g,1)),"temp_c":round(h.temp_c,1),"rh_pct":round(h.rh_pct,0)})).collect::<Vec<_>>(),
        },
        "moisture": {"dead_1h_pct": round(hours[0].moisture.dead_1h * 100.0, 1), "dead_10h_pct": round(hours[0].moisture.dead_10h * 100.0, 1),
            "dead_100h_pct": round(hours[0].moisture.dead_100h * 100.0, 1), "live_pct": round(hours[0].moisture.live * 100.0, 0), "basis": moisture_basis},
        "terrain": {"relief_m": round(env.dem.relief_m(), 0)},
        "fuel": {"fractions": env.cover.fractions(), "mapped_fraction": round(env.cover.mapped_fraction, 2),
            "ignition_model": ignition_cover.fuel().map(|f| format!("{} {}", f.code, f.name)), "nonburnable": LandCover::NonBurnable.label()},
        "smoke": smoke_stats,
        "smoke_method": {"emission_factor_g_kg": PM25_EMISSION_FACTOR_G_PER_KG, "convective_fraction": CONVECTIVE_FRACTION, "thresholds_ug_m3": SMOKE_THRESHOLDS_UG_M3},
        "intel": {
            "reports": env.intel.reports, "regional": env.intel.regional, "credits_used": env.intel.credits_used,
            "calls": env.intel.calls, "errors": env.intel.errors, "adjustments": env.adjustments, "agreement": agreement,
        },
        "warnings": env.warnings,
        "inputs": env.inputs,
        "references": [
            "Rothermel 1972, USDA FS Res. Pap. INT-115", "Albini 1976, USDA FS Gen. Tech. Rep. INT-30",
            "Anderson 1982, USDA FS Gen. Tech. Rep. INT-122", "Andrews 2018, USDA FS Gen. Tech. Rep. RMRS-GTR-371",
            "Anderson 1983, USDA FS Res. Pap. INT-305 (length-to-breadth)", "Finney 2002, Can. J. For. Res. 32 (minimum travel time)",
            "Byram 1959 (fireline intensity, flame length)", "Simard 1968 (equilibrium moisture content)",
            "Briggs 1973/1975 (dispersion coefficients, plume rise); Turner 1970 (Gaussian plume)", "Horn 1981 (slope and aspect)",
        ],
    });
    let geojson = json!({"type":"FeatureCollection","features":features,"firecast":summary});
    Ok(FirecastResult {
        geojson,
        summary,
        frame: env.frame,
        cell_m: cell,
        elevation_m: env.dem.elevation_m.clone(),
        smoke: smoke_fields,
        ignition_local,
        start: now,
        head_bearing_deg,
        transport_toward_deg,
        transport_mps,
        gust_mps,
        limitations,
        input_urls,
        ensemble,
    })
}

/// Wrap a result in the shared Forecast contract (geometry carries the summary as
/// a GeoJSON foreign member named `firecast`).
pub fn to_domain(
    context: &RunContext,
    incident: &Incident,
    evidence: &[Evidence],
    weather: &[Weather],
    result: &FirecastResult,
) -> Result<Forecast, FirecastError> {
    let now = context.clock.now();
    if incident.run_id != context.run_id
        || incident.mode != context.mode
        || evidence
            .iter()
            .any(|e| e.run_id != context.run_id || e.provenance.available_at > now)
        || weather
            .iter()
            .any(|w| w.run_id != context.run_id || w.provenance.available_at > now)
    {
        return Err(FirecastError::Compute("cross-run or future inputs".into()));
    }
    let mut evidence_ids: Vec<String> = evidence
        .iter()
        .filter(|e| incident.evidence_ids.contains(&e.id))
        .map(|e| e.id.clone())
        .collect();
    evidence_ids.sort();
    let mut weather_ids: Vec<String> = weather.iter().map(|w| w.id.clone()).collect();
    weather_ids.sort();
    Ok(Forecast {
        id: format!("firecast-{}-{}", incident.id, context.epoch),
        run_id: context.run_id.clone(),
        incident_id: incident.id.clone(),
        epoch: context.epoch,
        generated_at: now,
        input_evidence_ids: evidence_ids,
        input_weather_ids: weather_ids,
        model_version: MODEL_VERSION.into(),
        seed: result.summary["seed"].as_u64().unwrap_or(0),
        horizon_seconds: (HORIZON_H * 3600) as u32,
        illustrative: true,
        geometry: result.geojson.clone(),
        limitations: result.limitations.clone(),
    })
}
