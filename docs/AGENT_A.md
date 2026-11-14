# Merged Agent A platform

The server hosts both contracts on http://127.0.0.1:8080. The Next.js app keeps
its existing wire format through the Agent A adapter:

```powershell
cargo +stable run --locked -p emberwatch-server
# In another terminal, from web/:
$env:NEXT_PUBLIC_API_BASE_URL = 'http://127.0.0.1:8080/platform/ui'
npm run dev
```

Open http://127.0.0.1:3000. The January 2026 Sever do Vouga scenario is invented
training data. It includes a report, a copied upstream report, independent
synthetic satellite support and a synthetic gust. No real fire chronology is
claimed. Source URLs, availability, capture times, lineage and null gusts survive
the adapter; native km/h weather values become m/s for the browser.

| Interface | Purpose |
|---|---|
| `/platform/ui/api/scenarios`, `/platform/ui/api/snapshot?run_id=...` | Browser snapshots and UUID scenario IDs |
| `/platform/ui/api/scenarios/{id}/{play,pause,seek,reset}` | Browser controls over Agent A replay |
| `/platform/ui/api/incidents/{id}/dispatch?run_id=...` | C's illustrative spread, simulated mission and cited PT/EN briefing |
| `/platform/api/runs/{id}` | Native run snapshot |
| `/platform/api/sources` and `/{id}/run` | Native registry and operator-gated source runs |
| `/platform/api/scenarios` and `/{id}/{load,upload}` | Operator-gated creation/import |
| `/platform/api/events?run_id=...` | Native bounded event history and Last-Event-ID recovery |
| `/platform/contracts/openapi.yaml`, `/platform/contracts/schema.json` | Native OpenAPI and generated schema |
| Original `/api` routes | Preserved November synthetic compatibility slice |

Native domain types are in [platform.rs](../crates/domain/src/platform.rs).
Browser types stay at the domain crate root. The native forecast/dispatch stubs
retain their original 501 response; integrated simulation uses the browser adapter
route above. This keeps the independently published contracts explicit.

Without DATABASE_URL, Agent A uses STATE_PATH (default `.state/platform.json`).
Restart restores evidence, source settings and replay history while pausing clocks.
The file store assumes one server process. With DATABASE_URL, both migrations run;
Agent A uses `platform_state`, `platform_runs`, `platform_evidence` and
`platform_incidents` without replacing the original API's tables.

Forecasts, routes and briefings are cached by run, incident and epoch in memory.
Page reload retains them; server restart requires recomputation. Seek/reset removes
old outputs. Simulations are not operational fire forecasts or real drone dispatch.

Source configuration, manual runs and imports require OPERATOR_TOKEN in the
server environment and `Authorization: Bearer ...`. ALLOW_LOCAL_MUTATIONS=true
is allowed only with a loopback bind. ENABLE_LIVE_SOURCES is false by default;
FIRMS also requires FIRMS_MAP_KEY. These settings remain server-side, and
the server does not automatically load `.env` files. Fogos page reuse and complete
extraction remain unresolved in [DATA_SOURCES.md](DATA_SOURCES.md). B's Context/IPMA
adapters remain separate from Agent A's native source placeholders; no successful
live ingestion is implied by this merge.

Validation commands are in [README.md](../README.md). Additional checks:

```powershell
.\.venv\Scripts\python.exe scripts/check_contracts.py
# Native smoke helper needs fresh disposable state, or --restored after restart:
.\.venv\Scripts\python.exe scripts/smoke_http.py http://127.0.0.1:8080/platform
# From web/, against the running app:
$env:EMBERWATCH_API_TEST_URL = 'http://127.0.0.1:8080/platform/ui'
npm run test:e2e:online
```

The two PostGIS tests require TEST_DATABASE_URL and a running disposable database.
Docker was unavailable locally during the merge; CI provisions PostGIS for them.
