use emberwatch_domain::{FixedClock, RunContext, Snapshot};
use emberwatch_response::{
    Cell, CellPosition, Grid, SpreadInput, WindVector, demo_bundle, simulate_spread,
};
use std::{sync::Arc, time::Instant};

fn main() {
    let snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../contracts/fixtures/snapshot.json"))
            .expect("fixture");
    let scenario = &snapshot.scenario;
    let context = RunContext {
        run_id: scenario.run_id.clone(),
        scenario_id: Some(scenario.id.clone()),
        mode: scenario.mode,
        epoch: scenario.epoch,
        bbox: emberwatch_domain::PORTUGAL_BBOX,
        window_start: scenario.start_at,
        window_end: scenario.end_at,
        clock: Arc::new(FixedClock(scenario.current_at)),
    };
    let mut samples = Vec::new();
    for _ in 0..30 {
        let started = Instant::now();
        let result = demo_bundle(
            &context,
            &snapshot.incidents[0],
            &snapshot.evidence,
            &snapshot.weather,
        )
        .expect("synthetic bundle");
        std::hint::black_box(result);
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "synthetic bundle (21x21 grid, 8 spread steps, A* round trip, GeoJSON+PT/EN): p50={:.3} ms p95={:.3} ms max={:.3} ms n=30",
        samples[15], samples[28], samples[29]
    );
    let grid = Grid::new(120, 120, 500.0, vec![Cell::default(); 14400]).expect("grid");
    let started = Instant::now();
    let result = simulate_spread(
        &grid,
        &SpreadInput {
            seed: 760,
            ignitions: vec![CellPosition { x: 60, y: 60 }],
            steps: 60,
            step_seconds: 60,
            base_probability: 0.3,
            wind: WindVector {
                east_mps: 4.0,
                north_mps: 0.0,
            },
        },
    )
    .expect("spread");
    println!(
        "60km 120x120 spread, 60 steps: {:.3} ms; reached {} cells",
        started.elapsed().as_secs_f64() * 1000.0,
        result.burned_cells.len()
    );
    println!(
        "arch={} parallelism={} build={}",
        std::env::consts::ARCH,
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
}
