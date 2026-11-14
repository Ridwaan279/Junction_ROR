use emberwatch_domain::{EventEnvelope, Snapshot, SourceBatch};
use schemars::schema_for;
fn main() {
    for (name, schema) in [
        ("snapshot", schema_for!(Snapshot)),
        ("source-batch", schema_for!(SourceBatch)),
        ("event", schema_for!(EventEnvelope)),
    ] {
        std::fs::write(
            format!("contracts/{name}.schema.json"),
            serde_json::to_string_pretty(&schema).unwrap(),
        )
        .unwrap();
    }
}
