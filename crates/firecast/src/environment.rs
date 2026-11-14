//! Assemble the fire environment: terrain, land cover, weather field, Context.dev
//! intelligence and IPMA warnings. Each input records where it came from; any
//! input that cannot be fetched falls back to an explicitly synthetic value.
use crate::fuel::LandCover;
use crate::geo::LocalFrame;
use crate::intel::{Adjustments, ContextDev, FireIntel, IntelMode, derive_adjustments};
use crate::landcover::LandCoverGrid;
use crate::solar;
use crate::sources::{HttpSources, WeatherWarning, floor_hour};
use crate::terrain::{Dem, synthetic};
use crate::weather::{HourWeather, WeatherField, WeatherPoint};
use chrono::{DateTime, Utc};
use serde::Serialize;

pub const GRID_CELLS: usize = 161;
pub const CELL_M: f64 = 120.0;
pub const HORIZON_H: i64 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// Real incident, current weather and web reports.
    Live,
    /// Past incident, weather for that time.
    Historical,
    /// Training incident at a real place with present-day weather, terrain and cover.
    WhatIf,
    /// No external data: every input synthetic.
    Synthetic,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InputRecord {
    pub kind: String,
    pub source: String,
    pub url: Option<String>,
    pub via: Option<String>,
    pub status: String,
    pub detail: String,
    pub captured_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct AcquireRequest {
    pub ignition: [f64; 2],
    pub uncertainty_m: f64,
    pub place: String,
    /// Estimated ignition time (first report), when the simulation starts.
    pub start: DateTime<Utc>,
    /// Run-clock "now": forecast horizons are measured from here.
    pub now: DateTime<Utc>,
    pub basis: Basis,
    pub max_context_credits: f64,
    pub network: bool,
}

#[derive(Clone, Debug)]
pub struct FireEnvironment {
    pub request: AcquireRequest,
    pub basis: Basis,
    pub frame: LocalFrame,
    pub width: usize,
    pub height: usize,
    pub cell_m: f64,
    pub dem: Dem,
    pub cover: LandCoverGrid,
    pub weather: WeatherField,
    pub intel: FireIntel,
    pub adjustments: Adjustments,
    pub warnings: Vec<WeatherWarning>,
    pub inputs: Vec<InputRecord>,
}

fn record(
    kind: &str,
    source: &str,
    url: Option<String>,
    via: Option<&str>,
    status: &str,
    detail: impl Into<String>,
) -> InputRecord {
    InputRecord {
        kind: kind.into(),
        source: source.into(),
        url,
        via: via.map(str::to_owned),
        status: status.into(),
        detail: detail.into(),
        captured_at: Utc::now(),
    }
}

/// Explicitly synthetic weather: a hot, dry Portuguese summer afternoon with a
/// strong north-westerly ("nortada") veering and strengthening through the period.
pub fn synthetic_weather(centre: [f64; 2], start: DateTime<Utc>, hours_after: i64) -> WeatherField {
    let t0 = floor_hour(start) - chrono::Duration::hours(3);
    let n = hours_after + 5;
    let hours = (0..=n)
        .map(|k| {
            let time = t0 + chrono::Duration::hours(k);
            let f = k as f64 / n as f64;
            let (el, _) = solar::position(centre, time);
            HourWeather {
                time,
                temp_c: 32.0 + 3.0 * f,
                rh_pct: 22.0 - 7.0 * f,
                wind_mps: 7.0 + 3.0 * f,
                wind_from_deg: 310.0 + 15.0 * f,
                gust_mps: Some(12.0 + 5.0 * f),
                wind_upper_mps: Some(10.0 + 4.0 * f),
                wind_upper_from_deg: Some(318.0 + 12.0 * f),
                cloud_pct: Some(5.0),
                shortwave_wm2: Some((900.0 * el.to_radians().sin()).max(0.0)),
                precip_mm: Some(0.0),
                vpd_kpa: Some(3.2),
            }
        })
        .collect();
    WeatherField {
        points: vec![WeatherPoint {
            lon_lat: centre,
            hours,
        }],
    }
}

/// 3 × 3 sample points 8 km apart around the ignition for the spatial wind field.
pub fn weather_points(centre: [f64; 2]) -> Vec<[f64; 2]> {
    let frame = LocalFrame::centred(centre, 8_000.0);
    let mut points = Vec::new();
    for j in 0..3 {
        for i in 0..3 {
            points.push(frame.to_lon_lat(i as f64 * 8_000.0, j as f64 * 8_000.0));
        }
    }
    points
}

pub async fn acquire(
    sources: Option<&HttpSources>,
    context: Option<&ContextDev>,
    request: AcquireRequest,
) -> FireEnvironment {
    let (w, h, cell) = (GRID_CELLS, GRID_CELLS, CELL_M);
    let frame = LocalFrame::centred(request.ignition, w as f64 * cell / 2.0);
    let mut inputs = Vec::new();
    let online = request.network && request.basis != Basis::Synthetic;
    let sources = sources.filter(|_| online);
    let now = Utc::now();

    // Terrain: Terrarium tiles, then Open-Meteo elevation, then synthetic relief.
    let dem = match sources {
        Some(s) => {
            match s.dem_tiles(frame, w, h, cell).await {
                Ok(dem) => {
                    inputs.push(record("terrain", "Terrain Tiles (Mapzen / AWS Open Data; SRTM, EU-DEM et al.)", Some(s.terrarium.clone()), None, "ok", "Zoom-11 Terrarium DEM, bilinear to 120 m cells; slope/aspect by Horn's method"));
                    Some(dem)
                }
                Err(tile_err) => match s.dem_open_meteo(frame, w, h, cell).await {
                    Ok(dem) => {
                        inputs.push(record("terrain", "Open-Meteo elevation (Copernicus GLO-90)", Some(format!("{}/v1/elevation", s.open_meteo)), None, "ok", format!("21×21 samples resampled to 120 m (terrain tiles unavailable: {tile_err})")));
                        Some(dem)
                    }
                    Err(e) => {
                        inputs.push(record(
                            "terrain",
                            "Elevation",
                            None,
                            None,
                            "failed",
                            format!("{tile_err}; {e}"),
                        ));
                        None
                    }
                },
            }
        }
        None => None,
    };
    let dem = dem.unwrap_or_else(|| {
        inputs.push(record(
            "terrain",
            "Synthetic relief",
            None,
            None,
            "synthetic",
            "Deterministic NE–SW ridge system; not real terrain",
        ));
        synthetic(frame, w, h, cell)
    });

    // Web intelligence (Context.dev) first: it can supply the regional vegetation.
    let intel_mode = match request.basis {
        Basis::Live => Some(IntelMode::Live),
        Basis::Historical => Some(IntelMode::Historical),
        Basis::WhatIf => Some(IntelMode::WhatIf),
        Basis::Synthetic => None,
    };
    let intel = match (context.filter(|_| online), intel_mode) {
        (Some(ctx), Some(mode)) => {
            let intel = ctx
                .gather(
                    mode,
                    &request.place,
                    request.start,
                    request.max_context_credits,
                )
                .await;
            inputs.push(record(
                "intel",
                "Live web reports and regional conditions",
                None,
                Some("Context.dev"),
                if intel.errors.is_empty() {
                    "ok"
                } else if intel.calls.len() > intel.errors.len() {
                    "partial"
                } else {
                    "failed"
                },
                format!(
                    "{} calls ({}), {} reports, {:.0} credits{}",
                    intel.calls.len(),
                    intel.calls.join(", "),
                    intel.reports.len(),
                    intel.credits_used,
                    if intel.errors.is_empty() {
                        String::new()
                    } else {
                        format!("; errors: {}", intel.errors.join("; "))
                    }
                ),
            ));
            intel
        }
        _ => {
            inputs.push(record(
                "intel",
                "Live web reports",
                None,
                Some("Context.dev"),
                "skipped",
                if request.basis == Basis::Synthetic {
                    "Synthetic run"
                } else {
                    "CONTEXT_DEV_API_KEY not configured"
                },
            ));
            FireIntel::default()
        }
    };
    let adjustments = derive_adjustments(&intel);
    let regional = adjustments.regional_cover.as_ref().map(|s| s.value);
    let default_cover = regional.unwrap_or(LandCover::Eucalyptus);
    let default_note = match &adjustments.regional_cover {
        Some(s) => format!("regional default {} from web reports (\"{}\")", s.value.label(), s.evidence),
        None => "regional default eucalyptus (most widespread forest type in central Portugal; assumption)".into(),
    };

    // Land cover: OSM polygons over the regional default.
    let mut cover = match sources {
        Some(s) => match s.land_cover(frame, w, h, cell, default_cover).await {
            Ok((grid, url)) => {
                inputs.push(record(
                    "land_cover",
                    "OpenStreetMap (ODbL) via Overpass",
                    Some(url),
                    None,
                    "ok",
                    format!(
                        "{:.0}% of cells mapped from OSM polygons; {default_note}",
                        grid.mapped_fraction * 100.0
                    ),
                ));
                grid
            }
            Err(e) => {
                inputs.push(record(
                    "land_cover",
                    "OpenStreetMap",
                    None,
                    None,
                    "failed",
                    format!("{e}; uniform {default_note}"),
                ));
                LandCoverGrid::uniform(w, h, default_cover)
            }
        },
        None => {
            inputs.push(record(
                "land_cover",
                "Uniform fuel",
                None,
                None,
                "synthetic",
                format!("No land-cover data; {default_note}"),
            ));
            LandCoverGrid::uniform(w, h, default_cover)
        }
    };
    for (c, z) in cover.cells.iter_mut().zip(&dem.elevation_m) {
        if *z <= 0.5 {
            *c = LandCover::NonBurnable;
        }
    }

    // Weather field.
    let mut basis = request.basis;
    let points = weather_points(request.ignition);
    let elapsed_h = (request.now - request.start)
        .num_minutes()
        .max(0)
        .saturating_add(59)
        / 60;
    let span_h = elapsed_h + HORIZON_H;
    let weather = match sources {
        Some(s) => {
            // A what-if run applies the present-day forecast to the scenario clock.
            let fetch_at = if request.basis == Basis::WhatIf {
                now - (request.now - request.start)
            } else {
                request.start
            };
            match s.weather(&points, fetch_at, span_h).await {
                Ok((mut field, url)) => {
                    let shift = floor_hour(request.start) - floor_hour(fetch_at);
                    if request.basis == Basis::WhatIf {
                        field
                            .points
                            .iter_mut()
                            .flat_map(|p| p.hours.iter_mut())
                            .for_each(|h| h.time += shift);
                    }
                    inputs.push(record("weather", "Open-Meteo forecast (ECMWF/ICON/GFS/Météo-France blend), CC BY 4.0", Some(url), None, "ok",
                        if request.basis == Basis::WhatIf {
                            format!("Present-day forecast captured {} applied to the scenario clock; 9 points 8 km apart, 10 m and 120 m wind", now.format("%Y-%m-%d %H:%M UTC"))
                        } else {
                            "9 points 8 km apart; hourly 10 m and 120 m wind, gusts, T, RH, cloud, radiation, rain, VPD".into()
                        }));
                    field
                }
                Err(e) => {
                    inputs.push(record(
                        "weather",
                        "Open-Meteo",
                        None,
                        None,
                        "failed",
                        e.to_string(),
                    ));
                    basis = Basis::Synthetic;
                    synthetic_weather(request.ignition, request.start, span_h)
                }
            }
        }
        None => synthetic_weather(request.ignition, request.start, span_h),
    };
    if basis == Basis::Synthetic {
        inputs.push(record(
            "weather",
            "Synthetic weather",
            None,
            None,
            "synthetic",
            "Hot dry summer afternoon, 7→10 m/s north-westerly; not a forecast",
        ));
    }

    let warnings = match sources {
        Some(s) => match s.ipma_warnings(request.ignition, request.now).await {
            Ok((w, url)) => {
                inputs.push(record(
                    "warnings",
                    "IPMA weather warnings (source: IPMA)",
                    Some(url),
                    None,
                    "ok",
                    format!("{} active warnings for the nearest district", w.len()),
                ));
                w
            }
            Err(e) => {
                inputs.push(record(
                    "warnings",
                    "IPMA weather warnings",
                    None,
                    None,
                    "failed",
                    e.to_string(),
                ));
                Vec::new()
            }
        },
        None => Vec::new(),
    };

    FireEnvironment {
        request,
        basis,
        frame,
        width: w,
        height: h,
        cell_m: cell,
        dem,
        cover,
        weather,
        intel,
        adjustments,
        warnings,
        inputs,
    }
}
