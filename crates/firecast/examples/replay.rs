//! Multi-day replay of a historical dataset (offline):
//! `cargo run --release -p emberwatch-firecast --example replay -- data/historical/arganil-piodao-2025/dataset.json web/public/historical/arganil-piodao-2025.json`
use emberwatch_firecast::historical::{Dataset, ReplayConfig, replay};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let input = args.first().map_or(
        "data/historical/arganil-piodao-2025/dataset.json",
        String::as_str,
    );
    let output = args.get(1).map_or(
        "web/public/historical/arganil-piodao-2025.json",
        String::as_str,
    );
    let dataset: Dataset = serde_json::from_slice(&std::fs::read(input).expect("read dataset"))
        .expect("parse dataset");
    let result = replay(&dataset, &ReplayConfig::default()).expect("replay");
    if let Some(dir) = std::path::Path::new(output).parent() {
        std::fs::create_dir_all(dir).expect("output dir");
    }
    std::fs::write(output, serde_json::to_vec(&result).expect("json")).expect("write");
    println!(
        "frames: {}, runtime {} ms",
        result["frames"].as_array().map_or(0, Vec::len),
        result["meta"]["runtime_ms"]
    );
    for v in result["validation"].as_array().into_iter().flatten() {
        println!(
            "{} → {} ({:>5} h, from {:<18}) observed {:>6} ha, predicted p50 {:>6} ha [{}–{}], Dice total {}, growth {}, growth in 10% band {}, bearing obs {} / model {}",
            v["from"].as_str().unwrap_or("").get(5..16).unwrap_or(""),
            v["to"].as_str().unwrap_or("").get(5..16).unwrap_or(""),
            v["hours"],
            v["started_from"].as_str().unwrap_or(""),
            v["observed_area_ha"],
            v["predicted_area_ha_p50"],
            v["predicted_area_ha_p10"],
            v["predicted_area_ha_p90"],
            v["dice_total"],
            v["dice_growth"],
            v["growth_within_10pct_band"],
            v["observed_growth_bearing_deg"],
            v["predicted_growth_bearing_deg"],
        );
    }
}
