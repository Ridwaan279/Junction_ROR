//! Offline Rust replacement for the early-demo verification/export/build steps.
//! Full article Markdown is read privately and never written to public output.
use crate::{
    FirecastError,
    fleet::{self, Airframe, Drone, FleetPlan, Held, Station, Term},
    geo::{LocalFrame, haversine_m},
    historical::Dataset,
    routing::{self, Hazard, RoutedMission},
    smoke::{PlumeMet, PlumeSource, concentration, summarise},
    terrain::Dem,
};
use chrono::{DateTime, Utc};
use emberwatch_intelligence::accuracy::{availability, number_supported, quote_verified};
use emberwatch_response::WindVector;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

fn date(v: &Value) -> Option<DateTime<Utc>> {
    v.as_str().and_then(|s| s.parse().ok())
}
fn failure(message: &str) -> FirecastError {
    FirecastError::Parse(message.into())
}

/// The fixture index supplies the frozen display schema, publisher descriptions,
/// cited place matches and recorded status timeline. Every evidence-quality flag
/// and reveal time is recomputed from the retained originals, not trusted from it.
pub fn verified_index(
    reference: &Value,
    pages: &Value,
    raw_dir: &Path,
) -> Result<Value, FirecastError> {
    let mut index = reference.clone();
    let rows = pages.as_array().ok_or_else(|| failure("pages array"))?;
    let mut markdown_by_url = BTreeMap::new();
    for entry in std::fs::read_dir(raw_dir).map_err(|_| failure("private original directory"))? {
        let entry = entry.map_err(|_| failure("private capture entry"))?;
        if !entry
            .file_type()
            .map_err(|_| failure("capture file type"))?
            .is_file()
            || !entry.file_name().to_string_lossy().starts_with("scrape_")
        {
            continue;
        }
        let bytes = std::fs::read(entry.path()).map_err(|_| failure("read original"))?;
        let raw: Value = serde_json::from_slice(&bytes).map_err(|_| failure("original JSON"))?;
        if let (Some(url), Some(md)) = (
            raw["_requested_url"].as_str(),
            raw.pointer("/markdown/data").and_then(Value::as_str),
        ) {
            let page = rows
                .iter()
                .find(|p| p["url"] == url)
                .ok_or_else(|| failure("unrecognised original URL"))?;
            if page["request_id"] != raw["request_id"] {
                return Err(failure("original request ID mismatch"));
            }
            use sha2::{Digest, Sha256};
            if hex::encode(Sha256::digest(md.as_bytes()))
                != page["markdown_sha256"].as_str().unwrap_or_default()
            {
                return Err(failure("original Markdown fingerprint mismatch"));
            }
            markdown_by_url.insert(url.to_owned(), md.to_owned());
        }
    }
    let mut reveal = BTreeMap::new();
    for (i, page) in rows.iter().enumerate() {
        let id = format!("page-{i:02}");
        if page["url"]
            .as_str()
            .unwrap_or_default()
            .contains("fogos.pt/")
        {
            continue;
        }
        let md = markdown_by_url.get(page["url"].as_str().unwrap_or_default());
        let printed = page["printed_published"].as_str();
        let statements = page["facts"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|f| [f["quote"].as_str(), f["time_text"].as_str()])
            .flatten()
            .collect::<Vec<_>>();
        let times = availability(
            page["meta_published"].as_str(),
            page["meta_modified"].as_str(),
            printed,
            md.is_some_and(|md| printed.is_some_and(|p| quote_verified(md, p))),
            &statements,
        );
        reveal.insert(id.clone(), times.known_at);
        if let Some(out) = index["pages"]
            .as_array_mut()
            .and_then(|p| p.iter_mut().find(|p| p["id"] == id))
        {
            out["published_at"] = json!(times.published_at);
            out["modified_at"] = page["meta_modified"].clone();
            out["known_at"] = json!(times.known_at);
            out["time_flags"] = json!(times.flags);
        }
    }
    for evidence in index["evidence"]
        .as_array_mut()
        .ok_or_else(|| failure("evidence array"))?
    {
        let md = markdown_by_url.get(evidence["url"].as_str().unwrap_or_default());
        let quote = evidence["quote"].as_str().unwrap_or_default();
        let quote_ok = md.is_some_and(|md| quote_verified(md, quote));
        let number_ok = evidence["number"]
            .as_f64()
            .is_none_or(|n| number_supported(n, quote));
        let places_ok = evidence["places"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|p| {
                let name = p["name"]
                    .as_str()
                    .unwrap_or_default()
                    .split('(')
                    .next()
                    .unwrap_or_default()
                    .trim();
                !name.is_empty()
                    && emberwatch_intelligence::accuracy::normalize(quote)
                        .to_lowercase()
                        .contains(
                            &emberwatch_intelligence::accuracy::normalize(name).to_lowercase(),
                        )
            });
        evidence["quality"] = json!(if quote_ok && number_ok && places_ok {
            "verified"
        } else {
            "unverified"
        });
        evidence["number_check"] = if evidence["number"].is_null() {
            Value::Null
        } else {
            json!(if number_ok {
                "in_quote"
            } else {
                "not_literal_in_quote"
            })
        };
        evidence["checks"]
            .as_array_mut()
            .unwrap()
            .push(json!(if !quote_ok {
                "Rust: quote absent after NFC/whitespace normalization"
            } else if !number_ok {
                "Rust: value unsupported by Portuguese quantity parser"
            } else if !places_ok {
                "Rust: named place missing from supporting quote"
            } else {
                "Rust: original hash, quote and value grounding passed"
            }));
        if let Some(t) = reveal.get(evidence["page"].as_str().unwrap_or_default()) {
            evidence["known_at"] = json!(t);
        }
    }
    index["frames"] = json!([]);
    index["golden_benchmark_status"] = json!("omitted_by_user_request");
    index["limitations"] = json!([
        "Historical replay: source observations, model forecasts and hypothetical drone operations are distinct.",
        "Original articles were captured retrospectively in November 2026. Later edits cannot be ruled out; reveal times are conservative, not proof of operational availability in 2025.",
        "ERA5 is retrospective reanalysis, not a weather forecast available at the incident cursor.",
        "Half-hour model frames during the first three hours; then three-hour frames. Fuel assumptions and the experimental spread/smoke model remain uncalibrated.",
        "Human-labelled benchmark omitted at user request. Verified means deterministic source grounding, not independent accuracy certification.",
        "Existing cited GeoNames place matches are retained with their provenance. Ambiguous/unresolved names remain unmapped; no OSM match is invented.",
        "Ground transfers are schematic, with configured speed/distance assumptions; no real road route, depot, aircraft or dispatch is claimed.",
        "Routing blocks unknown hazards and terrain, burn probability >=0.3 and model surface smoke >=150 ug/m3. Surface smoke is a conservative proxy, not an altitude-resolved forecast."
    ]);
    Ok(index)
}

