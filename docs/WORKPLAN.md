# EmberWatch two-agent work plan

Status: plan only. Nothing here is implemented until a gate below says so.
Read with [AGENTS.md](../AGENTS.md), [PLAN.md](PLAN.md), [DATA_SOURCES.md](DATA_SOURCES.md)
and, for Agent 2, [CONTEXT_DEV_ACCURACY.md](CONTEXT_DEV_ACCURACY.md).

## 1. What we are building next

The goal for the demo: an operator opens one screen, sees Portugal in 3D, a fire
is reported, and **without anyone clicking** the system has already verified the
report against cited sources, forecast spread and smoke, sized a drone fleet,
launched simulated drones from fixed stations along smoke-avoiding routes, and is
producing drone situation reports. The operator can ask a Gemini advisor
"what is the risk of this reaching Piódão in the next 6 hours?" and get an
answer whose numbers come from our models, with citations.

Priority order (from the request; do not reorder):

| # | Work | Owner |
|---|---|---|
| 1 | UI/UX overhaul: one operations screen, clear hierarchy, automation story | Agent 1 |
| 2 | Merge `codex/portugal-earth-buildings` (3D terrain + buildings) once its final push lands | Agent 1 |
| 3 | Context.dev accuracy: verified extraction, provenance, golden tests | Agent 2 |
| 4 | Drone stations, deterministic fleet sizing, daily/frame re-sizing | Agent 2 (model), Agent 1 (map) |
| 5 | Station→fire 3D routes, A* on a cost map that avoids smoke and predicted fire | Agent 2 (planner), Agent 1 (render) |
| 6 | People-at-risk / nearby events from Context.dev, feeding deployment priority | Agent 2 |
| 7 | Gemini: fleet-count cross-check, drone situation reports, risk chatbot | Agent 2 (server), Agent 1 (UI) |
| 8 | Accuracy and correctness checks across all of the above | Both, in their own scope |
| 9 | ROS2 bridge | Later phase. Only the message contract is written now (section 9) |

Out of scope for now: ElevenLabs/voice, real dispatch, any contact with real
emergency services. Drones, stations and dispatch are simulations and are
labelled so everywhere.

## 2. Agents, branches and file ownership

Two agents work in parallel. Merging stays easy because **no file has two
writers**. Every change outside your scope goes through a request to the owner.

