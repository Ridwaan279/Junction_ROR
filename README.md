# EmberWatch

Portugal wildfire evidence map and an explicitly synthetic replay/response demo.
The source plan is [docs/PLAN.md](docs/PLAN.md); source limitations and attribution
are tracked in [docs/DATA_SOURCES.md](docs/DATA_SOURCES.md).

## Local run

Requirements: Rust 1.100.0 (rustfmt/clippy) and Node.js 22+.

```sh
cargo run -p emberwatch-server
cd web
npm ci
npm run dev
```

Open http://localhost:3000. To connect the UI to the Rust service, set
`NEXT_PUBLIC_API_BASE_URL=http://127.0.0.1:8080/platform/ui` before `npm run dev` or
`npm run build`; without it the UI uses an explicitly labelled local fixture.
The merged Agent A scenario is invented training data near Sever do Vouga, not a live incident or
a reconstruction of the 2024 fires. No API key is needed for the offline demo.
All dispatch, drone routes and spread are illustrative simulations.

Agent A's native API is mounted at `/platform` on the same Rust process. The
`/platform/ui` adapter retains the browser contract and integrates C's simulations.
The original `/api` endpoints and their November fixture remain available for
compatibility; configure the browser with the URL above to use Agent A's platform.
See [the merged platform guide](docs/AGENT_A.md) for native routes and operator controls.

If the installed Rust toolchain is named `stable` and reports 1.100.0, `cargo
+stable ...` uses that installation without downloading a second named copy.

## Optional PostgreSQL/PostGIS

```sh
docker compose -f infra/compose.yaml up -d
# Set DATABASE_URL from .env.example in the server process environment.
cargo run -p emberwatch-server
```

The server applies both migrations when DATABASE_URL is set. Without it, Agent A
persists datasets, source settings and replay history to `.state/platform.json`
(configurable with STATE_PATH), while the original `/api` fixture stays in memory.
The adapter caches forecast/route/briefing outputs in memory; recompute after a
server restart. Database connection/migration errors stop
startup rather than silently losing persistence. Local Compose credentials are
development-only. Secrets are never exposed to the browser; `.env` is ignored.

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# With the synthetic server running in another process:
node tests/smoke.mjs
cd web
npm test
npm run lint
npm run typecheck
npm run build
```

Contracts and synthetic fixtures live in [contracts/](contracts/README.md).
Recurring Fogos scraping remains disabled pending source checks. A source
failure never means zero incidents. IPMA nulls and uncertain times remain
unknown. External keys, weather coverage, historical assets and source reuse
permission are not prerequisites for the labelled synthetic replay.

The implementation is an initial integrated slice. Fogos returned partial page
coverage in the bounded Context probe. IPMA timestamps without a confirmed time
basis are skipped. Agent A adds gated FIRMS/Open-Meteo adapters, scheduling,
CSV/JSONL/point-GeoJSON uploads, replay persistence and operator authorization.
Live upstream calls were not enabled for the merge tests. Historical assets,
Gemini/voice and production deployment remain planned. See the intelligence/response/web
READMEs for exact implemented behavior and limitations.