pub fn probability(grid: &Value, key: &str, p: [f64; 2]) -> Option<f64> {
    let origin: [f64; 2] = serde_json::from_value(grid["origin_lon_lat"].clone()).ok()?;
    let frame = LocalFrame {
        origin_lon_lat: origin,
    };
    let (x, y) = frame.to_local(p);
    let cell = grid["cell_m"].as_f64()?;
    let n = grid["size"].as_u64()? as usize;
    if x < 0. || y < 0. || x >= n as f64 * cell || y >= n as f64 * cell {
        return None;
    }
    grid[key]
        .get((y / cell).floor() as usize * n + (x / cell).floor() as usize)?
        .as_f64()
}

pub fn exposures(index: &Value, frame: &Value, at: DateTime<Utc>) -> Vec<Value> {
    let ignition: [f64; 2] = serde_json::from_value(index["ignition"]["position"].clone()).unwrap();
    let mut sites: BTreeMap<String, Value> = BTreeMap::new();
    for e in index["evidence"].as_array().into_iter().flatten() {
        if date(&e["known_at"]).is_none_or(|t| t > at) {
            continue;
        }
        let kind = match e["category"].as_str().unwrap_or_default() {
            "evacuation" => "evacuated",
            "threatened_place" => "threatened",
            "shelter" => "shelter",
            "people_present" => "people_present",
            "event_gathering" => "event",
            "road_closure" => "road_closure",
            _ => continue,
        };
        for place in e["places"].as_array().into_iter().flatten() {
            let name = place["geonames"]
                .as_str()
                .or(place["name"].as_str())
                .unwrap_or_default();
            if matches!(
                name,
                "Arganil" | "Seia" | "Oliveira do Hospital" | "Pampilhosa da Serra"
            ) {
                continue;
            }
            let position: Option<[f64; 2]> = serde_json::from_value(place["position"].clone()).ok();
            let site=sites.entry(name.into()).or_insert_with(||json!({"id":format!("site-{name}"),"name":name,"kind":[],
                "position":position,"people":null,"people_basis":null,"population_geonames":null,
                "distance_from_ignition_km":position.map(|p|haversine_m(ignition,p)/1000.),"events":[],"evidence":[],
                "in_modelled_burn_p50":false,"in_forecast_6h":false,"in_forecast_12h":false,
                "p_fire_6h":null,"priority":null,"automation_eligible":false}));
            if !site["kind"].as_array().unwrap().iter().any(|k| k == kind) {
                site["kind"].as_array_mut().unwrap().push(json!(kind));
            }
            site["evidence"]
                .as_array_mut()
                .unwrap()
                .push(e["id"].clone());
            if e["event"].is_object() {
                let mut event = e["event"].clone();
                event["evidence"] = e["id"].clone();
                site["events"].as_array_mut().unwrap().push(event);
            }
            if let Some(p) = position {
                let probability = probability(&frame["planning"], "burn_6h", p);
                site["p_fire_6h"] = json!(probability);
                site["in_forecast_6h"] = json!(probability.is_some_and(|p| p >= 0.5));
                site["in_modelled_burn_p50"] =
                    json!(point_in_geometry(p, &frame["predicted"]["p50"]));
                site["in_forecast_12h"] = json!(point_in_geometry(p, &frame["forecast"]["h12"]));
                let eligible = e["quality"] == "verified"
                    && kind != "road_closure"
                    && (kind != "event"
                        || e["event"]["status"] != "cancelled"
                            && e["event"]["scheduled_start_date"]
                                .as_str()
                                .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                                .is_some_and(|d| {
                                    d >= at.date_naive()
                                        && d <= (at + chrono::Duration::hours(30)).date_naive()
                                }));
                site["automation_eligible"] =
                    json!(site["automation_eligible"] == true || eligible);
                if eligible
                    && e["places"].as_array().is_some_and(|p| p.len() == 1)
                    && e["unit"].as_str().is_some_and(|u| {
                        ["pessoa", "morador", "habitante", "residente"]
                            .iter()
                            .any(|s| u.contains(s))
                    })
                    && let Some(people) = e["number"].as_f64()
                {
                    site["people"] = json!(people);
                    site["people_basis"] =
                        json!(format!("stated: {}", e["id"].as_str().unwrap_or_default()));
                }
                // These are explicitly configured scenario weights, never claimed
                // attendance. A source count is used only when stated and grounded.
                let default = site["assumed_people_for_priority"]
                    .as_f64()
                    .unwrap_or(0.)
                    .max(if kind == "shelter" { 40. } else { 20. });
                site["assumed_people_for_priority"] = json!(default);
                if site["people"].is_null() {
                    site["people_basis"] = json!("kind_default");
                }
                site["priority"] =
                    json!(probability.map(|p| p * site["people"].as_f64().unwrap_or(default)));
            }
        }
    }
    sites.into_values().collect()
}

