# Local setup

The web dashboard can run from bundled captures. The Rust service adds the
interactive API workspace, source adapters and simulation endpoints. ROS/PX4 is
an optional, separate simulator setup described in [the ROS README](../ros2/README.md).

## Requirements

| Component | Requirement |
|---|---|
| Backend | Rust 1.100.0, including rustfmt and Clippy |
| Frontend | Node.js 22+ and npm; CI uses Node.js 24 |
| Contract checks | Python 3 with [requirements-dev.txt](../requirements-dev.txt) |
| Database | Optional PostgreSQL/PostGIS; local Compose configuration is included |
| Maps | Internet access and a browser with WebGL |

Use the checked-in Cargo and npm lockfiles. If rustup is installed but Cargo is
not on your shell path, run `export PATH="$HOME/.cargo/bin:$PATH"` for that shell.

## Offline source mode

From the repository root:

```sh
FIRECAST_AUTO=false FIRECAST_NETWORK=false cargo run --locked -p emberwatch-server
```

From `web/` in another terminal:

```sh
npm ci
NEXT_PUBLIC_API_BASE_URL=http://127.0.0.1:8080/platform/ui npm run dev
```

Open [the workspace](http://127.0.0.1:3000/workspace) for the backend-connected
synthetic demo. Open `/` for the dashboard and historical replay. Setting the
workspace API URL does not replace the dashboard's bundled historical files.

On PowerShell, set variables before starting the corresponding process:

```powershell
$env:FIRECAST_AUTO = 'false'
$env:FIRECAST_NETWORK = 'false'
cargo run --locked -p emberwatch-server
# In a second terminal, from web/:
$env:NEXT_PUBLIC_API_BASE_URL = 'http://127.0.0.1:8080/platform/ui'
npm run dev
```

## Environment variables

The Rust process does not automatically read `.env`. Use
[.env.example](../.env.example) as a reference and export the values you need.
`NEXT_PUBLIC_*` values are embedded in the browser build; set them before
`npm run dev` or `npm run build` and keep credentials out of them.

| Variable | Purpose |
|---|---|
| `BIND_ADDRESS` | Server bind address, default `127.0.0.1:8080` |
| `WEB_ORIGIN` | Browser origin allowed by CORS; match the URL used in your browser |
| `DATABASE_URL` | Enable PostgreSQL persistence and startup migrations |
| `STATE_PATH` | Platform JSON state, default `.state/platform.json` without PostgreSQL |
| `ENABLE_LIVE_SOURCES` | Native platform live-source gate, default `false` |
| `OPERATOR_TOKEN` | Bearer token for protected native source runs and imports |
| `ALLOW_LOCAL_MUTATIONS` | Optional operator bypass, permitted only on loopback |
| `FIRMS_MAP_KEY` | Optional NASA FIRMS credential |
| `CONTEXT_DEV_API_KEY` | Server-side Context.dev REST credential |
| `FIRECAST_AUTO` | Automatic forecast startup, default enabled |
| `FIRECAST_NETWORK` | Forecast network acquisition, default enabled |
| `FIRECAST_WEATHER` | Set to `synthetic` for explicitly synthetic weather |
| `FIRECAST_CONTEXT_MAX_CREDITS` | Forecast Context.dev credit cap per incident |
| `NEXT_PUBLIC_API_BASE_URL` | Browser workspace API base, normally ending in `/platform/ui` |
| `NEXT_PUBLIC_MAP_STYLE_URL` | Optional licensed MapLibre style override |

`ENABLE_LIVE_SOURCES` and `FIRECAST_NETWORK` control different paths. Disable
forecast acquisition explicitly when running the training demo offline.

## Service interfaces

| Base | Contract |
|---|---|
| `/api` | Original synthetic snapshot, replay, dispatch and SSE routes; ROS uses this Snapshot contract |
| `/platform/api` | Native platform runs, source registry, imports and event history |
| `/platform/ui/api` | Browser adapter for the native platform and illustrative response models |
| `/platform/contracts` | Native OpenAPI and generated JSON Schema |

See [the platform API reference](AGENT_A.md) and
[shared contracts](../contracts/README.md) for route details. The dashboard's
`/api/live/status` request is separate from these workspace routes; when it is
unavailable, the dashboard uses its saved status capture.

## PostgreSQL/PostGIS

From the repository root:

```sh
docker compose -f infra/compose.yaml up -d db
export DATABASE_URL=postgres://emberwatch:local-development-only@localhost:5432/emberwatch
FIRECAST_AUTO=false FIRECAST_NETWORK=false cargo run --locked -p emberwatch-server
```

Compose credentials are for local development. The server applies both checked-in
migrations when `DATABASE_URL` is set. Connection or migration errors stop startup.

Without PostgreSQL, the native platform saves datasets, source settings and replay
history to `STATE_PATH`; its file store assumes one server process. The original
`/api` scenario stays in memory. Response outputs are cached in memory and need
recomputing after a server restart. Seek/reset invalidates the relevant outputs.

## Context.dev

The application calls Context.dev REST from Rust using `CONTEXT_DEV_API_KEY`.
Codex's OAuth/MCP connection is separate and does not provide that environment
variable to the running application. No Fogos.pt API key or direct API client is
used. Fogos.pt content is read only through public-page captures via Context.dev.

For Windows, the included helper supports masked local key entry and encrypted
storage under the Git-ignored `.state/` directory:

```powershell
.\infra\context.ps1 -Action set-key
.\infra\context.ps1 -Action status
.\infra\context.ps1 -Action usage
.\infra\context.ps1 -Action probe -Page table
.\infra\context.ps1 -Action serve
```

The probe makes a billable capture. `CONTEXT_PROBE_CAPTURE=1` retains public-page
diagnostics locally; `-UseBuilt` reuses an executable only if it is current with
the source. On other systems, export the key in the server's environment and use
the [intelligence crate's probe](../crates/intelligence/README.md).

Keep recurring Fogos.pt ingestion disabled until the checks in
[DATA_SOURCES.md](DATA_SOURCES.md) are resolved. A successful capture does not
establish complete coverage or redistribution permission.

## Validation with PostgreSQL

Use a disposable database. These tests write test state:

```sh
export TEST_DATABASE_URL=postgres://emberwatch:local-development-only@localhost:5432/emberwatch
cargo test -p emberwatch-platform postgres_round_trip -- --ignored
cargo test -p emberwatch-platform --test postgres --locked -- --ignored
```

With the server running, `node tests/smoke.mjs` exercises the original synthetic
API. `python3 scripts/smoke_http.py http://127.0.0.1:8080/platform` exercises native
platform persistence and needs fresh disposable state, or `--restored` after a
restart. Frontend browser checks and their requirements are in
[web/README.md](../web/README.md).