| | Agent 1: Experience + integration | Agent 2: Intelligence + planning + Gemini |
|---|---|---|
| Session | This session | The user's other agent |
| Branch | integration branch (this session's branch) | its own branch, cut from the Gate 0 commit |
| Writes | `web/**` except `web/public/generated/**`; `docs/UI.md`; `docs/WORKPLAN.md` | `crates/intelligence/**`, `crates/firecast/**`, new `crates/advisor/**`, `crates/server/**`, root `Cargo.toml`/`Cargo.lock`, `data/**`, `web/public/generated/**`, `docs/CONTEXT_DEV_ACCURACY.md`, `docs/DATA_SOURCES.md` |
| Never writes | any Rust, `Cargo.*`, `data/**`, `web/public/generated/**` | `web/**` outside `web/public/generated/**`, `web/package*.json` |
| Git integration | Performs all merges into the integration branch | Hands off a commit; never merges into the integration branch |

Mapping to the AGENTS.md table: Agent 1 = D (UI) plus the coordinator's Git
integration duty; Agent 2 = B (intelligence) + C (response/firecast) and the
server routes it needs. `contracts/**`, `crates/domain/**`, `crates/platform/**`
and `migrations/**` are frozen for this phase; if Agent 2 needs a change there it
writes the proposal in its handoff and Agent 1 applies it at a gate.

Rules that keep merges trivial:

1. The JSON contracts in section 7 are frozen at Gate 0. Agent 1 builds against
   the real demo fixture in `web/public/fixtures/v2/` (DEMO_DATA.md); Agent 2 makes the real
   generator write the same shapes to `web/public/generated/`. Swapping fixture
   for generated output is a one-line URL change.
2. A contract change is a request: proposed shape, consumers, fixture diff. The
   other agent acknowledges before either side commits it. Additive optional
   fields are allowed without a request but must be listed in the handoff.
3. Before every handoff run the scope check and include the output:

   ```bash
   git diff --name-only <gate-commit>..HEAD
   ```

   Every path must be inside your "Writes" column.
4. Agent 2 adds server routes in its own modules (`crates/server/src/advisor_api.rs`,
   `crates/server/src/fleet_api.rs`) and mounts them in `crates/server/src/lib.rs`;
   Agent 1 does not touch the server, so no conflict.
5. Rebase onto the integration branch after every gate. Do not rewrite another
   agent's history, revert its files, or reformat files you do not own.
6. Handoff note: commit hash, files, behaviour delivered, commands run with
   results, open issues, required env var **names** (never values).

## 3. Gates

| Gate | Done when | Who |
|---|---|---|
| 0 | Done: plan, contracts and firecast work pushed to `claude/fervent-clarke-m9qsko` (`9a061da`) | Agent 1 |
| 1 | 3D branch merged into the integration branch, all checks green, Agent 2 rebased | Agent 1 |
| 2 | UI shell redesign on fixtures; Agent 2 delivers stations + deterministic fleet + 3D routes for the Arganil replay into `web/public/generated/` | both |
| 3 | Context.dev accuracy harness and golden set passing thresholds; exposures (people at risk) in the replay output | Agent 2 |
| 4 | Gemini advisor endpoints (fleet check, sitreps, chat) with cached replay outputs; UI panels wired | both |
| 5 | End-to-end demo pass, accuracy checklist (section 8) green, screenshots | both |

## 4. Agent 1 tasks: UI/UX first, then the 3D merge

### 4.1 Merge the 3D branch (Gate 1) — done

Merged `origin/codex/portugal-earth-buildings` at `7d1363a` into
`claude/fervent-clarke-m9qsko` with `--no-ff`. The four conflicts were resolved as
below; all Rust and web checks passed after the merge.

| File | Resolution |
|---|---|
| `.gitignore` | Union of both sides |
| `README.md` | Keep both sections: firecast/historical replay and 3D terrain/buildings |
| `web/components/MapView.tsx` | Adopt their basemap: `loadMapLibre`, `loadTerrainStyle`, `addTerrainLayers` from `@/lib/terrain` replace my exported inline OSM/Terrarium `style`. Keep all firecast deck.gl layers, horizon controls and tooltips. Keep "source tile errors do not fail the map" |
| `web/components/Workspace.tsx` | Keep both header actions: the history-replay switch and their `/terrain` link |

`HistoricalView.tsx` now uses the same terrain loader, so both maps share one
basemap; a basemap failure there no longer breaks the replay panels.

### 4.2 UI/UX overhaul (highest priority)

**User decisions (15 Nov 2026), these override anything below that conflicts:**

1. **Not too much information.** The screen must stay calm and readable. Show
   the few things that matter now; details open on demand (hover, click, expand),
   never all at once.
2. **3D map from the start.** The map opens in 3D (terrain, tilt) with no button
   press needed. A **2D view** is also available as a simple toggle, and the
   fire/smoke/drone layers work the same in both.
3. **Ask before designing.** Whichever agent builds the UI must first ask the user
   questions about the UI/UX (who looks at it, what must be visible at first
   glance, style, demo flow) and wait for the answers before committing to a
   layout. The layout below is a starting proposal, not a decision.

Answers from the user (first round):

| Question | Answer | What it means for the build |
|---|---|---|
| Main viewer | Hackathon judges | Clear story in a few minutes; big visuals, little reading |
| Visible on first open, no click | Fire + predicted spread | The 3D map opens on the fire with burn-probability bands and the 6/12 h envelope; everything else (drones, timeline, villages, sources) is one click or hover away |
| Visual style | Realistic satellite imagery with a dark ops-room UI | Satellite basemap on 3D terrain (a free, attributable imagery source with terms checked, e.g. Sentinel-2 cloudless); dark panels and controls; fire, smoke and drones glow on top |
| Demo flow | Runs on real-time data; a button switches to the previous simulation | Default view is **live** (current Portugal data through the server). A clear button switches to the **historical replay** (Arganil/Piódão 2025, real data), and back. Live never shows replay data and vice versa; the mode badge is always visible |

Live view with no active fire (user decision): the screen clearly says there is
no active fire right now, e.g. "Sem incêndios ativos neste momento / No active
fires right now", with the time of the last successful check and the sources
checked. That message is shown only after a successful capture that positively
shows no active incidents; if a capture fails or is partial, the screen says the
check failed instead. The replay button stays visible next to the message.

Starting proposal: one operations screen replaces the current workspace/historical split. Layout:

```
┌ top bar: EmberWatch · mode badge LIVE / HISTORICAL REPLAY / SYNTHETIC · Lisbon clock · source freshness · PT/EN ┐
├ left rail ──────────┬ 3D map (terrain, buildings, fire, smoke, stations, drone routes) ┬ right panel ────────┤
│ incidents           │                                                                   │ Risk summary        │
│ automation timeline │                                                                   │ Fleet (count + why) │
│  (detected → …)     │                                                                   │ Drone reports       │
│                     │  bottom: replay timeline scrubber + layer legend                  │ Ask the advisor     │
└─────────────────────┴───────────────────────────────────────────────────────────────────┴─────────────────────┘
```

Requirements:

- **Automation timeline** in the left rail, one row per step with timestamp,
  latency from detection and source link: Reported → Verified (n independent
  sources) → Forecast ready → Fleet sized (N) → Drones launched → First report.
  Each row appears as its event arrives. This is the "fire detected and the drone
  is already going" story; it must be readable in five seconds.
- **Map layers** with a compact legend and toggles: observed perimeter (solid),
  predicted burn-probability bands per horizon (sequential, colour-blind safe),
  smoke surface concentration (separate hue), wind arrows, stations (icons with
  "simulated" label), drone routes as 3D paths at flight altitude above terrain,
  drone positions animated along routes by replay time, no-fly/smoke-blocked
  areas, exposure sites (people at risk) with citation popovers.
- **Right panel cards**: risk summary (area, head bearing/speed, fire type,
  towns within the 6-h envelope), fleet card (count with the breakdown terms
  from section 6.2, change since previous frame and the reason), drone reports
  (latest first, "simulated observation" badge), advisor chat.
- **Provenance always visible**: every number that comes from a source shows its
  source and capture time on hover; derived values say "model" and link to the
  method. Scrape failure is shown as a failure state, never as "0 incidents".
- Design tokens (colour, type scale, spacing) in `globals.css` custom properties;
  dark operations theme by default with a light theme; WCAG AA contrast; keyboard
  access to timeline, layers and chat; PT/EN for every string; times displayed in
  `Europe/Lisbon`, stored UTC.
- Performance: split the 2.6 MB replay file into an index plus per-frame files
  once Agent 2 emits them (section 7.1); lazy-load frames; keep deck.gl layer
  instances stable across frames.
- Verification: production build, Playwright screenshots at
  `deviceScaleFactor: 2` for the main states (idle, fire reported, fleet
  launched, chat answer, scrape failure) and phone width.

### 4.3 Rendering the new contracts

- Stations: `IconLayer`; colour by availability.
- Routes: `PathLayer` with `[lon, lat, alt_m]` coordinates from section 7.2,
  dashed segment for `ground_transfer` legs, solid for flight legs.
- Drone animation: interpolate along `route.waypoints` by `t_s`.
- Smoke-avoidance explanation on hover: "detour +2.1 km to avoid smoke ≥ 150 µg/m³".
- Chat UI: streaming not required; show answer, the tool facts used (section
  7.4 `facts`), citations and the model name returned by the server.

## 5. Agent 2 tasks (backend)

Agent 2's first task is the Context.dev accuracy work in
[CONTEXT_DEV_ACCURACY.md](CONTEXT_DEV_ACCURACY.md). Then:

### 5.0 What EmberWatch monitors around a fire

Every monitor runs on a schedule per active incident, records request ID, credits
and status, and feeds evidence with provenance. Real data only: demo and replay
data must come from sources that were public at the time (see
[DEMO_DATA.md](DEMO_DATA.md)); nothing is invented, and a capture that fails is a
failure, not "nothing found".

| Monitor | How | What it extracts |
|---|---|---|
| Official incident page | Context.dev scrape of the public Fogos.pt incident page (never the API, never `/gaia`) | Status changes with times, official coordinates, nature; current resource counts only while live |
| News | Context.dev `web/search` + `web/scrape` with a schema; `news/search` only where its index covers the outlet (it does not cover Portuguese outlets, see DEMO_DATA.md) | Cause, spread direction, evacuations, shelters, threatened villages, resources over time |
| Events and gatherings | Context.dev search + scrape of municipal agendas, parish and tourism pages, regional press | Festas, feiras, romarias, markets, campsites, summer camps: name, place, dates, stated attendance |
| Anything else happening in the area | Context.dev search + scrape: civil protection, municipalities, GNR, roads, utilities | Road closures, evacuation and shelter notices, power/telecom outages, school/care-home moves, tourist presence |
| Warnings and fire danger | IPMA API (current warnings, fire-risk index) plus dated reports of IPMA notices for history | Heat/wind warnings, fire danger level per municipality |
| Weather | Open-Meteo forecast (live) or ERA5 archive (history), 9 points around the fire | Temperature, humidity, 10 m and 100–120 m wind speed and direction, gusts, cloud, radiation, rain, VPD; dead-fuel moisture derived from these |
| Satellite | NASA FIRMS (needs `FIRMS_MAP_KEY`) | Hotspots for confirmation and growth |
| Terrain and fuel | Terrarium DEM, OSM land cover (Overpass) | Slope, aspect, fuel class |

Context.dev also cross-checks the numeric feeds: a report saying "ventos fortes"
or "rajadas de 40 km/h" is compared with the weather feed for the same hour and
flagged when they disagree.

### 5.0b Demo data handover

`data/demo/arganil-early/` and `web/public/fixtures/v2/arganil-early/` were created
by Agent 1 before Agent 2 started (requested by the user). From now on Agent 2 owns
`data/demo/**` and should (1) port the capture scripts to the Rust
`probe_context` example, (2) produce 15–30 min frames for the first 3 hours,
(3) fill `fleet`, `sitreps` and `fleet_check`, and write the result to
`web/public/generated/arganil-early/` in the same format. Agent 1 keeps
`web/public/fixtures/**`.

### 5.1 Drone stations (simulated)

- `data/stations/portugal.json`: a small set of **hypothetical** stations near
  district seats in the fire-prone interior (for example Viseu, Coimbra, Castelo
  Branco, Guarda, Vila Real, Bragança, Leiria, Santarém, Portalegre, Faro).
  Coordinates must come from a cited source (OSM place nodes), not memory.
  Every station carries `simulated: true` and the source of its coordinates.
- Airframe parameters (cruise, endurance, reserve, payload) are configuration
  with the label "demonstration values"; no real product specs are claimed.

### 5.2 Deterministic fleet sizing (the number of drones)

The fleet count is computed in Rust, from a formula we can defend and test; it
is recomputed every replay frame and whenever a new daily perimeter or new
evidence arrives. Gemini cross-checks and explains it (5.5); it never silently
changes it.

```
airborne_perimeter = ceil( P_front / (v · T_revisit / 2) )
  P_front    perimeter of the predicted 1-h front (m); each drone patrols a segment back and forth
  v          cruise ground speed (m/s), T_revisit target revisit interval (s)
airborne_head      = number of distinct head-fire sectors (rate of spread above threshold), ≥ 1
airborne_exposure  = number of exposure clusters (5.4) with P(burn within 6 h) ≥ 0.2, capped
duty               = (E − 2·t_transit) / (E + R)
  E endurance, t_transit station→task time from the A* route, R recharge/swap time
fleet              = ceil( (airborne_perimeter + airborne_head + airborne_exposure) / duty · (1 + reserve) )
```

Output the breakdown and every input with its unit and source (observed
perimeter, forecast, configured value). Replace the current heuristic
`fleet_size` in `crates/firecast/src/historical.rs`. Tests: monotonic in
perimeter, revisit and transit time; duty ≤ 0 means "station out of range"
rather than a negative fleet; reproduces hand-computed cases.

### 5.3 Station→fire routes on a smoke-aware cost map

- Regional grid (≈500 m cells) covering the selected stations and the fire,
  larger than the 19 km forecast domain. Smoke beyond the forecast domain comes
  from evaluating the existing Gaussian-plume model at the cell centre.
- Costs: hard block where P(burn within mission horizon) ≥ 0.3 or smoke ≥
  150 µg/m³ (configurable), penalty below that, climb cost from the DEM, wind
  ground-speed pricing as today in `emberwatch-response::plan_mission`.
- Altitude: terrain + fixed AGL (configurable, default 120 m) along the path,
  emitted as `[lon, lat, alt_m]`.
- Choose stations by feasible mission time; if no fixed station is within
  range, emit a `ground_transfer` leg to a forward launch point and say so.
- Invariants as tests: no route vertex inside a blocked cell; altitude ≥ terrain
  + AGL − tolerance; route time ≤ endurance − reserve; a detour is reported when
  the straight line crosses blocked smoke.

### 5.4 People at risk and nearby events (Context.dev)

- Context.dev search/scrape for events and gatherings near the fire's
  municipalities in the forecast window (festas, romarias, feiras, campsites,
  summer camps), plus official evacuation/shelter notices. Fixed sites (schools,
  care homes, hospitals, campsites) come from OSM Overpass.
- Extract with a JSON schema: name, kind, place text, start/end, expected people
  (null if not stated), source URL, quote. Geocode place text against OSM place
  names in the affected municipalities; unresolved places are kept, unmapped.
- Priority per site = people (or a kind default, labelled) × P(fire within 6 h
  at the site). Sites above threshold add exposure drones (5.2) and are task
  points for routes.
- Accuracy rules from CONTEXT_DEV_ACCURACY.md apply: quote verification,
  lineage, never invent attendance.

### 5.5 Gemini advisor (`crates/advisor`)

Server-side only: `GEMINI_API_KEY`, `GEMINI_MODEL` (required, no default model
ID in the repo: the team picks one from Google's current model list). REST
`generateContent` with structured JSON output (`responseSchema`), function
calling and, for the fleet check, the code-execution tool. Network: the
environment must allow `generativelanguage.googleapis.com`.

1. **Fleet cross-check**: send the fleet inputs and the formula; Gemini computes
   the count with code execution and explains it in PT/EN. Rust compares: equal →
   "verified by advisor"; different → show the deterministic number and flag the
   disagreement. Gemini may add a recommendation (for example "add one drone for
   the campsite"), shown as a suggestion the operator can accept.
2. **Drone situation reports**: at each task point, sample the facts the drone
   would see in the simulation (distance to observed or predicted edge, smoke
   concentration, wind, nearby exposure sites, flank or head). Gemini turns only
   those facts into a structured report. Every report is labelled "simulated
   observation" and keeps the facts beside the text.
3. **Risk chatbot**: function calling over our data, so numbers are not
   generated by the model: `find_place(name)`, `burn_probability(lon, lat,
   hours)`, `smoke(lon, lat, hour)`, `exposures()`, `fleet()`, `sources()`.
   Post-check: every number in the answer must appear in a tool result
   (rounded), otherwise the answer is rejected and retried once, then returned
   with `grounded: false`.
4. **Replay determinism**: for the Arganil replay, cache advisor outputs into
   `web/public/generated/` with `model`, `generated_at` and inputs hash, so the
   demo runs without a key and a seek never shows a report from the future.

Endpoints (Agent 2 owns `crates/server`):

| Route | Returns |
|---|---|
| `GET /api/stations` | 7.2 `stations` |
| `POST /api/incidents/{id}/fleet?run_id=&at=` | 7.2 `FleetPlan` |
| `POST /api/incidents/{id}/advisor/fleet-check` | 7.3 `FleetCheck` |
| `POST /api/incidents/{id}/advisor/sitrep` | 7.3 `Sitrep` |
| `POST /api/incidents/{id}/advisor/chat` | 7.4 `ChatAnswer` |
| `GET /api/incidents/{id}/exposures` | 7.2 `ExposureSite[]` |

## 6. How Gemini and the physics fit together

- Physics (Rothermel/ensemble, Gaussian plume, A*) produces every number.
- Context.dev produces evidence, each item with URL, quote and capture time.
- Gemini explains, cross-checks arithmetic, writes reports from given facts and
  answers questions by calling our functions. It does not forecast fire.
- The UI always shows which of the three a value came from.

## 7. Shared contracts (frozen at Gate 0)

All times are RFC 3339 UTC. Coordinates are `[lon, lat]` or `[lon, lat, alt_m]`.
Unknown values are `null`. Every object that is derived carries `basis`
(`"observed" | "model" | "configured" | "advisor"`), and every object built from
sources carries `sources: Source[]`.

```ts
type Source = { url: string; title: string | null; captured_at: string; event_time: string | null;
  quote: string | null; lineage: string; quality: "verified" | "unverified" | "failed" };
```

### 7.1 Replay files

`generated/<dataset>/index.json`: `{ dataset, mode: "historical", frames: [{ at, file }], stations_file, sources: Source[] }`.
Each frame file keeps the existing frame fields and adds `fleet: FleetPlan`,
`exposures: ExposureSite[]`, `sitreps: Sitrep[]`, `fleet_check: FleetCheck | null`,
`automation: AutomationEvent[]`.

```ts
type AutomationEvent = { step: "reported" | "verified" | "forecast" | "fleet_sized" | "launched" | "first_report";
  at: string; latency_s: number; detail_pt: string; detail_en: string; sources: Source[] };
```

### 7.2 Stations, fleet, routes, exposures

```ts
type Station = { id: string; name: string; position: [number, number]; simulated: true;
  drones_total: number; drones_available: number; coordinate_source: string };
type FleetPlan = { at: string; count: number; previous_count: number | null; change_reason_pt: string | null;
  change_reason_en: string | null; breakdown: { term: string; value: number; unit: string; basis: string; source: string | null }[];
  drones: Drone[]; held: { station_id: string; reason: string }[] };
type Drone = { id: string; station_id: string; role: "perimeter" | "head" | "exposure" | "reserve";
  route: Route | null; task_point: [number, number] };
type Route = { waypoints: { position: [number, number, number]; t_s: number }[];
  legs: { kind: "flight" | "ground_transfer"; from: number; to: number }[];
  duration_s: number; distance_m: number; straight_line_m: number;
  detour_reason: string | null; max_smoke_ugm3: number; reserve_pct_at_return: number };
type ExposureSite = { id: string; name: string;
  kind: ("evacuated" | "threatened" | "shelter" | "people_present" | "event" | "road_closure")[];
  position: [number, number] | null; people: number | null; people_basis: string | null; // "stated: <evidence id>" or "kind_default"
  population_geonames: number | null; distance_from_ignition_km: number;
  events: { name: string | null; scheduled_start_text: string | null; scheduled_start_date: string | null;
    scheduled_start_basis: string | null; status: "cancelled" | null; evidence: string }[];
  evidence: string[]; in_modelled_burn_p50: boolean; in_forecast_6h: boolean; in_forecast_12h: boolean;
  p_fire_6h?: number | null; priority?: number | null };
type Evidence = { id: string; page: string; category: string; extracted_category: string; checks: string[];
  statement_en: string; quote: string; places: { name: string; geonames: string | null; position: [number, number] | null }[];
  time_text: string | null; number: number | null; unit: string | null; quality: "verified" | "unverified";
  number_check: "in_quote" | "not_literal_in_quote" | null; known_at: string; url: string; publisher: string;
  lineage: string | null; mode: "live" | "historical" };
type Monitor = { id: string; category: string; provider: string; target: string;
  status: "ok" | "coverage_gap" | "not_run" | "failed"; items: number; note: string };
```

Revision 2.1 (after Gate 0, before any consumer shipped): `ExposureSite`,
`Evidence`, `Monitor` and the frame layout follow the real demo data in
[DEMO_DATA.md](DEMO_DATA.md), which is the reference fixture for both agents.
`AutomationEvent` gains `basis: "observed" | "simulated"` and `url`. Frames list
`evidence_ids`; the evidence itself lives once in `index.json`.

### 7.3 Advisor outputs

```ts
type FleetCheck = { at: string; deterministic_count: number; advisor_count: number | null; agrees: boolean;
  explanation_pt: string; explanation_en: string; suggestion: string | null; model: string; generated_at: string };
type Sitrep = { drone_id: string; at: string; simulated: true; facts: Record<string, number | string | null>;
  summary_pt: string; summary_en: string; recommended_action: string | null; model: string; generated_at: string };
```

### 7.4 Chat

Request `{ question: string; locale: "pt" | "en"; run_id: string; at: string }`.

```ts
type ChatAnswer = { answer: string; grounded: boolean; facts: { tool: string; args: unknown; result: unknown }[];
  sources: Source[]; model: string; generated_at: string };
```

## 8. Accuracy and verification checklist

Each agent runs the checks for what it changed and reports any it could not run.

- Rust: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- Web: `npm run lint`, `npm run typecheck`, `npm test`, `npm run build` in `web/`.
- Spread validation (existing): Dice and area per Arganil perimeter stay at or
  better than the current values recorded in `crates/firecast/README.md`.
- Fleet formula unit tests and hand-computed cases (5.2); route invariants (5.3).
- Context.dev golden-set thresholds (CONTEXT_DEV_ACCURACY.md section 6).
- Advisor: chat answers pass the number-grounding check; sitrep text mentions
  no fact absent from `facts`; fleet-check disagreement path tested with a stub.
- Replay: seeking to any time shows no evidence, sitrep or exposure with a time
  after the cursor.
- UI: screenshots for the states in 4.2; PT and EN; Lisbon time display tests.

## 9. Later phase: ROS2 bridge (contract only)

The dashboard will send commands to a ROS2 simulation through a rosbridge
WebSocket owned by the server. Nothing is built now. Proposed messages:

- `/emberwatch/fleet/command`: `{ drone_id, action: "goto" | "loiter" | "drop_water" | "return", target: [lon, lat, alt_m] | null, payload_l: number | null, issued_at }`
- `/emberwatch/fleet/state`: `{ drone_id, position: [lon, lat, alt_m], battery_pct, payload_l, status, at }`
- `/emberwatch/fleet/size`: `{ count, reason, at }`

The `FleetPlan` and `Route` contracts above are already the source for these
commands, so the bridge can be added without changing the UI.

## 10. Kickoff prompt for Agent 2

Agent 2 works on **its own branch** cut from `origin/claude/fervent-clarke-m9qsko`
(its cloud session's assigned branch is fine). It never pushes to
`claude/fervent-clarke-m9qsko`; Agent 1 merges Agent 2's branch at each gate.
Paste this into the other agent:

> You are Agent 2 (intelligence, planning and Gemini) on EmberWatch. Fetch
> `origin/claude/fervent-clarke-m9qsko` and create your own branch from it; work
> and push only there. Read AGENTS.md, docs/WORKPLAN.md (sections 2, 5, 7, 8),
> docs/CONTEXT_DEV_ACCURACY.md and docs/DEMO_DATA.md. You write only the paths in
> the "Agent 2" column of WORKPLAN section 2; never edit `web/**` except
> `web/public/generated/**`. Work in this order: (1) CONTEXT_DEV_ACCURACY.md
> tasks 0–5, using the real Arganil captures in `data/demo/arganil-early/` as the
> first golden pages; (2) WORKPLAN 5.0b (port the capture scripts to Rust, finer
> early frames); (3) 5.1 stations, 5.2 fleet sizing, 5.3 smoke-aware 3D routes;
> (4) 5.4 people at risk and nearby events; (5) 5.5 Gemini advisor. The live
> view needs an honest "no active fires" signal (WORKPLAN 4.2): expose, per
> source, whether the last capture succeeded and whether it positively showed no
> active incidents. Use only real data that was public at the time; never invent
> model IDs, quotas, counts, events or timestamps. Context.dev's key is injected
> by the environment proxy (`CONTEXT_DEV_API_KEY` may be a placeholder); Gemini
> needs `GEMINI_API_KEY`, `GEMINI_MODEL` and network access to
> `generativelanguage.googleapis.com`. At each gate, push your branch and hand off:
> commit, `git diff --name-only <base>..HEAD` scope check, `cargo fmt/clippy/test`
> results, open issues and env var names.
