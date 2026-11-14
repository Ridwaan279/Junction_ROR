use emberwatch_domain::platform::*;
use schemars::{JsonSchema, schema_for};

#[derive(JsonSchema)]
#[allow(dead_code)]
struct Contracts {
    evidence: Evidence,
    weather: WeatherSample,
    incident: Incident,
    source: SourceDescriptor,
    batch: SourceBatch,
    scenario_spec: ScenarioSpec,
    dataset: ScenarioDataset,
    scenario: Scenario,
    event: EventEnvelope,
    forecast: Forecast,
    drone_plan: DronePlan,
    briefing: Briefing,
}

fn main() {
    println!(
        "{}",
        serde_json::to_string_pretty(&schema_for!(Contracts)).expect("schema serialization")
    );
}
