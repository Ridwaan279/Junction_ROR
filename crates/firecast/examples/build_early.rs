//! Offline source-grounded demo generation. Full article text stays private.
use emberwatch_firecast::{
    early_demo,
    fleet::{Airframe, Station},
    historical::Dataset,
    routing::RegionalTerrain,
};
use serde_json::Value;
fn read<T: serde::de::DeserializeOwned>(
    path: impl AsRef<std::path::Path>,
) -> Result<T, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let input = args
        .first()
        .ok_or("build_early <Rust replay.json> [web/public/generated/arganil-early]")?;
    let output = std::path::PathBuf::from(
        args.get(1)
            .map_or("web/public/generated/arganil-early", String::as_str),
    );
    let generated =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/public/generated");
    std::fs::create_dir_all(&generated)?;
    let absolute = if output.is_absolute() {
        output
    } else {
        std::env::current_dir()?.join(output)
    };
    // A command-line typo must never overwrite Agent 1's fixture or UI files.
    let parent = absolute.parent().ok_or("output parent")?;
    std::fs::create_dir_all(parent)?;
    if !parent
        .canonicalize()?
        .starts_with(generated.canonicalize()?)
        || absolute
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Output must stay under web/public/generated".into());
    }
    let raw = std::env::var("EMBERWATCH_GOLDEN_RAW_DIR")
        .map_err(|_| "Set EMBERWATCH_GOLDEN_RAW_DIR to the private verified-originals directory")?;
    let reference: Value = read("web/public/fixtures/v2/arganil-early/index.json")?;
    let pages: Value = read("data/demo/arganil-early/context_dev/pages.json")?;
    let index = early_demo::verified_index(&reference, &pages, std::path::Path::new(&raw))?;
    let replay: Value = read(input)?;
    let dataset: Dataset = read("data/historical/arganil-piodao-2025/dataset.json")?;
    let stations: Vec<Station> = read("data/stations/portugal.json")?;
    let terrain: RegionalTerrain = read("data/stations/arganil-regional-terrain.json")?;
    let airframe = Airframe::default();
    let result = early_demo::build(
        index,
        &replay,
        &dataset,
        &stations,
        &terrain.dem()?,
        &airframe,
    )?;
    std::fs::create_dir_all(absolute.join("frames"))?;
    for (i, frame) in result.frames.iter().enumerate() {
        std::fs::write(
            absolute.join(format!("frames/{i:02}.json")),
            serde_json::to_vec(frame)?,
        )?;
    }
    std::fs::write(
        absolute.join("index.json"),
        serde_json::to_vec_pretty(&result.index)?,
    )?;
    println!(
        "Wrote {} source-grounded frames; human benchmark omitted by request",
        result.frames.len()
    );
    Ok(())
}
