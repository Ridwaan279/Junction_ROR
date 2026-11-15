# EmberWatch — Junction 2026 build plan (v3, Portugal + Rust)

Updated **14 November 2026**. Working name: **EmberWatch**. Main track:
Context.dev, "The Web Is Your Real-Time Database".

Decisions: **mainland Portugal**, **Context.dev scraping Fogos.pt public pages**,
and a **Rust backend**. This replaces v2's geography, language, sources, replay,
backend, and agent assignments. The original sections below describe the intended
build; section 17 records the initial implementation and remaining limitations.

Read [AGENTS.md](../AGENTS.md) for collaboration rules and
[DATA_SOURCES.md](DATA_SOURCES.md) for dated source evidence and open questions.

## 1. Decisions in one screen

| Question | Decision |
|---|---|
| Product | A Portugal wildfire information map with cited public reports, satellite observations, weather, historical replay, and simulated spread/response. |
| Geography | Mainland Portugal, bbox `[-9.6, 36.9, -6.1, 42.2]` in west/south/east/north order, plus a mainland boundary filter. Islands are stretch scope. |
| Language/time | Portuguese extraction/place names, Portuguese/English briefings and labels. Store UTC; display `Europe/Lisbon`, including daylight saving. |
| Primary web source | Fogos.pt public rendered pages through Context.dev. No Fogos.pt API integration or API fallback. Extraction, coverage and website reuse checks remain open. |
| Other evidence | Portuguese news and ANEPC/municipal notices via Context.dev, independent NASA FIRMS detections, and uploaded scenario datasets. |
| Weather | IPMA documented station observations and municipal fire-risk API; Open-Meteo forecast/elevation/archive. |
| Backend | Rust workspace: Axum, Tokio, Serde, reqwest, SQLx, PostgreSQL/PostGIS; optional TimescaleDB. |
| Frontend | Next.js/TypeScript + MapLibre GL + deck.gl. MapTiler satellite/outdoor/terrain after account, quota and attribution checks. |
| Simulation | Rust spread grid, wind blending, A*, and seeded drone simulation on bounded compute workers. Experimental results, never operational guidance. |
| Replay | Same domain pipeline with virtual time. Primary candidate: Aveiro / Sever do Vouga, 15–18 September 2024, EMSR760; freeze a subwindow after checking data coverage. |
| AI/voice | Gemini for Portuguese extraction, citations, summaries and explanations; optional ElevenLabs PT/EN voice. Geometry comes from code. |
| Transport/hosting | Axum REST + SSE + webhook endpoint; Docker locally, Next.js hosting and a long-running Rust VM/container with PostgreSQL. Hosting is not provisioned yet. |
| Team | Four workstreams: A platform/integrator, B intelligence/sources, C forecast/response, D terrain UI. Coordinator implements A, with three workers when implementation begins. |

## 2. Product value and priorities

Story: **"Portugal's public fire reports become a live, cited map; weather and
satellite evidence add context; replay makes the response simulation inspectable."**
Do not guarantee first detection, complete national coverage, or confirmation
of every report by satellite.

