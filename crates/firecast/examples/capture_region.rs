//! One-shot public terrain capture, separate from the fully offline demo build.
use emberwatch_firecast::{fleet::Station, geo::LocalFrame, sources::HttpSources};
use serde_json::{Value, json};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let stations: Vec<Station> = serde_json::from_slice(&std::fs::read(
        args.first()
            .ok_or("capture_region <stations.json> <output.json>")?,
    )?)?;
    if stations.is_empty() {
        return Err("No sourced stations".into());
    }
    let output = args.get(1).ok_or("Missing output")?;
    if std::path::Path::new(output).exists() {
        return Err("Refusing to overwrite an existing terrain capture".into());
    }
    let dataset: Value = serde_json::from_slice(&std::fs::read(
        "data/historical/arganil-piodao-2025/dataset.json",
    )?)?;
    let ignition: [f64; 2] = serde_json::from_value(dataset["ignition_position"].clone())?;
    let mut points = stations.iter().map(|s| s.position).collect::<Vec<_>>();
    points.push(ignition);
    let west = points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min) - 0.12;
    let east = points
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max)
        + 0.12;
    let south = points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min) - 0.1;
    let north = points
        .iter()
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max)
        + 0.1;
    let frame = LocalFrame {
        origin_lon_lat: [west, south],
    };
    let (w, h) = frame.to_local([east, north]);
    let cell = 500.;
    let width = (w / cell).ceil() as usize;
    let height = (h / cell).ceil() as usize;
    let dem = HttpSources::new()?
        .dem_tiles_at_zoom(frame, width, height, cell, 9)
        .await?;
    let document = json!({"origin_lon_lat":frame.origin_lon_lat,"width":width,"height":height,
        "cell_m":cell,"elevation_m":dem.elevation_m,"captured_at":chrono::Utc::now(),
        "source":"https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png",
        "zoom":9,"attribution":"Terrain Tiles (Mapzen/AWS Open Data); underlying sources vary by tile; see https://github.com/tilezen/joerd/blob/master/docs/attribution.md",
        "note":"Sampled regional planning terrain, not a clearance survey. No synthetic fallback."});
    std::fs::write(output, serde_json::to_vec(&document)?)?;
    println!("Captured {width} x {height} real terrain samples");
    Ok(())
}
