# Agent integration contract, version 0.1.0

Agent A owns the workspace manifests/lockfile, domain types, persistence,
router registration, SSE and these contracts. Propose shared changes together
with the affected consumers and example fixtures. The existing three crates
are working implementations; Agent B/C/D directories remain theirs to create.

## Agent B

Create crates/intelligence as a library depending on emberwatch-domain.
Implement Source::descriptor and Source::run, returning a SourceBatch with
matching source_id on all records. Register Arc<dyn Source> adapters through
Platform::open in the server composition root; coordinate the workspace/member
and route changes with A. The server already exposes source metadata, enable,
run, scheduling, ingestion and persistence.

Each evidence item needs a source-local external_id, upstream origin_group,
URL, title, location text, optional coordinates, radius and explicit availability
time/basis. Repeated capture time is excluded from content identity. Changed
content appends a revision, and replay chooses the latest revision available at
its cursor. Preserve the same origin_group for copied ANEPC reports; do not
mislabel mirrored satellite data as independent FIRMS evidence.

No observed coordinates means point=null; such evidence is retained but does
not create a map incident until a separately labelled geocode is available.
Source errors must not contain credential-bearing URLs. Return complete=false
for partial extraction. explicit_empty=true requires complete coverage and zero
records. Sources lacking setup/policy checks stay unvalidated and cannot run.
FIRMS and Open-Meteo are Agent A adapters; IPMA and Context/Fogos are yours.

The Context client should own per-request credit accounting/reservation before
requests. SourceBatch does not yet contain provider usage metadata; coordinate
that contract addition with A when the client is integrated. The current
platform does not make any Context.dev requests.

Webhook handlers belong in your library; A mounts the router. Verify raw-body
signatures and deduplicate receipts before producing evidence. No webhook route
is advertised as working on this branch.

## Agent C

Create crates/response as a library consuming domain Incident, Evidence,
WeatherSample and the run's cursor/seed/epoch. It must not own SQL or another
HTTP server. Forecast, DronePlan and Briefing output contracts are already
published for review. Coordinate any richer task/terrain input contract with A.

Point coordinates are WGS84 longitude/latitude. Wind is km/h and the
meteorological FROM direction, clockwise from north. Gusts can be null.
Do metre-based grid calculations in a local projection; do CPU work off Tokio's
event loop. Every result includes run_id and epoch; discard work computed for a
superseded epoch. Agent A will add persistence/routes around your functions.

The forecast/drone/dispatch HTTP routes intentionally return 501 until your
implementation is wired. The dataset supplies a reproducible westerly and later
synthetic gust; it does not supply a calibrated spread model or legal airspace.

## Agent D

Build web against OpenAPI and schema.json. Start the Rust API without keys to
use the seeded synthetic scenario. GET /healthz returns the live run ID;
GET /api/scenarios returns visible scenario snapshots, including the demo ID.
Use run_id on incident/weather/SSE queries. Render the mode, epoch, cursor,
source time, capture time, uncertainty, evidence lineage and source URLs.

On run.reset, discard old layers and GET the provided snapshot_url. Resume SSE
using Last-Event-ID. Numeric IDs are global, so gaps between events of one run
are normal. Reconnection is scoped and a retained history is bounded.
Weather/update and incident/upsert payloads contain full visible arrays.

Never treat an empty live incident list as proof that no fires exist: first
check source validation, enabled state, health and coverage. A paused scenario
does not move until play. Seek accepts {"at":"UTC timestamp"}; play accepts
an optional {"speed":60} body; pause needs no body. Display explicit 501 states
for Agent C capabilities until integrated.

## Ownership and remaining platform work

File storage and PostgreSQL storage currently assume one application writer.
Cloud provisioning, Timescale aggregates, exact Portugal border masking,
custom CSV mapping/polygon upload, a complete forecast grid and source credit
metadata integration remain follow-up work. Do not report them as shipped.
All synthetic fixture records are authored for this repository; no Fogos.pt
data snapshot is redistributed with the scaffold.