The [Copernicus event report](https://mapping.emergency.copernicus.eu/news/wildfire-in-portugal/)
documents the September 2024 fires around Oliveira de Azeméis, Sever do Vouga,
and Albergaria-a-Velha. This is a grounded Portuguese replay candidate, not a
verified minute-by-minute chronology. Intended evaluators/users include civic
analysts, journalists, volunteer organizations and civil-protection researchers.

**Minimum complete slice:** Portugal map, one cited incident, source freshness,
a playable scenario, and a Context.dev extraction in its timeline.
**Full pitch:** weather, illustrative spread, simulated drone reroute and briefing.
**Stretch:** webcams, arbitrary source wizard, extra AI models, islands and 3D.
No stretch feature starts before the full pitch works.

Retain the original judging themes of useful live web data, impact, demo
completeness, execution and measured freshness. Current scoring, prizes,
sponsor credits and submission deadline need confirmation from the event brief.
Enter Context.dev/Gemini first; Tiger Data/ElevenLabs/CRACI and other tracks only
when actually integrated and eligible. v2 prize amounts and model IDs are not
verified requirements.

## 3. Tech stack and repository

| Layer | Choice / responsibility |
|---|---|
| Service | Axum REST, SSE and webhook routing; Tokio scheduling/cancellation/bounded channels; tracing and HTTP middleware. |
| HTTP/serialization | reqwest deadlines/backoff; Serde boundary types. Context.dev, Gemini, ElevenLabs and data-provider keys remain server side. |
| Storage | SQLx + PostgreSQL/PostGIS migrations; JSONB evidence; Timescale hypertables if the host supports them, ordinary indexed tables otherwise. |
| Compute | Rust geospatial operations, metre-based grids, A*, seeded random spread. Use a bounded worker pool or `spawn_blocking` for CPU work. |
| Browser | Next.js/React/TypeScript, MapLibre terrain, deck.gl overlays, GeoJSON, native SSE client. |
| Maps | MapTiler styles/DEM if entitled, configured licensed raster fallback otherwise; retain provider attribution. |
| Build | Pinned Rust toolchain, Cargo workspace/lockfile, separate web manifest/lockfile, Docker Compose and CI. |
| Hosting | Long-running Rust process for ingestion/SSE, PostgreSQL/PostGIS and Next.js host. A VM/container is proposed, not already deployed. |

Rust owns ingestion, replay, fusion, forecast and drone planning. TypeScript owns
the browser. No Python service is required. Call Context.dev REST from reqwest;
do not assume an official Rust SDK exists. Pin compatible crate/toolchain and
frontend versions during scaffold; select actual model IDs from the account.

Planned layout:

    Cargo.toml / Cargo.lock / rust-toolchain.toml
    crates/domain/        Shared Serde types, Source, Clock, inputs/outputs
    crates/platform/      Scheduler, replay, fusion, persistence, FIRMS/Open-Meteo/uploads
    crates/intelligence/  Context.dev, Fogos pages, IPMA, news, extraction/summary
    crates/response/      Spread, wind blend, planner, simulation, briefing
    crates/server/        Axum entry point, route wiring, SSE and composition
    contracts/            OpenAPI, JSON Schemas, shared fixtures
    migrations/           PostgreSQL/PostGIS schema
    web/                  Next.js UI
    infra/                Compose/deployment
    docs/                 Plan and source research

Domain imports no application crate. Platform/intelligence/response depend on
domain; server composes them. UI consumes JSON contracts and fixtures. B/C must
not import platform internals or create another backend service.

## 4. Source registry and Context.dev scraping

A source declares ID, kind/roles, fetch method, cadence, upstream lineage,
enabled state, validation status, health, replay support and coverage.
Health distinguishes `healthy`, `empty_verified`, `partial`, `stale`, `blocked`,
`failed`, `unconfigured`, and `needs_validation`.

Define an object-safe Rust Source trait returning a typed SourceBatch from a
RunContext containing clock, window, bbox and run ID. Use a boxed future or
adapter for heterogeneous sources. A batch contains candidates, evidence,
weather, provenance and coverage metadata. Every normalized record carries:

- Source ID, upstream origin group, visible external ID/link where available.
- Event/observed time, publication time if known, capture time, replay availability
  time and the basis of that time. Unknown times remain null.
- Geometry if known, location text, uncertainty radius, extraction quality flags.
- Raw source type/status, interpreted values, URL and evidence hash.
- Live/scenario run scope. Capture time is never silently used as ignition time.

| Source | Method | Proposed cadence / replay | Owner |
|---|---|---|---|
| Fogos.pt public list/detail | Context.dev rendered scrape + schema | 5 min after checks, budget permitting; replay only from permitted snapshots | B |
| Portuguese news/local reports | Context.dev search + article scrape | 5–10 min, bounded by incident/query; test historical coverage | B |
| ANEPC/municipal notices | Context.dev public-page scrape/monitor | 10 min or permitted monitor schedule; archived notices where available | B |
| NASA FIRMS VIIRS/MODIS | Documented API, free MAP_KEY | 10 min live; available dated products for replay | A |
| IPMA station observations | Documented public weather API | Hourly data, shared national cache | B |
| IPMA municipal fire risk | Documented API | Hourly fetch cap, daily validity dates | B |
| Open-Meteo | Documented forecast/elevation/archive APIs | Hourly forecast, cached terrain and replay weather | A |
| Copernicus EMSR760 products | Verified download/import | Once per product; enforce publication time | B discovers/provides provenance; A imports |
| CSV/GeoJSON/JSONL | Bounded upload parser | Once per upload, replayable | A |
| Webcams/vision | Context.dev screenshot + Gemini | Phase 2, curated permitted pages only | B |

### 4.1 Fogos.pt public-page extraction

    Rust scheduler -> Context.dev POST /v1/web/scrape
                   -> rendered https://fogos.pt/pt public content
                   -> nullable JSON schema + evidence validation
                   -> SourceBatch -> persistence/fusion -> SSE -> map

Use bearer `CONTEXT_DEV_API_KEY`, never a Fogos API key. Start with one bounded
rendered-page probe; inspect Markdown/JSON against the visible page and optional
screenshot. Discover actual detail links. No populated incident rows appeared
in the research fetch, so successful extraction remains to be demonstrated.

Extract only visible ID/link, locality, municipality, raw Portuguese type/status,
source timestamps and resource counts when present. Coordinates absent from
the page require a separately labelled geocode/radius. Never invent precision.
Validate per-output success and partial/truncated/paginated coverage. Loading
shells are failures/partial results, never "0 fires in Portugal". Disappearing
rows do not automatically mean resolution. Keep last good data marked stale.

Set `maxAgeMs: 0` on scheduled fresh captures; share stored results across clients.
Use one poller or one monitor per page, not both. Bound detail visits/actions and
back off on failures. Do not target Fogos API URLs through Context.dev, replay
network requests, or bypass access restrictions.

Website viewing was available without a fee in the research fetch. Website
scraping/republication rights and crawler policy remain unresolved in
DATA_SOURCES. API terms do not establish website scraping permission. Resolve
those conditions before recurring ingestion/public redistribution; all agents
can build against synthetic or independently licensed fixtures meanwhile.

### 4.2 Cost and freshness

The [current Context.dev pricing page](https://www.context.dev/pricing) lists
1,000 free monthly credits, 2 concurrent requests and 10 requests/minute. Standard
scrape starts at 1 credit; successful JSON extraction adds 4. One page every five
minutes over 44 hours is 528 requests, about 2,640 credits with JSON, before
pagination, actions, retries, monitors and other sources. Free-tier-only testing
is possible; that full structured schedule needs more credits or lower cadence.

Confirm actual dashboard balance/event credits. If available, target a maximum
10,000-credit event budget: 3,500 Fogos, 3,000 news/notices, 1,000 monitors/probes,
2,500 reserve. Enforce the lesser of that target and the confirmed allocation.
The previous 50,000-credit `JUNCTION26` claim is unverified until redeemed. Use
on-demand captures/slower cadence and replay when the budget is smaller.

Record event/publication time, capture time, provider cache age, extraction
latency and map delivery separately. Polling frequency is not source freshness.
Budget real returned usage, including partial-success requests.

## 5. Weather, provenance and confidence

Use the [IPMA documentation](https://api.ipma.pt/) endpoints catalogued in
DATA_SOURCES. Observations update hourly; `-99.0` is missing. Select nearby fresh
stations, normalize wind units/direction, and retain absent gusts as null. Use
labelled Open-Meteo forecast gusts when measurements are unavailable. Do not
carry v2's ten-minute station claim into the Portugal UI.

[NASA FIRMS area queries](https://firms.modaps.eosdis.nasa.gov/api/area/) use the
Portugal bbox and actual product/date availability. Preserve satellite,
acquisition time, quality, FRP and capture time. Thermal detections and cluster
envelopes are not measured fire perimeters.

Match stable source identity first, then compatible time/location/uncertainty;
preserve ambiguous nearby incidents. Fogos ANEPC-derived reports and articles
repeating them share an origin group. FIRMS imagery displayed on Fogos may also
share our satellite lineage; do not count these as independent confirmations.

Evidence labels:

- `reported`: one geocoded public report, with its source status.
- `corroborated`: two reviewed independent origin groups agree in time/location.
- `satellite_supported`: a matching independent satellite observation supports it.

Official-source provenance, evidence level and active/contained/resolved status
are separate fields. Disagreements stay visible. A numeric confidence score is
a heuristic, not a probability or automatic official confirmation.

## 6. Data model and API contracts

A publishes executable migrations/contracts at hour 1. This table describes
planned entities; it is not production SQL.

| Entity | Fields / invariant |
|---|---|
| runs/scenarios | UUID, live/historical/synthetic mode, bbox/window, speed, seed, replay epoch. |
| sources/source_runs | Nonsecret config, policy/validation, cadence, lineage, health/coverage, last success, cache/capture metadata, credits. |
| evidence | Run, source/external identity, origin group, event/publication/availability/capture times, URL, point/radius, raw/normalized attributes. Scoped idempotent upsert. |
| detections | Run, satellite/product/pixel identity, acquisition time, point, quality, FRP. Scenario identity participates in dedupe. |
| incidents/evidence_links | Run-scoped identity, operational status, evidence level, centroid/radius, observed envelope, linked evidence, severity. |
| weather | Run, station/grid ID, observed/forecast flag, valid/capture times, units and nullable fields. |
| forecasts | Run/incident/epoch, input evidence/weather IDs, model version, seed, generated time, simulated horizon and geometry. |
| drones/plans/telemetry | Run, depot/role, state/position/battery, route/ETA, avoided hazards and simulated time. |
| alerts/briefings | Run/incident, reason, PT/EN text, optional audio, simulated inbox delivery. |
| webhook receipts/snapshots | Idempotency key, raw-body hash, verification, extracted batch and policy-aware retention. |

Distances use metres and an appropriate projected grid or PostGIS geography;
GeoJSON is `[longitude, latitude]`. Timescale hypertable unique keys must include
time; ordinary indexed tables are the portable fallback. Delay aggregates until
incident-to-evidence links are stable.

Initial API surface:

    GET  /healthz
    GET  /api/incidents?run_id=&bbox=&since=
    GET  /api/incidents/{id}                 /api/incidents/{id}/timeline
    GET  /api/incidents/{id}/forecast
    POST /api/incidents/{id}/enrich          /api/incidents/{id}/dispatch
    GET  /api/sources
    POST /api/sources/{id}/test              /api/sources/{id}/run
    PATCH /api/sources/{id}
    GET  /api/scenarios
    POST /api/scenarios                     /api/scenarios/{id}/load
    POST /api/scenarios/{id}/upload          /api/scenarios/{id}/play
    POST /api/scenarios/{id}/pause           /api/scenarios/{id}/seek
    GET  /api/wind-field?run_id=&bbox=&at=    /api/stations?run_id=&near=
    GET  /api/drones?run_id=                 /api/drones/{id}/plan
    POST /api/drones/dispatch                /api/drones/{id}/replan
    GET  /api/events?run_id=
    POST /webhooks/context

All operations are run-scoped, including ID routes. Public viewers can read and
operate a simulated scenario; source config, upstream runs, uploads and monitor
management require an operator credential or local dev mode. Viewer clicks do
not trigger unbounded paid calls. Fixed allowlisted sources are MVP; arbitrary
URLs are stretch with host/address restrictions and upload limits.

SSE envelope: `{id, type, run_id, scenario_id, epoch, event_time, emitted_at,
payload}`. Events: `source.run`, `evidence.new`, `incident.upsert`, `weather.update`,
`forecast.ready`, `drone.plan`, `drone.replan`, `drone.update`, `briefing.ready`,
`scenario.tick`, `run.reset`. Bound buffers; provide heartbeat/reconnect/resync.
Seek increments the replay epoch and invalidates old derived outputs.

## 7. Shared live/replay pipeline

1. Scheduler supplies RunContext. Live sources capture current pages/APIs;
   historical sources read prepared data, never present-day incident state.
2. B validates extraction and emits typed batches. A deduplicates/stores/fuses;
   geocoding uses Portuguese locality/municipality/parish context and uncertainty.
3. Enrich with available weather, risk and exposure; keep URLs and input IDs.
4. C computes simulation geometry/plans off the async executor. AI explains
   supplied evidence and computed outputs with citations.
5. A persists scoped outputs and emits SSE; D renders them. Alerts go only to
   the simulated inbox, never to real emergency services.

A seeded Clock governs every module. Play/pause/speed/seek must be deterministic.
Release inputs by recorded availability, not just the event they describe.
Current Fogos pages cannot reconstruct 2024 state. Historical news search today
also does not establish what was visible at a precise minute; label unknown
delays/reconstructed order explicitly.

Scenario candidate: **Aveiro / Sever do Vouga, 15–18 September 2024, EMSR760**.
The [official EMS bulletin](https://mapping.emergency.copernicus.eu/news/information-bulletin-174-copernicus-emergency-management-service-monitors-wildfires-in-portugal/)
identifies the activation/products. Verify FIRMS archive coverage, weather,
article timestamps, product downloads/terms, then freeze a 12–24 hour subwindow.
Later-published perimeter products must not appear early in replay.

Uploads: bounded CSV/GeoJSON/JSONL with mapping preview, CRS/timezone checks,
provenance/redistribution status and time range. Do not commit captured Fogos
records as openly licensed fixtures without verifying those rights. Maintain
fully offline synthetic Portuguese fixtures, clearly labelled, while licensed
historical assets are assembled.

## 8. Rust spread, drones and AI

**Spread:** documented cellular grid driven by fuel, wind, slope and humidity.
Verify the published model's equations, units, constants and time-step
calibration; v2's remembered coefficients are not defaults. Seed ensemble runs.
Test downwind behavior, nonburnable cells, reproducibility and parameter bounds.
Use actual hourly observation freshness, otherwise labelled forecast input.
Return GeoJSON uncertainty/arrival contours. Until calibrated, describe them as
illustrative simulation rather than an accurate three-hour forecast. Keep
measured perimeter and simulated spread in separate layers.

**Planner:** Rust A* over a bounded 60 km box/500 m grid, with wind-relative travel
time, climb, configured obstacles, smoke and gust hazards. These are demo
resolutions, not terrain clearance guarantees. Airport/OSM buffers are simulation
obstacles, not a complete legal airspace model. Check outbound task plus return
and battery reserve; reject unreachable missions. Replan with hysteresis, except
new hard hazards invalidate immediately. Explain each route change using input
diffs and measure runtime on demo hardware. No assumed sub-100 ms guarantee.

**Simulation:** idle -> enroute -> on_task -> return -> charging, advanced by the
run clock. Scout/suppressor are simulated capabilities. No vehicle control,
aviation authorization or real dispatch is in scope.

**AI:** Gemini extracts nullable Portuguese facts, geocoding hints, cited summaries,
contradictions, exposure ranking and route explanations. It does not create
precise coordinates, measured perimeters or authoritative status from nothing.
Generate PT/EN briefings; optional ElevenLabs audio falls back to text. Server
config holds model/voice IDs; cache by input hash. Gemma/Nemotron and webcam
vision remain stretch after the complete baseline.

## 9. Terrain UI and four-minute demo

MapLibre satellite/outdoor/terrain, deck.gl evidence pins, uncertainty circles,
satellite points, observed envelopes, illustrative spread, weather, configured
obstacles and simulated drone paths/icons. Measure frame rate; use flat-map
fallback when needed. Keep map/provider attribution, including a visible linked
`Source: Fogos.pt` and `via Context.dev` for that source.

Panels: Live/Replay/Synthetic badge and clock; event ticker; cited incident
status/evidence/timeline; Sources with coverage/health/data time/capture time;
scenario play/pause/seek/upload preview; fleet and simulated inbox. Never
hardcode today's fire count or danger. Failure produces unknown/stale, not an
all-clear. SSE reconnect/reset must remove obsolete scenario layers.

Demo:

1. Open Portugal and actual live source health; use a labelled replay if live
   extraction is unavailable/unvalidated.
2. Show a permitted Context.dev Fogos public-page capture, URL, extracted visible
   fields, capture time and uncertainty. Live capture is budget/validation gated.
3. Play the verified Aveiro subwindow or explicitly synthetic fallback. Reports
   and independent evidence arrive in recorded order, not a fabricated claim
   that the web always beats satellites.
4. Add weather/spread and dispatch simulated drones around an obstacle. If there
   is no historical measured gust, inject a clearly synthetic gust to demonstrate
   rerouting and its explanation.
5. Show PT/EN briefing and a monitor run on an approved page or clearly labelled
   team-controlled page. Show provenance, credit use and offline behavior.
6. Return to live Portugal without implying replay events are current.

## 10. Timeline and integration gates

Relative hours until the real deadline is confirmed. Agents work in parallel
from the hour-1 baseline; each gate requires one integrated demo.

| Hours | A: platform | B: intelligence | C: response | D: UI | Gate |
|---|---|---|---|---|---|
| 0–1 | Workspace/domain/contracts/fixtures, DB fallback | Fogos rules/rendering; Context budget; IPMA | Typed inputs/outputs; synthetic case | Fixture Portugal map | All acknowledge contracts; source uncertainty logged |
| 1–8 | Replay/persistence/SSE, FIRMS/Open-Meteo/upload skeleton | Fogos extractor/coverage report, IPMA | Seeded grid/A* baseline | Cited incident/freshness/replay | End-to-end Portugal fixture replay; source probe report |
| 8–16 | Live ingestion/identity/lineage; archive manifest | Portuguese news, notice monitor/webhook | Wind, obstacles, battery, spread | API/SSE, source panel, timeline | Validated web evidence integrated; offline replay works |
| 16–26 | Deploy/integration/reset/reconnect/alerts | Edge cases, cited summary, usage limits | Reroute/simulation, PT/EN text/optional voice | Full forecast/drone/inbox flow | Full pitch on shared URL or reproducible local deployment |
| 26–34 | Reliability/CI/extension checks | Optional approved source polish | Model bounds/runtime | Map performance/camera/locale | Feature freeze |
| 34–40 | Release/docs/fallback | Verify citations/source notes | Verify simulation/limitations | Rehearsal/video | Recorded demo and offline backup |
| 40+ | Fixes/buffer only | Buffer | Buffer | Buffer | Submit before verified deadline |

If Fogos checks are unresolved at hour 1, B continues a bounded probe while
A/C/D work on independent/synthetic fixtures. If still unusable at hour 8, show
it as unavailable and use permitted Portuguese notices/news through the same
registry. Never silently switch to Fogos API or relabel another source as Fogos.

## 11. Contracts and collaboration

A publishes domain types, Source/Clock/RunContext, OpenAPI, JSON Schemas and
fixtures by hour 1. Fixtures cover a Portuguese reported incident, duplicated
upstream reports, satellite support, missing/stale weather, empty-vs-failed page,
forecast, reroute and replay reset. B/C/D acknowledge and develop independently.

B returns evidence/weather batches and webhook verification results; A owns
scheduling, persistence, fusion, route mounting and SSE. C returns computed
outputs; A stores/exposes them. D consumes serialized contracts, not Rust
internals. Shared-contract changes include affected consumers and fixture updates.

Use four worktrees/branches when possible; coordinator implements A and three
workers cover B/C/D, fitting four active slots. Never switch a shared checkout
under another agent. One owner edits root Cargo/lockfile/contracts. Integrate
at hours 8/16/26/34, earlier for small compatible handoffs, and sync workers to
the new baseline. This documentation task does not launch implementation agents.

## 12. Agent assignments and done criteria

| Agent / branch | Exclusive ownership | Deliverables and acceptance |
|---|---|---|
| **A: platform/integrator**, `codex/core` | Root config/lockfile, domain/platform/server crates, contracts, migrations, infra, integration tests, AGENTS/PLAN | Axum/source scheduler, SQLx/PostGIS, FIRMS/Open-Meteo/uploads, identity/lineage fusion, deterministic replay, SSE and deploy. Done: play/pause/seek/reset/reconnect work; restart/upsert does not duplicate reports; clean run instructions and integration results. |
| **B: intelligence**, `codex/intelligence` | Intelligence crate, its tests/local fixtures, DATA_SOURCES | Rust Context client, Fogos public-page extraction/report, IPMA, Portuguese news/geocoding hints/cited summary, signed/deduplicated webhook module, usage metadata. Done: inspected capture where permitted/possible; tested partial/empty/blocked/missing fields and IPMA null/direction/time; copied upstream lineage preserved; unresolved source checks explicit. |
| **C: forecast/response**, `codex/response` | Response crate, its tests/local fixtures | Rust grid/wind/A*/return-reserve checks, simulation/replan, severity proposal, grounded explanation and PT/EN text/optional voice. Done: seeded reproducibility, west wind pushes east, obstacles never crossed, unreachable mission rejected, hard hazard reroutes, measured runtime and text fallback. |
| **D: terrain UI**, `codex/ui` | Web app, web manifest/lockfile/tests | Portugal map, cited timeline, source health/coverage, mode/clock controls, SSE/resync, forecast/drone/inbox, visible attribution and PT/EN. Done: fixture click-through by hour 8–10, integrated API by 26, failure cannot display all-clear, replay clears old layers, usable measured map performance. |

Workers request root dependency/contract changes from A; do not edit shared
lockfiles concurrently. B/C do not create database schemas or extra servers. A
owns cross-crate tests; each worker owns tests in their directory. D does not
reimplement fusion/spread in the browser.

Handoff: branch/commit, owned paths, observable behavior, validation commands and
results, environment variable names, contract requests, known limitations and
next dependency. No secret values. A verifies integration gates and owns the
shared deployment. Exact rules and paths are in AGENTS.md.

## 13. Risk register

| Risk | Mitigation / owner |
|---|---|
| Fogos JS shell, pagination or blocking | B verifies rendered coverage, bounded public UI extraction only; partial/stale state and independent fixtures. |
| Website rights/crawler rules unclear | B records exact evidence; do not claim confirmed open reuse. Resolve before public redistribution; other modules continue. |
| Insufficient Context credits | A enforces confirmed budget; B bounds queries/pages, lower cadence and replay-first demo. |
| No live incidents | Actual source state plus replay; never assume November means zero fires. |
| Copied evidence inflates confidence | A/B preserve upstream lineage; D exposes evidence and uncertainty. |
| Missing weather/gusts | Null/stale flags and labelled forecast input; synthetic gust labelled in demo. |
| Historical gaps/hindsight leakage | Coverage manifest, availability-time gating, labelled synthetic chronology. |
| Database extension mismatch | A verifies PostGIS/Timescale early; indexed PostgreSQL fallback for time series. |
| CPU work stalls SSE | C uses bounded compute workers; A handles queues/cancellation. |
| Terrain quota/FPS | D uses licensed flat/raster fallback and measures on target hardware. |
| AI/voice failure | Validation, cited text, bounded retries/cache; voice optional. |
| Monitor silent during pitch | Approved manual run or labelled controlled page, never a fake public alert. |
| Agent edit collisions | Ownership, isolated worktrees, one shared-contract/lockfile owner and timed gates. |

## 14. First 60 minutes

1. A reads all docs, checks workspace, scaffolds compatible Rust/web contracts and
   fixtures, verifies PostgreSQL/PostGIS and records the real deadline.
2. B checks Context key/balance/concurrency, Fogos public-page rules/robots and one
   bounded rendered extraction. Record coverage/time/null fields/reuse status;
   no Fogos API calls. A/C/D proceed on synthetic fixtures regardless.
3. B verifies IPMA payload/attribution. A checks FIRMS archive availability and
   Open-Meteo coverage; B discovers EMSR760 products/news. Build a manifest before
   claiming historical completeness.
4. C creates a seeded Portuguese weather/hazard case; D consumes the same fixtures.
5. Freeze interfaces/ownership/gates. Configure server-only Context/Gemini/FIRMS
   keys, optional ElevenLabs/map credentials and database URL by variable name.
   No Fogos API key is part of this architecture.

## 15. Validation and submission

For documentation-only work: links, stack/region consistency, ownership and
whitespace checks. Do not claim application tests before code exists.

Once implemented:

- `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo test --workspace`; web manifest lint/typecheck/build; fresh DB migrations.
- End-to-end: extraction -> stored evidence -> cited incident -> SSE -> map;
  replay/reset/reconnect; offline/failure/partial page; duplicate report/webhook;
  drone reroute; text briefing without optional voice credentials.
- Measure source-to-screen components, compute runtime and map frame rate.
- Submit runnable repo, deployment/local demo instructions, video, citations and
  data/source attribution, limitations, mode labels and offline backup.
- Include sponsor tracks only for real eligible integrations; verify event rules
  and resources separately. No emergency contact, account application, email,
  publishing or deployment is performed by this documentation revision.

## 16. Research status

All existing local project documentation (`docs/PLAN.md`) was read. Root
AGENTS.md was absent and is now added; DATA_SOURCES records primary-source
research and links. Architecture and work ownership are settled. Extractability,
website reuse, crawler policy, account credits, archive coverage, exact dependency
versions, hosting extensions and current event rules remain explicit checks.

Public viewing, extraction feasibility, reuse rights and Context.dev cost are
separate questions. The plan follows the requested Rust + public-page scraping
route without claiming an unrestricted free feed, and retains an independent
replay path that every agent can build against.

## 17. Initial parallel implementation checkpoint

The user requested implementation by B/C/D in parallel. The coordinator publishes
shared Rust DTOs, Source/Clock contracts, generated JSON Schemas, OpenAPI and
synthetic Portuguese fixtures. Agent branches are `agent-b`, `agent-c`, and
`agent-d`, each in its own managed worktree; the integrator uses `codex/core`.

The first platform slice implements run-scoped in-memory evidence ingestion,
stable-source identity, conservative lineage handling, deterministic availability
gating, play/pause/seek/reset, atomic snapshots, bounded SSE and optional SQLx
PostgreSQL/PostGIS persistence. The source registry starts with Fogos ingestion
disabled pending reuse checks. The synthetic fixture is invented training data,
not a reconstruction of the September 2024 event. No live report or historical
completeness is claimed.

The SQL migration and local Compose recipe are present; database integration
verification requires a running Docker daemon/PostgreSQL instance. The local
Docker daemon was unavailable during the initial checks. Without DATABASE_URL,
the server explicitly runs in memory. FIRMS/Open-Meteo adapters, uploads,
reviewed cross-source geographic fusion, full historical asset manifests,
production authentication/deployment and operational calibration remain planned.
See each crate/web README for current behavior and validation details.

B now provides a Rust Context REST client, conservative Fogos extraction,
IPMA weather/risk normalization, Portuguese report handling and signed webhook
verification. Immutable observation versions preserve revised source captures.
On 15 November, Codex Context MCP OAuth/tool discovery and a separate encrypted
application key were verified. A fresh REST table scrape matched the browser's
`Sem registo de incêndios` label and correctly retained unknown coverage; the
current compiled Rust client independently reproduced it with partial health.
Populated incident rows and national completeness remain unverified; recurring
ingestion stays disabled. Formatting/lint and 61 Rust tests passed (one optional
DB test skipped). The ordinary probe build hit Windows Application Control
(4551); the live Rust check used the current test-built executable. See
DATA_SOURCES for request metadata and the root README for secure setup/probes.
C provides tested illustrative spread, wind blending, reserve-aware A* routing,
replanning, seeded state simulation and cited PT/EN text. D provides the bilingual
Next.js/MapLibre/deck.gl workspace, source panels, local replay and API/SSE sync.
The integrator mounts synthetic dispatch through a bounded blocking compute pool
and returns forecast/route/briefing snapshots. Webhook HTTP is receipt-only and
requires a live run, PostgreSQL and an independently validated monitor binding;
the bundled synthetic run rejects live deliveries.

Run `node tests/smoke.mjs` against the local Rust service for the HTTP/SSE response,
seek/reset and playback check. DB round-trip verification is an explicit ignored
test requiring TEST_DATABASE_URL; CI provisions PostGIS and runs that check.

## 18. Agent A merge checkpoint

`origin/agent-A` is merged into `codex/core`. Its independently developed DTOs
are retained in `domain::platform`; the existing browser DTOs remain compatible
with B/C/D. One Axum process mounts the native platform at `/platform`, and a
`/platform/ui` adapter supplies canonical snapshots, scenario controls, SSE and
incident-scoped C simulation outputs to the existing browser.

Agent A supplies file/PostGIS persistence, bounded replay event history,
FIRMS/Open-Meteo adapters, shared cadence gates, operator authorization and bounded
CSV/JSONL/point-GeoJSON uploads. Live adapters remain disabled for merge testing.
The SQL migrations use separate `platform_*` tables, avoiding TEXT/UUID collisions
with the original compatibility API. Dataset/replay state survives a file-store
restart; adapter response outputs are cached in memory and recomputed after restart.

Merge regression tests cover stable incident IDs through revisions, returning to
a prior status, scoped recovery URLs, per-incident response retention, unit
conversion and backward seek invalidation. Original and incoming Rust tests,
contract validation, both HTTP smoke flows and browser checks are the merge gate.
Local PostGIS verification remains blocked by the unavailable Docker daemon.
