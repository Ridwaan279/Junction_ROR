# Shared contract v1

Domain wire types live in `crates/domain/src/lib.rs`; JSON Schemas are generated
with `cargo run -p emberwatch-domain --example export_contracts` from the repo root.
All HTTP lists are bare JSON arrays. Every ID route requires `?run_id=...`.
`GET /api/snapshot?run_id=...` returns Snapshot and is the resync authority.
Scenario commands return a fresh Snapshot. `POST .../seek` accepts
`{"at":"2026-11-14T12:10:00Z"}`; play optionally accepts `{"speed":60}`.

SSE sends the full EventEnvelope JSON as data and its type as the SSE event name.
Connect to `/api/events?run_id=...`. On `run.reset`, replace all derived client
state from `payload` (a Snapshot); discard lower epochs and wrong run IDs.
On network reconnect or `resync.required`, fetch Snapshot again. Named events
must be explicitly subscribed to; `onmessage` alone does not receive them.

`fixtures/snapshot.json` contains ONLY synthetic evidence, not a real fire report.
It needs no credentials or rights to captured Fogos data. Simulation outputs are
initially empty. Future source observations are in `fixtures/replay-evidence.json`;
release them only at provenance.available_at, never event_at alone.

Null fields mean unknown. A failed/partial page is not a verified empty result.
GeoJSON positions are [longitude, latitude], distances metres, wind m/s and
meteorological degrees FROM. Timestamp display uses Europe/Lisbon.
