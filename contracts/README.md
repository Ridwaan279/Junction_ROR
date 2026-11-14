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

## Merged Agent A interface

The browser wire types above remain stable. `/platform/ui` exposes that contract
over Agent A's persistent engine; configure NEXT_PUBLIC_API_BASE_URL accordingly.
Its scenario/run IDs are UUID strings, and all events embed a complete Snapshot.
Wind is converted from the native contract's km/h to the browser's m/s.

Agent A's native API remains at `/platform`, described by `openapi.yaml` and
`schema.json`. Its Rust DTOs live in `emberwatch_domain::platform`, with point
objects and native UUIDs; browser DTOs remain at the domain crate root. Do not
interchange these JSON shapes. Native reset URLs resolve below `/platform/api/runs`.
Regenerate its schema with `cargo run -p emberwatch-domain --example export_schema`.
The workspace keeps its existing pinned schemars 0.8; native schema refs therefore
use `definitions`, consistently with `scripts/check_contracts.py` and OpenAPI.
