# EmberWatch

Rust platform for a Portugal wildfire information and simulation map.
Agent A's initial implementation is on **agent-A**. Read [AGENTS.md](AGENTS.md)
before contributing, and [the product plan](docs/PLAN.md) for the larger target.

## Run without credentials

Install Rust 1.100.0 with rustfmt and Clippy. The repository pins that toolchain;
Cargo.lock pins the resolved dependencies. From the repository root:

```sh
cargo run --locked -p emberwatch-server
```

The server listens on http://127.0.0.1:8000. With no DATABASE_URL, it stores state
in .state/platform.json. Startup seeds a paused, explicitly **synthetic** Sever
do Vouga scenario. Live upstream adapters are disabled on a fresh installation.
Restart restores evidence and pauses scenarios rather than silently advancing
them. The single-process file store uses atomic replacement, not multiwriter
coordination.

In another PowerShell terminal:

```powershell
$health = Invoke-RestMethod http://127.0.0.1:8000/healthz
$scenarios = Invoke-RestMethod http://127.0.0.1:8000/api/scenarios
$runId = $scenarios[0].id
Invoke-RestMethod "http://127.0.0.1:8000/api/incidents?run_id=$runId"
Invoke-RestMethod -Method Post "http://127.0.0.1:8000/api/scenarios/$runId/play" -ContentType application/json -Body '{"speed":60}'
curl.exe -N "http://127.0.0.1:8000/api/events?run_id=$runId"
```

If cargo is missing from PATH on Windows, open a new terminal after installation
or invoke the executable in your user profile's .cargo/bin directory.

The synthetic report appears at 12:00, a mirror of the same upstream report at
12:10, independent synthetic satellite support at 12:20, and a synthetic gust at
12:25. The mirror does not increase independent corroboration. These are invented
demonstration events, not records of a real fire or a Fogos.pt export.

## PostgreSQL/PostGIS

Docker Desktop must be running. Start the development database:

```sh
docker compose -f infra/compose.yaml up -d db
```

Then run the API against it:

```powershell
$env:DATABASE_URL = 'postgres://emberwatch:emberwatch-local@localhost:55432/emberwatch'
cargo run --locked -p emberwatch-server
```

Migrations run at startup. The development database listens only on loopback
port 55432. Its checked-in credentials are for local development; configure
separate credentials for hosting. PostgreSQL persistence uses a transactional
snapshot plus normalized evidence and incident tables with PostGIS geography
and spatial indexes. Ordinary tables work without TimescaleDB.

To build/run the containerized API and database:

```sh
docker compose -f infra/compose.yaml up --build -d
```

The Docker API is also published only on loopback port 8000. No cloud deployment
or external account is provisioned by this branch.

## HTTP and agent contracts

- [OpenAPI 3.1](contracts/openapi.yaml), also at /contracts/openapi.yaml.
- [Generated JSON Schema](contracts/schema.json), also at /contracts/schema.json.
- [Shared Rust types and Source/Clock interfaces](crates/domain/src/lib.rs).
- [Synthetic dataset](contracts/fixtures/portugal-synthetic.json).
- [Source health examples](contracts/fixtures/source-health.json) and
  [illustrative response payloads](contracts/fixtures/response-synthetic.json).
- [Integration guide](contracts/README.md) for Agents B, C and D.

Read APIs include incidents, incident timeline, sources, scenarios, run snapshots,
weather/station samples and scoped SSE. A missing run_id defaults to the live run
returned by /healthz. Scenario play/pause/seek are public simulation controls;
seek pauses, increments epoch and emits run.reset.

SSE sends numeric event IDs and supports Last-Event-ID within a persisted bounded
512-event history. Missing, expired or future cursors produce run.reset with a
snapshot URL. Fetch the snapshot on reset, discard older-epoch output and
reconnect with the last received ID. Streams are run-scoped and heartbeat every
15 seconds. Timeline currently returns the latest available revision of each
linked source record; full revisions remain in durable storage.

Source configuration/runs and scenario creation/loading/uploads require
Authorization: Bearer OPERATOR_TOKEN. Set the token in the server environment.
ALLOW_LOCAL_MUTATIONS=true is a development alternative and is rejected with a
non-loopback server bind address. The server does not implicitly load .env;
export variables into your shell or configure your host. See [.env.example](.env.example).

Source polling uses one shared cadence gate for manual and scheduled runs,
preserves partial/failure/empty distinctions, and never clears incidents merely
because a source fails or returns an empty page. Fogos.pt/IPMA remain explicit
Agent B placeholders. No Fogos.pt API client or key exists in this platform.

FIRMS and Open-Meteo HTTP adapters are implemented but live requests require
operator enablement; ENABLE_LIVE_SOURCES=true enables configured adapters at
startup. FIRMS also needs FIRMS_MAP_KEY. Use the source PATCH endpoint to disable
a persisted source setting; settings otherwise survive restart. The Open-Meteo
adapter currently fetches a single bbox-centre weather sample, and exposes a
batched elevation method. It is not yet a full interpolated wind grid.

Uploads are JSON requests containing format, source_id and data. Supported:
FIRMS-column CSV, Evidence-shaped JSONL, or GeoJSON Point FeatureCollections with
Evidence-shaped properties. Limits: 2 MiB HTTP body, 10,000 imported rows.
Custom CSV column mapping, polygon import and multipart preview remain follow-up
work. CSV capture time is its availability default; historical reconstruction
must supply explicit availability times through JSONL/dataset loading rather
than invent publication timestamps.

Forecast, dispatch and drone endpoints return 501 agent_c_pending until Agent C
is integrated. Agent D's browser UI is not included. The current geographic
filter is the mainland Portugal bounding box; a precise national-border mask
remains follow-up work. Source-specific archive coverage and Fogos page
extraction/reuse checks are recorded in [DATA_SOURCES.md](docs/DATA_SOURCES.md).

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The PostGIS integration test is opt-in so the default suite works offline:

```powershell
$env:TEST_DATABASE_URL = 'postgres://emberwatch:emberwatch-local@localhost:55432/emberwatch'
cargo test -p emberwatch-platform --test postgres --locked -- --ignored
```

Use a disposable database. The test creates its own run and verifies migrations,
evidence deduplication, longitude/latitude ordering and restart restoration.
CI runs this database check as well as formatting, Clippy, regular tests and
generated-schema drift checks.

Optional OpenAPI/schema/fixture validation uses development tools only:

```sh
python -m pip install -r requirements-dev.txt
python scripts/check_contracts.py
```

For Linux container verification, build the toolchain stage and run the same
checks there. The database network below comes from the Compose command above:

```sh
docker build --target build -f infra/Dockerfile -t emberwatch-agent-a-build .
docker run --rm --network infra_default -e TEST_DATABASE_URL=postgres://emberwatch:emberwatch-local@db:5432/emberwatch emberwatch-agent-a-build cargo test --workspace --release --locked -- --include-ignored
```

The HTTP smoke helper accepts a running API URL:
`python scripts/smoke_http.py http://127.0.0.1:8000`.
It is intended for fresh disposable state, advances the synthetic scenario and
starts playback. After restarting that same state, run it with `--restored`.

Regenerate schema after shared type changes:

```sh
cargo run --quiet --locked -p emberwatch-domain --example export_schema > contracts/schema.json
```

On Windows PowerShell 5, save the generated output as UTF-8 without a BOM.
Semantic validation of coordinates, windows and source identities also runs in
Rust at ingestion; JSON Schema alone does not express every domain invariant.