fn point_in_geometry(p: [f64; 2], g: &Value) -> bool {
    fn ring(p: [f64; 2], r: &Value) -> bool {
        let Some(r) = r.as_array() else {
            return false;
        };
        let mut inside = false;
        for (a, b) in r.iter().zip(r.iter().cycle().skip(1)).take(r.len()) {
            let (Some(ax), Some(ay), Some(bx), Some(by)) =
                (a[0].as_f64(), a[1].as_f64(), b[0].as_f64(), b[1].as_f64())
            else {
                continue;
            };
            if (ay > p[1]) != (by > p[1]) && p[0] < (bx - ax) * (p[1] - ay) / (by - ay) + ax {
                inside = !inside;
            }
        }
        inside
    }
    let polys = if g["type"] == "MultiPolygon" {
        g["coordinates"].as_array().cloned().unwrap_or_default()
    } else if g["type"] == "Polygon" {
        vec![g["coordinates"].clone()]
    } else {
        vec![]
    };
    polys.iter().any(|poly| {
        poly.as_array().is_some_and(|r| {
            !r.is_empty() && ring(p, &r[0]) && !r.iter().skip(1).any(|r| ring(p, r))
        })
    })
}

#[allow(clippy::too_many_arguments)]
fn planned_fleet(
    frame: &Value,
    stations: &[Station],
    dem: &Dem,
    cfg: &Airframe,
    exposures: &[Value],
    previous: Option<usize>,
    detected: bool,
) -> Result<FleetPlan, FirecastError> {
    let at = date(&frame["t"]).ok_or_else(|| failure("frame timestamp"))?;
    let mut plan = FleetPlan {
        at,
        count: 0,
        previous_count: previous,
        change_reason_pt: None,
        change_reason_en: None,
        breakdown: vec![],
        drones: vec![],
        held: vec![],
    };
    if !detected {
        return Ok(plan);
    }
    let grid = &frame["planning"];
    let origin: [f64; 2] = serde_json::from_value(grid["origin_lon_lat"].clone())
        .map_err(|_| failure("grid origin"))?;
    let local = LocalFrame {
        origin_lon_lat: origin,
    };
    let n = grid["size"].as_u64().ok_or_else(|| failure("grid size"))? as usize;
    let cell = grid["cell_m"]
        .as_f64()
        .ok_or_else(|| failure("grid cell"))?;
    let plume: Option<(PlumeSource, PlumeMet)> =
        serde_json::from_value::<PlumeSource>(grid["plume"]["source"].clone())
            .ok()
            .zip(serde_json::from_value::<PlumeMet>(grid["plume"]["met"].clone()).ok());
    let summary = plume.as_ref().map(|(s, m)| summarise(s, m));
    let hazard = |p| Hazard {
        burn_probability: probability(grid, "burn_mission", p),
        smoke_ugm3: if let (Some((s, m)), Some(summary)) = (&plume, &summary) {
            let (x, y) = local.to_local(p);
            Some(concentration(s, m, summary, x, y))
        } else if frame["weather"].is_object() {
            Some(0.)
        } else {
            None
        },
    };
    let wind_speed = frame["weather"]["wind_mps"].as_f64();
    let wind_from = frame["weather"]["wind_from_deg"].as_f64();
    let (Some(speed), Some(from)) = (wind_speed, wind_from) else {
        plan.held.push(Held {
            station_id: "all".into(),
            reason: "Weather missing; no flight planned".into(),
        });
        return Ok(plan);
    };
    let bearing = (from + 180.).to_radians();
    let wind = WindVector {
        east_mps: speed * bearing.sin(),
        north_mps: speed * bearing.cos(),
    };
    let gust = frame["weather"]["gust_mps"].as_f64();
    // Stand-off task points near the forecast front, distributed by compass sector.
    let burned = grid["burn_1h"]
        .as_array()
        .ok_or_else(|| failure("burn grid"))?;
    let mut centre = (0., 0.);
    let mut count = 0.;
    for (k, p) in burned.iter().enumerate() {
        if p.as_f64().unwrap_or(0.) >= 0.5 {
            centre.0 += (k % n) as f64 * cell;
            centre.1 += (k / n) as f64 * cell;
            count += 1.;
        }
    }
    if count > 0. {
        centre.0 = centre.0 / count + cell / 2.;
        centre.1 = centre.1 / count + cell / 2.;
    } else {
        centre = (n as f64 * cell / 2., n as f64 * cell / 2.);
    }
    let safe = |p: [f64; 2]| {
        let (x, y) = local.to_local(p);
        [
            (0., 0.),
            (350., 350.),
            (-350., 350.),
            (350., -350.),
            (-350., -350.),
        ]
        .iter()
        .all(|(dx, dy)| !hazard(local.to_lon_lat(x + dx, y + dy)).blocked())
    };
    let mut exposure_targets: Vec<[f64; 2]> = Vec::new();
    let mut clusters: Vec<[f64; 2]> = Vec::new();
    for site in exposures {
        if site["automation_eligible"] != true
            || !site["p_fire_6h"].as_f64().is_some_and(|p| p >= 0.2)
        {
            continue;
        }
        let Ok(position) = serde_json::from_value::<[f64; 2]>(site["position"].clone()) else {
            continue;
        };
        if clusters.iter().any(|p| haversine_m(*p, position) < 1000.) {
            continue;
        }
        clusters.push(position);
        let (x, y) = local.to_local(position);
        let point = if safe(position) {
            Some(position)
        } else {
            [1000., 2000., 3000.].into_iter().find_map(|r| {
                [
                    bearing + std::f64::consts::PI,
                    bearing + std::f64::consts::FRAC_PI_2,
                    bearing - std::f64::consts::FRAC_PI_2,
                ]
                .into_iter()
                .map(|a| local.to_lon_lat(x + r * a.sin(), y + r * a.cos()))
                .find(|p| safe(*p))
            })
        };
        if let Some(point) = point {
            exposure_targets.push(point);
        }
    }
    let mut tasks = exposure_targets.clone();
    for sector in 0..8 {
        let angle = sector as f64 * std::f64::consts::FRAC_PI_4;
        let mut radius = 0.0_f64;
        for (k, p) in burned.iter().enumerate() {
            if p.as_f64().unwrap_or(0.) < 0.5 {
                continue;
            }
            let (dx, dy) = (
                ((k % n) as f64 + 0.5) * cell - centre.0,
                ((k / n) as f64 + 0.5) * cell - centre.1,
            );
            if (dx.atan2(dy) - angle).cos() > 0.707 {
                radius = radius.max(dx.hypot(dy));
            }
        }
        for extra in [1500., 2000., 3000., 4000.] {
            let p = local.to_lon_lat(
                centre.0 + (radius + extra) * angle.sin(),
                centre.1 + (radius + extra) * angle.cos(),
            );
            if safe(p) {
                tasks.push(p);
                break;
            }
        }
    }
    let mut templates: Vec<(usize, [f64; 2], RoutedMission)> = Vec::new();
    for task in tasks {
        let mut best = None;
        for (si, s) in stations.iter().enumerate() {
            if let Ok(route) = routing::flight(dem, s.position, task, wind, gust, cfg, &hazard)
                && best
                    .as_ref()
                    .is_none_or(|(_, _, r): &(usize, [f64; 2], RoutedMission)| {
                        route.route.duration_s < r.route.duration_s
                    })
            {
                best = Some((si, task, route));
            }
        }
        if best.is_none() {
            let (tx, ty) = local.to_local(task);
            for direction in [
                bearing + std::f64::consts::PI,
                bearing + std::f64::consts::FRAC_PI_2,
                bearing - std::f64::consts::FRAC_PI_2,
            ] {
                let launch =
                    local.to_lon_lat(tx + 3000. * direction.sin(), ty + 3000. * direction.cos());
                if !safe(launch) {
                    continue;
                }
                let Ok(route) = routing::flight(dem, launch, task, wind, gust, cfg, &hazard) else {
                    continue;
                };
                for (si, s) in stations.iter().enumerate() {
                    let candidate = routing::with_ground_transfer(
                        RoutedMission {
                            route: route.route.clone(),
                            transit_s: route.transit_s,
                        },
                        dem,
                        s.position,
                    );
                    if let Ok(candidate) = candidate
                        && best.as_ref().is_none_or(
                            |(_, _, r): &(usize, [f64; 2], RoutedMission)| {
                                candidate.route.duration_s < r.route.duration_s
                            },
                        )
                    {
                        best = Some((si, task, candidate));
                    }
                }
            }
        }
        if let Some(best) = best {
            templates.push(best);
        }
    }
    if templates.is_empty() {
        for s in stations {
            plan.held.push(Held{station_id:s.id.clone(),reason:"No feasible smoke/fire-clear route with known terrain, weather and return reserve".into()});
        }
        return Ok(plan);
    }
    let transit = templates.iter().map(|t| t.2.transit_s).fold(0., f64::max);
    let cluster_count = exposure_targets
        .iter()
        .filter(|p| templates.iter().any(|t| t.1 == **p))
        .count();
    let sizing = fleet::size(
        grid["front_1h_m"]
            .as_f64()
            .ok_or_else(|| failure("front length"))?,
        grid["head_sectors"].as_u64().unwrap_or(1) as usize,
        cluster_count,
        transit,
        cfg,
    )?;
    plan.count = sizing.count;
    plan.breakdown = sizing.breakdown;
    let mut available = stations
        .iter()
        .map(|s| s.drones_available)
        .collect::<Vec<_>>();
    let airborne = sizing.airborne_perimeter + sizing.airborne_head + sizing.airborne_exposure;
    for i in 0..plan.count {
        let role = if i >= airborne {
            "reserve"
        } else if i < sizing.airborne_head {
            "head"
        } else if i < sizing.airborne_head + sizing.airborne_exposure {
            "exposure"
        } else {
            "perimeter"
        };
        let candidates = templates
            .iter()
            .filter(|t| (role == "exposure") == exposure_targets.contains(&t.1))
            .collect::<Vec<_>>();
        let (si, task, r) = if candidates.is_empty() {
            &templates[i % templates.len()]
        } else {
            candidates[i % candidates.len()]
        };
        // All templates may prefer one base. Assign overflow to another sourced
        // hypothetical station with an explicit recomputed schematic transfer.
        let chosen = if available[*si] > 0 {
            Some(*si)
        } else {
            available.iter().position(|n| *n > 0)
        };
        let Some(chosen) = chosen else {
            break;
        };
        available[chosen] -= 1;
        let route = if role == "reserve" {
            None
        } else if chosen == *si {
            Some(r.route.clone())
        } else {
            // Reuse only the flight portion, never another station's ground leg.
            let flight_leg = r.route.legs.iter().find(|l| l.kind == "flight").unwrap();
            let mut path = r.route.waypoints[flight_leg.from..=flight_leg.to].to_vec();
            let offset = path[0].t_s;
            for p in &mut path {
                p.t_s -= offset;
            }
            let mut route = r.route.clone();
            route.waypoints = path;
            route.legs = vec![crate::fleet::Leg {
                kind: "flight".into(),
                from: 0,
                to: route.waypoints.len() - 1,
            }];
            route.duration_s = route.flight_duration_s;
            route.ground_duration_s = 0.;
            route.distance_m = r.route.waypoints[flight_leg.from..=flight_leg.to]
                .windows(2)
                .map(|p| {
                    haversine_m(
                        [p[0].position[0], p[0].position[1]],
                        [p[1].position[0], p[1].position[1]],
                    )
                })
                .sum();
            route.straight_line_m = 2.
                * haversine_m(
                    [
                        route.waypoints[0].position[0],
                        route.waypoints[0].position[1],
                    ],
                    *task,
                );
            routing::with_ground_transfer(
                RoutedMission {
                    route,
                    transit_s: r.transit_s,
                },
                dem,
                stations[chosen].position,
            )
            .ok()
            .map(|r| r.route)
        };
        plan.drones.push(Drone {
            id: format!(
                "{}-{:02}",
                stations[chosen].id,
                stations[chosen].drones_available - available[chosen]
            ),
            station_id: stations[chosen].id.clone(),
            role: role.into(),
            route,
            task_point: *task,
        });
    }
    plan.breakdown.push(Term {
        term: "allocated".into(),
        value: plan.drones.len() as f64,
        unit: "drones".into(),
        basis: "configured hypothetical station inventory".into(),
        source: None,
    });
    if plan.drones.len() < plan.count {
        plan.held.push(Held {
            station_id: "all".into(),
            reason: format!(
                "Demand {}, available inventory {}; no additional aircraft invented",
                plan.count,
                plan.drones.len()
            ),
        });
    }
    plan.change_reason_en = Some(format!(
        "Simulated demand {} (previous {:?}); re-sized from the one-hour model front, grounded exposure sites and feasible transit. {} assigned, including ground reserves.",
        plan.count,
        previous,
        plan.drones.len()
    ));
    plan.change_reason_pt = Some(format!(
        "Necessidade simulada: {} drones; {} atribuídos, incluindo reservas. Recalculado pela frente prevista, locais expostos e tempo de trânsito.",
        plan.count,
        plan.drones.len()
    ));
    Ok(plan)
}

