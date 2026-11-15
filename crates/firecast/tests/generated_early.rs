//! Offline acceptance checks over the exact artifact handed to the UI.
use chrono::{DateTime, Utc};
use emberwatch_firecast::{
    fleet::Airframe,
    routing::{RegionalTerrain, terrain_at},
};
use serde_json::Value;
use std::collections::BTreeMap;
fn read(path: impl AsRef<std::path::Path>) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
#[test]
fn generated_replay_preserves_physics_cursor_scope_and_flight_constraints() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repo.join("web/public/generated/arganil-early");
    let index = read(root.join("index.json"));
    let cfg: Airframe = serde_json::from_value(index["airframe"].clone()).unwrap();
    let terrain: RegionalTerrain = serde_json::from_value(read(
        repo.join("data/stations/arganil-regional-terrain.json"),
    ))
    .unwrap();
    let dem = terrain.dem().unwrap();
    let baseline = read(repo.join("web/public/historical/arganil-piodao-2025.json"));
    let evidence = index["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["id"].as_str().unwrap(), e))
        .collect::<BTreeMap<_, _>>();
    let summaries = index["frames"].as_array().unwrap();
    assert_eq!(summaries.len(), 20);
    let mut previous: Option<DateTime<Utc>> = None;
    let (mut routes, mut reports, mut mapped) = (0, 0, 0);
    for (i, summary) in summaries.iter().enumerate() {
        let f = read(root.join(summary["file"].as_str().unwrap()));
        let at = f["t"].as_str().unwrap().parse::<DateTime<Utc>>().unwrap();
        if let Some(previous) = previous {
            assert_eq!((at - previous).num_minutes(), if i <= 6 { 30 } else { 180 });
        }
        previous = Some(at);
        assert_eq!(f["mode"], "historical");
        assert_eq!(f["fleet_check"], Value::Null);
        assert!(f["sitreps"].as_array().unwrap().is_empty());
        if let Some(old) = baseline["frames"]
            .as_array()
            .unwrap()
            .iter()
            .find(|old| old["t"] == f["t"])
        {
            assert_eq!(
                f["predicted"], old["predicted"],
                "spread geometry changed at {}",
                f["t"]
            );
            assert_eq!(
                f["model"], old["model"],
                "model values changed at {}",
                f["t"]
            );
        }
        for id in f["evidence_ids"].as_array().unwrap() {
            let e = evidence[id.as_str().unwrap()];
            assert!(
                e["known_at"]
                    .as_str()
                    .unwrap()
                    .parse::<DateTime<Utc>>()
                    .unwrap()
                    <= at
            );
        }
        for event in f["automation"].as_array().unwrap() {
            assert!(
                event["at"]
                    .as_str()
                    .unwrap()
                    .parse::<DateTime<Utc>>()
                    .unwrap()
                    <= at
            );
            if event["step"] == "first_report" {
                reports += 1;
                assert_eq!(event["basis"], "simulated");
            }
        }
        for site in f["exposures"].as_array().unwrap() {
            if site["position"].is_array() {
                mapped += 1;
            }
            if site["people_basis"] == "kind_default" {
                assert!(
                    site["people"].is_null(),
                    "assumption misrepresented as stated attendance"
                );
            }
        }
        for drone in f["fleet"]["drones"].as_array().unwrap() {
            let route = &drone["route"];
            if route.is_null() {
                continue;
            }
            routes += 1;
            assert!(
                route["flight_duration_s"].as_f64().unwrap()
                    <= cfg.endurance_s * (1. - cfg.reserve_fraction) + 1e-6
            );
            assert!(
                route["reserve_pct_at_return"].as_f64().unwrap()
                    >= 100. * cfg.reserve_fraction - 1e-6
            );
            assert!(route["max_smoke_ugm3"].as_f64().unwrap() < 150.);
            let points = route["waypoints"].as_array().unwrap();
            for pair in points.windows(2) {
                assert!(pair[0]["t_s"].as_f64().unwrap() <= pair[1]["t_s"].as_f64().unwrap());
            }
            for leg in route["legs"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|l| l["kind"] == "flight")
            {
                for p in &points
                    [leg["from"].as_u64().unwrap() as usize..=leg["to"].as_u64().unwrap() as usize]
                {
                    let ll = [
                        p["position"][0].as_f64().unwrap(),
                        p["position"][1].as_f64().unwrap(),
                    ];
                    assert!(
                        p["position"][2].as_f64().unwrap()
                            >= terrain_at(&dem, ll).unwrap() + cfg.flight_agl_m - 0.01
                    );
                }
            }
        }
    }
    assert!(
        routes > 0 && reports > 0 && mapped > 0,
        "expected routed missions, simulated reports and sourced exposures"
    );
}
