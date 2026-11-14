//! Download and assemble the Arganil / Piódão 2025 historical dataset (network):
//! `cargo run --release -p emberwatch-firecast --example build_dataset`
//! Writes data/historical/arganil-piodao-2025/dataset.json.
use emberwatch_firecast::{HttpSources, historical::build_arganil_2025};

#[tokio::main]
async fn main() {
    let sources = HttpSources::new().expect("http client");
    let dataset = build_arganil_2025(&sources)
        .await
        .unwrap_or_else(|e| panic!("dataset build failed: {e}"));
    dataset.validate().expect("valid dataset");
    let dir = std::path::Path::new("data/historical").join(&dataset.id);
    std::fs::create_dir_all(&dir).expect("create dir");
    let path = dir.join("dataset.json");
    std::fs::write(&path, serde_json::to_vec(&dataset).expect("json")).expect("write");
    let hours: usize = dataset.weather.points.iter().map(|p| p.hours.len()).sum();
    println!(
        "{}: {} perimeters ({} → {} ha), {} hourly weather records at {} points, {} events, DEM {}×{} → {}",
        dataset.name,
        dataset.perimeters.len(),
        dataset.perimeters.first().map_or(0.0, |p| p.area_ha),
        dataset.perimeters.last().map_or(0.0, |p| p.area_ha),
        hours,
        dataset.weather.points.len(),
        dataset.events.len(),
        dataset.domain.size,
        dataset.domain.size,
        path.display()
    );
}