pub struct Export {
    pub index: Value,
    pub frames: Vec<Value>,
}

/// An observation samples an already-planned simulated flight at this cursor;
/// it never reports a future task as a completed sensor observation.
fn edge_distance(p: [f64; 2], geometry: &Value) -> Option<f64> {
    fn visit(p: [f64; 2], v: &Value, best: &mut f64) {
        let Some(values) = v.as_array() else {
            return;
        };
        if values.first().is_some_and(|v| v[0].is_number()) {
            let local = LocalFrame { origin_lon_lat: p };
            for pair in values.windows(2) {
                let (Ok(a), Ok(b)) = (
                    serde_json::from_value::<[f64; 2]>(pair[0].clone()),
                    serde_json::from_value::<[f64; 2]>(pair[1].clone()),
                ) else {
                    continue;
                };
                let (ax, ay) = local.to_local(a);
                let (bx, by) = local.to_local(b);
                let (dx, dy) = (bx - ax, by - ay);
                let length2 = dx * dx + dy * dy;
                let f = if length2 > 0. {
                    (-(ax * dx + ay * dy) / length2).clamp(0., 1.)
                } else {
                    0.
                };
                *best = best.min((ax + f * dx).hypot(ay + f * dy));
            }
        } else {
            for child in values {
                visit(p, child, best);
            }
        }
    }
    let mut best = f64::INFINITY;
    visit(p, &geometry["coordinates"], &mut best);
    best.is_finite().then_some(best)
}
fn observation(
    frame: &Value,
    planned: DateTime<Utc>,
    drone: &Drone,
    at: DateTime<Utc>,
    sites: &[Value],
) -> Option<String> {
    let route = drone.route.as_ref()?;
    let elapsed = (at - planned).num_seconds() as f64;
    let leg = route.legs.iter().find(|l| l.kind == "flight")?;
    let path = &route.waypoints;
    if elapsed < 0. || path.len() < 2 {
        return None;
    }
    let phase = if elapsed >= route.duration_s {
        "returned / regressado"
    } else if elapsed >= path[leg.from].t_s && elapsed <= path[leg.to].t_s {
        "flight / voo"
    } else {
        "schematic ground transfer / transporte terrestre esquemático"
    };
    let elapsed = elapsed.clamp(path.first()?.t_s, path.last()?.t_s);
    let pair = path
        .windows(2)
        .find(|pair| pair[0].t_s <= elapsed && elapsed <= pair[1].t_s)?;
    let f = if pair[1].t_s > pair[0].t_s {
        (elapsed - pair[0].t_s) / (pair[1].t_s - pair[0].t_s)
    } else {
        0.
    };
    let p = [
        pair[0].position[0] + f * (pair[1].position[0] - pair[0].position[0]),
        pair[0].position[1] + f * (pair[1].position[1] - pair[0].position[1]),
    ];
    let origin: [f64; 2] =
        serde_json::from_value(frame["planning"]["origin_lon_lat"].clone()).ok()?;
    let (x, y) = LocalFrame {
        origin_lon_lat: origin,
    }
    .to_local(p);
    let smoke = match (
        serde_json::from_value::<PlumeSource>(frame["planning"]["plume"]["source"].clone()),
        serde_json::from_value::<PlumeMet>(frame["planning"]["plume"]["met"].clone()),
    ) {
        (Ok(source), Ok(met)) => concentration(&source, &met, &summarise(&source, &met), x, y),
        _ if frame["weather"].is_object() => 0.,
        _ => return None,
    };
    let wind = frame["weather"]["wind_mps"].as_f64()?;
    let probability = probability(&frame["planning"], "burn_6h", p);
    let edge = edge_distance(p, &frame["predicted"]["p50"])
        .map_or("unknown".into(), |m| format!("{:.1} km", m / 1000.));
    let nearby = sites
        .iter()
        .filter(|s| s["automation_eligible"] == true)
        .filter_map(|s| {
            serde_json::from_value::<[f64; 2]>(s["position"].clone())
                .ok()
                .filter(|q| haversine_m(p, *q) <= 2000.)
                .and(s["name"].as_str())
        })
        .take(3)
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "{} · {} · {} · simulated position {:.5}, {:.5}; distance to model fire edge {}; model surface PM2.5 {:.1} µg/m³; retrospective wind {:.1} m/s; six-hour burn probability {}; mapped verified exposure sites within 2 km: [{}]. No real sensor measurement.",
        drone.id,
        drone.role,
        phase,
        p[0],
        p[1],
        edge,
        smoke,
        wind,
        probability.map_or("unknown".into(), |p| format!("{:.0}%", 100. * p)),
        nearby
    ))
}
pub fn build(
    mut index: Value,
    replay: &Value,
    ds: &Dataset,
    stations: &[Station],
    dem: &Dem,
    cfg: &Airframe,
) -> Result<Export, FirecastError> {
    let start = date(&index["window"]["from"]).ok_or_else(|| failure("window start"))?;
    let end = date(&index["window"]["to"]).ok_or_else(|| failure("window end"))?;
    let mut frames = Vec::new();
    let mut previous = None;
    let mut summaries = Vec::new();
    let mut missions: BTreeMap<String, (DateTime<Utc>, Drone)> = BTreeMap::new();
    let detection = index["timeline"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| date(&s["at"]))
        .min()
        .ok_or_else(|| failure("official timeline"))?;
    for fr in replay["frames"]
        .as_array()
        .ok_or_else(|| failure("replay frames"))?
    {
        let at = date(&fr["t"]).ok_or_else(|| failure("frame time"))?;
        if at < start || at > end {
            continue;
        }
        let sites = exposures(&index, fr, at);
        let fleet = planned_fleet(fr, stations, dem, cfg, &sites, previous, at >= detection)?;
        previous = Some(fleet.count);
        let mut out = fr.clone();
        out.as_object_mut().unwrap().remove("planning");
        out.as_object_mut().unwrap().remove("drones");
        out["mode"] = json!("historical");
        out["fleet"] = json!(fleet);
        out["exposures"] = json!(sites);
        out["sitreps"] = json!([]);
        out["fleet_check"] = Value::Null;
        let evidence = index["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| date(&e["known_at"]).is_some_and(|t| t <= at))
            .collect::<Vec<_>>();
        out["evidence_ids"] = json!(evidence.iter().map(|e| &e["id"]).collect::<Vec<_>>());
        out["fogos_status"] = index["timeline"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| date(&s["at"]).is_some_and(|t| t <= at))
            .max_by_key(|s| date(&s["at"]))
            .cloned()
            .unwrap_or(Value::Null);
        let mut automation = Vec::new();
        for (planned, drone) in missions.values() {
            if let Some(detail) = observation(fr, *planned, drone, at, &sites) {
                automation.push(json!({"step":"first_report","at":at,"basis":"simulated",
                    "detail":detail,"detail_en":detail,"detail_pt":format!("Observação simulada: {}. Fumo/vento/probabilidade são valores do modelo; sem medição real.",drone.id),
                    "url":null,"sources":[],"latency_s":(at-detection).num_seconds()}));
            }
        }
        for drone in &fleet.drones {
            if drone.route.is_some()
                && missions.get(&drone.id).is_none_or(|(planned, d)| {
                    d.route
                        .as_ref()
                        .is_none_or(|r| (at - *planned).num_seconds() as f64 >= r.duration_s)
                })
            {
                missions.insert(drone.id.clone(), (at, drone.clone()));
            }
        }
        if at >= detection {
            automation.push(json!({"step":"reported","at":detection,"basis":"observed","detail":"Dated official status on the captured Fogos.pt page","detail_en":"Official incident recorded","detail_pt":"Ocorrência oficial registada","url":index["ignition"]["url"],"latency_s":0}));
            for (step, en, pt) in [
                (
                    "forecast",
                    "Experimental fire/smoke model at the cursor",
                    "Modelo experimental de fogo/fumo no instante",
                ),
                (
                    "fleet_sized",
                    "Fleet formula recalculated from current inputs",
                    "Fórmula da frota recalculada com dados atuais",
                ),
            ] {
                automation.push(json!({"step":step,"at":at,"basis":"simulated","detail":en,"detail_en":en,"detail_pt":pt,"url":null,"latency_s":(at-detection).num_seconds()}));
            }
        }
        let launched = fleet
            .drones
            .iter()
            .filter(|d| d.route.as_ref().is_some_and(|r| r.ground_duration_s == 0.))
            .count();
        if launched > 0 {
            automation.push(json!({"step":"launched","at":at,"basis":"simulated","detail":format!("{launched} aircraft assigned direct flight routes"),"detail_en":format!("{launched} direct simulated flights"),"detail_pt":format!("{launched} voos diretos simulados"),"url":null,"latency_s":(at-detection).num_seconds()}));
        }
        out["automation"] = json!(automation);
        let file = format!("frames/{:02}.json", frames.len());
        summaries.push(json!({"t":at,"file":file,"evidence":evidence.len(),"exposures":sites.len(),"area_ha_p50":fr["model"]["area_ha_p50"]}));
        frames.push(out);
    }
    index["frames"] = json!(summaries);
    index["stations"] = json!(stations);
    index["airframe"] = json!(cfg);
    index["weather_source"] = json!(ds.weather_source);
    index["model"] = replay["meta"]["model"].clone();
    index["generator"] = json!("Rust early_demo; no network calls or external model");
    Ok(Export { index, frames })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evidence_after_cursor_and_unknown_probabilities_do_not_leak() {
        let at: DateTime<Utc> = "2025-08-13T12:00:00Z".parse().unwrap();
        let index = json!({"ignition":{"position":[-8.,40.]},"evidence":[{"id":"future","known_at":"2025-08-13T12:01:00Z","category":"evacuation","places":[{"name":"Future place","position":[-8.,40.]}]}]});
        assert!(exposures(&index, &json!({}), at).is_empty());
        let grid = json!({"origin_lon_lat":[-8.,40.],"size":1,"cell_m":500.,"burn_6h":[0.25]});
        let p = LocalFrame {
            origin_lon_lat: [-8., 40.],
        }
        .to_lon_lat(250., 250.);
        assert_eq!(probability(&grid, "burn_6h", p), Some(0.25));
        assert_eq!(probability(&grid, "burn_6h", [-9., 40.]), None);
    }
}
