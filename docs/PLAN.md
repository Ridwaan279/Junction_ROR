# EmberWatch — Junction 2026 build plan (v2, Greece)

Working name: **EmberWatch** (rename freely). Main track: Context.dev, "The Web Is Your Real-Time Database".
v2 written Fri 14 Nov 2026. Changes from v1: Greece only; detection from a configurable registry of sources (satellite, web, social, stations, webcams, uploads) instead of FIRMS alone; a Google-Maps-style terrain map; real-time drone path planning driven by live wind and web events; a first-class historical replay engine with uploads; four agents on four branches.

---

## 1. Decisions in one screen

| Question | Decision |
|---|---|
| What it is | A live wildfire operations map for Greece. The web reports a fire within minutes; satellites confirm and locate it within hours; weather stations and a physical model say where it goes; drones are routed around wind, terrain and airspace to the fire and to the people the web says are in danger. |
| Scope | **Greece only** (bbox 19.3, 34.7, 29.7, 41.8). The bbox is a source setting, so flipping to another region is one config change, but the UI, sources and language are Greek-first. |
| Detection | A **source registry**: NASA FIRMS (VIIRS, MODIS), NOA FireHub (Meteosat, 5-minute) if scrapeable, Greek news queries, 112 messages (X account and news mirrors), official pages (Fire Service, Civil Protection), public webcams read by a vision model, Copernicus EMS activations, and uploaded datasets. Every source has a trust weight and can be toggled or added from the UI. |
| Web-first detection | The headline capability. A Greek fire is on 112, X and local news 10 to 20 minutes after ignition; the next VIIRS pass can be hours away. Context.dev finds the report, Gemini extracts the place, the incident appears as a "reported" circle and tightens when satellite points arrive. |
| Weather | Context.dev scrapes the three nearest **meteo.gr stations** (real 10-minute observations: wind, gusts, direction, humidity, temperature) and Open-Meteo supplies the forecast horizon, the wind field grid and the historical weather for replays. Sun position is computed locally. |
| Spread prediction | A published cellular-automaton model driven by station wind, forecast and land cover. Gemini parameterises and explains; it does not draw geometry. |
| Drones | Simulated swarm with **real-time path planning**: A* over a cost grid built from terrain, the live wind field, no-fly zones (airports), the smoke plume, gust limits and web events; re-planned on every new weather sample or web signal, with a Gemini one-line explanation of each reroute. |
| Map | **Terrain, not stylised graphics**: MapLibre GL with MapTiler satellite-hybrid and outdoor styles, 3D terrain from terrain-RGB tiles, hillshade, tilted camera, Greek and Latin labels. deck.gl for hotspots, polygons and drones. Google Photorealistic 3D Tiles only as a stretch "cinematic mode". |
| Replay | A **scenario engine** on the same pipeline: a time window and bbox, loaded from the FIRMS archive, Context.dev historical news, Open-Meteo archive weather and uploaded CSV/GeoJSON; played at 60x through a virtual clock. Headline scenario: Rethymno, Crete, 29 July 2026. |
| Emergency services | Simulated dispatch inbox, ElevenLabs voice briefing (Greek and English), SMS or email to the team's own phones. Never real 112. |
| Gemini role | Greek extraction and geocoding hints, multi-source fusion with citations, model parameterisation, threat ranking, briefing, reroute explanations, vision over webcams and the daily fire-risk map, source-quality audit. |
| Sub-tracks | Enter Context.dev Structured Web, Gemini API, Tiger Data, ElevenLabs, CRACI. Add Nemotron (vision), Gemma (drone commander), Codex, Vultr if hands are free. |
| Stack | Next.js + MapLibre GL + MapTiler + deck.gl · FastAPI · Postgres with TimescaleDB and PostGIS (Tiger Cloud) · SSE · Vercel + a Vultr VM for webhooks. |
| Team | Four agents on four branches: core platform, web intelligence, forecast and response, terrain UI. Core is the integrator. |

---

## 2. What the judges score

Five criteria at 20 % each: real-world impact · creative use of live web data · completeness of demo · technical execution (latency and accuracy) · real-timeness.

- **Impact**: Greece is the sharpest case in Europe. Rethymno (Crete) burned about 4,500 ha in two days from 29 July 2026 with a 15 km front and a state of emergency; Ierapetra (Crete) evacuated 5,000 people on 2 to 3 July 2025 after 112 messages; Patras evacuated a children's hospital on 12 to 13 August 2025. EU-wide, 703,709 ha had burned by 4 Nov 2026 against a 20-year average of 386,483 ha (JRC). Users: regional civil-protection duty officers, volunteer brigades, hotel operators, local journalists.
- **Creative use of live web data**: web-first detection, local-language extraction, station scraping, webcams, monitors, historical web for replay. Every claim carries its URL.
- **Completeness**: deployed URL, no login, cached fallbacks, works offline on fixtures.
- **Technical execution**: show latency per source (report time → on map), confidence tiers, source health.
- **Real-timeness**: ticker with timestamps, per-source "last run" clocks, a monitor webhook landing live during the pitch, drones re-routing when a station reports a gust.

Resources: 50,000 Context.dev credits (code `JUNCTION26`), 50 concurrent requests. Mentors Fri until 23:00, Sat 09:00 to Sun 02:00, Sun 07:00 to end.

---

## 3. Sub-tracks: enter these

| Track | Prize | Fit | Extra work | Verdict |
|---|---|---|---|---|
| Context.dev: Fully Structured Web | €750 / €250 / swag | Same code; note on endpoints | 0 h | **Enter** |
| Gemini API | **€4,000 + $1,000 GCP credits per member** / €2,000 / €1,000 | Gemini is the brain: extraction, fusion, vision, explanations | 0 h if designed in | **Enter, design for it** |
| Tiger Data | Stream Deck Mini | Detections, station samples and drone telemetry are time-series + geo | 2 h | **Enter** |
| ElevenLabs | Earbuds | Bilingual voice briefing | 1 h | **Enter** |
| CRACI | LEGO | `runs-on: craci` | 15 min | **Enter** |
| Nemotron 3.5 Super VL | RTX 5080 ×2 | Vision over webcams, fire-risk map and satellite crops | 4 h | Add in Phase 2 if a person is free |
| Gemma | €1,000 ×3 | Local drone commander (task allocation) via Ollama | 3 h | Add with the drone layer |
| Codex | $10k / $5k / $2.5k credits | Document meaningful Codex use | 0 h if true | Enter if true |
| Vultr | M5Stack | Host the backend VM | 0 h | Enter if hosting there |
| Modal, Aiven, Oxylabs, MongoDB, Verda, Cloudinary, Entire, Solana, Arduino, Atech | — | Overlap, overhead or hardware we do not need | — | Skip |

---

## 4. What is flawed, and the fix for each

1. **Greece in November has no wildfires.** Emptier than Europe. *Fix:* replay is a first-class mode, not a fallback: the Rethymno scenario is loaded at startup and the demo opens on it; live Greece runs beside it and the bbox is a config flag if a live view of the world is wanted on stage.
2. **Satellite-only detection is slow.** VIIRS arrives 2 to 3 hours after overpass. *Fix:* web-first detection from Greek news, 112 and X, geocoded by Gemini, shown as a "reported" circle with an uncertainty radius; satellite points later tighten it. This is the single most convincing use of live web data in the project.
3. **X scraping is unreliable.** x.com blocks most unauthenticated readers and Context.dev may or may not render a profile page. *Fix:* test `x.com/112Greece` and `x.com/pyrosvestiki` through Context.dev scrape in hour 1; if it fails, the same 112 messages reach Greek news sites within minutes, and `news/search` with Greek queries covers them. Never block the pipeline on X.
4. **Scraping weather forecasts is worse than the API.** Judges will ask why. *Fix:* scrape what the API cannot give: real station observations from meteo.gr every 10 minutes (600+ stations). Use Open-Meteo for forecasts, the wind-field grid and archive weather. Say exactly this on stage.
5. **An LLM predicting spread is indefensible.** *Fix:* cellular automaton with published constants; Gemini parameterises and explains.
6. **Drone path planning can over-claim.** Real operations need HCAA coordination and keep clear of aerial firefighting. *Fix:* the planner enforces airport no-fly buffers and an exclusion zone around the active front for non-suppression drones, and the UI says "simulation".
7. **More sources mean more false positives.** *Fix:* a trust model (noisy-OR over independent sources) and a rule: no incident becomes "confirmed" from a single social post or a single satellite pixel.
8. **3D terrain with many layers can drop to 10 fps on a laptop.** *Fix:* deck.gl for all point and polygon layers, capped pitch, lower terrain exaggeration at low zoom, test on the demo laptop by hour 12.
9. **Contacting real emergency services is illegal.** *Fix:* simulated dispatch, labelled.
10. **Four parallel branches diverge.** *Fix:* contracts frozen at hour 1 (schema, API, Source interface, fixtures) and merges at hours 8, 16, 26 and 34 by the core agent.

---

## 5. The product and the demo storyline

**Tagline:** "The web sees the fire first. The satellite proves it. EmberWatch puts both on the terrain, predicts the next three hours and routes the drones."

**Demo (4 minutes), on the Rethymno scenario:**

1. Open tilted over Crete on satellite-hybrid terrain. Header: Live · Greece · 0 active incidents · fire danger low (November) · 11 sources, 9 healthy. Ticker shows today's source runs.
2. Switch to **Scenario: Rethymno, 29 July 2026, 60x**. Camera flies to Agios Vasileios.
3. 14:3x: a Cretan news site reports a fire near Krya Vrysi. Context.dev news search → Gemini extracts the toponym → geocoded → incident appears as a 5 km "reported" circle, tier *reported*, with the article link.
4. 14:4x: 112 message (from the X mirror or news): evacuate toward Plakias. Tier *official*. Severity jumps. Evacuation direction drawn.
5. 15:xx: first satellite points (FIRMS archive) tighten the location; hull drawn; meteo.gr station readings show 7 Bft gusts; isochrones elongate downwind; threat list names the villages and hotels inside +3 h.
6. **Dispatch**: two scouts to the reported evacuation points, four suppressors to the head. The planner routes south of the ridge for tailwind and keeps the exclusion zone; a new gust reading triggers a reroute and Gemini explains it in one line. Briefing voiced in Greek and English.
7. Trigger a Context.dev monitor run live on the Civil Protection announcements page (or a page we control, labelled): webhook lands, ticker updates.
8. Back to Live Greece: sources running against today's web; open the Sources panel, toggle one off, add a webcam URL, watch it run.

Numbers above are placeholders; the real times come from the archive on the day.

---

## 6. Architecture

```
SOURCE REGISTRY (each: kind, fetch, cadence, trust, enabled, replayable)
  satellite  FIRMS api · FireHub (Context.dev scrape, probe)
  web        Greek news queries · official pages · 112 on X (probe) · EMS activations   [Context.dev news/search/scrape/monitors]
  weather    meteo.gr stations (Context.dev scrape+JSON) · Open-Meteo forecast/archive/elevation (api)
  vision     public webcams (Context.dev screenshot → Gemini/Nemotron) · daily fire-risk map (image → Gemini)
  upload     CSV / GeoJSON / JSONL datasets attached to a scenario
          │
          ▼
FastAPI backend ─ Clock (live | scenario) ─ fetch → normalise → fuse → enrich → forecast → score → respond
          │  SSE /api/events · REST · /webhooks/context
          ▼
Postgres + TimescaleDB + PostGIS (Tiger Cloud)      Gemini · ElevenLabs · (Nemotron, Gemma)
          │
          ▼
Next.js + MapLibre + MapTiler terrain + deck.gl: map, ticker, incident timeline, sources panel, scenario controls, drones, dispatch inbox
```

### 6.1 Source registry (the core abstraction)

```python
class Source(Protocol):
    id: str; name: str
    kind: Literal['satellite','news','official','social','station','weather_api','webcam','risk_map','ems','upload']
    role: Literal['detect','confirm','context','weather','hazard']
    fetch: Literal['api','context_scrape','context_news','context_search','context_monitor','upload']
    cadence_s: int; trust: float; enabled: bool; replayable: bool
    async def run(self, window: TimeWindow, bbox: BBox, clock: Clock) -> list[Candidate | Signal]

Candidate = {geom, radius_m, t, quality 0..1, attrs}          # a place that may be burning
Signal    = {place_hints[], geom?, radius_m?, t, url, kind, extracted jsonb, quality 0..1}   # a fact about an incident
```

Seed registry for Greece (edit from the UI):

| Source | Kind / role | Fetch | Cadence | Trust | Replay | Notes |
|---|---|---|---|---|---|---|
| NASA FIRMS VIIRS SNPP/NOAA-20/NOAA-21, MODIS | satellite / detect | api | 10 min | 0.8 (h), 0.5 (n), 0.2 (l) | yes (date param, SP archive) | bbox Greece, day range 1 |
| NOA FireHub (Meteosat, 5 min) | satellite / detect | context_scrape (probe) | 5 min | 0.7 | no | public viewer; if not scrapeable, drop it |
| Greek news queries ("φωτιά τώρα", "πυρκαγιά σε εξέλιξη", "μήνυμα 112 εκκένωση", place-specific) | news / detect+confirm | context_news + context_search, freshness last_24_hours, articleLanguage el | 5 min | 0.5 per outlet, 0.7 ertnews.gr | yes (date filter) | Gemini extracts toponyms, status, evacuations |
| 112 Greece on X (`x.com/112Greece`), Fire Service (`x.com/pyrosvestiki`) | social / confirm | context_scrape (probe) | 2 min | 0.9 (112) | no | fallback: news mirrors of 112 texts |
| Civil Protection announcements, Fire Service incidents page | official / confirm | context_monitor (page) + context_scrape | 10 min, webhook | 0.95 | partial (archived PDFs) | semantic change detection |
| Copernicus EMS activations (Wildfire) | ems / context | api (activations list + GeoRSS) | 30 min | 0.9 | yes | perimeters for replay |
| meteo.gr stations (nearest 3 to each incident) | station / weather | context_scrape + JSON schema | 10 min | — | no (Open-Meteo archive instead) | wind Bft, gust, dir, RH, T |
| Open-Meteo forecast, elevation, archive | weather_api / weather | api | hourly | — | yes | wind field grid, slope, past weather |
| Civil Protection daily fire-risk map | risk_map / hazard | context_scrape (image) → Gemini vision | daily | — | yes (archived PDFs) | categories 1–5 per region; season May–Oct |
| Public webcams (curated list in fire-prone areas) | webcam / confirm | context_scrape screenshot → vision "smoke?" | 10 min | 0.4 | no | Phase 2 |
| Uploads (CSV, GeoJSON, JSONL) | upload / any | upload | once | set by user | yes | FIRMS export, EMS products, custom points |

**Trust model.** Each incident's confidence is a noisy-OR over independent sources: `conf = 1 − Π(1 − trust_i × quality_i)`. Tiers: *reported* (< 0.5, web only), *probable* (0.5 to 0.8), *confirmed* (> 0.8, or any official source). Two signals from the same outlet count once. Satellite alone never exceeds *probable* without two detections or FRP above threshold. Disagreements are shown, not hidden.

### 6.2 Data sources (endpoints, verified)

**NASA FIRMS** (free MAP_KEY; 5,000 transactions / 10 min)
```
GET https://firms.modaps.eosdis.nasa.gov/api/area/csv/{MAP_KEY}/{SOURCE}/19.3,34.7,29.7,41.8/{1..5}[/{YYYY-MM-DD}]
SOURCE ∈ VIIRS_SNPP_NRT | VIIRS_NOAA20_NRT | VIIRS_NOAA21_NRT | MODIS_NRT | *_SP (archive)
GET https://firms.modaps.eosdis.nasa.gov/api/data_availability/csv/{MAP_KEY}/ALL
```
VIIRS columns: latitude, longitude, bright_ti4, scan, track, acq_date, acq_time, satellite, instrument, confidence (l/n/h), version, bright_ti5, frp, daynight.

**Open-Meteo** (no key)
```
GET https://api.open-meteo.com/v1/forecast?latitude=..&longitude=..&hourly=temperature_2m,relative_humidity_2m,wind_speed_10m,wind_direction_10m,wind_gusts_10m,precipitation&wind_speed_unit=kmh&forecast_days=2
GET https://archive-api.open-meteo.com/v1/archive?latitude=..&longitude=..&start_date=2026-07-29&end_date=2026-07-31&hourly=wind_speed_10m,wind_direction_10m,wind_gusts_10m,relative_humidity_2m,temperature_2m
GET https://api.open-meteo.com/v1/elevation?latitude=a,b,c&longitude=a,b,c
```

**meteo.gr stations** (National Observatory of Athens, 600+ stations, 10-minute data). Scrape the station widget page for the nearest stations with Context.dev `web/scrape`, `formats.json` and a schema `{station, time, wind_bft, wind_dir, gust_kmh, temp_c, rh}`; `maxAgeMs: 0`. Find station pages on hour 1 (meteo.gr/meteonav, "Live Σταθμοί"); cache the station list with coordinates.

**NASA GIBS** (no key) true-colour layer for the satellite view; **EFFIS** FWI WMS as the always-on fire-danger layer; **Copernicus EMS** activations API and GeoRSS (Wildfire category).

**Greek official and news domains** (verify hour 1): civilprotection.gov.gr (announcements, daily-fire-prediction-map in season), fireservice.gr, 112.gov.gr, ertnews.gr, in.gr, protothema.gr, kathimerini.gr, skai.gr, cretalive.gr, neakriti.gr, patrisnews.com, ekriti.gr, plus per-region outlets added as sources.

**Context.dev** (`https://api.context.dev/v1`, `Authorization: Bearer ctxt_secret_...`, `pip install context.dev`)

| Endpoint | Use | Key fields | Credits |
|---|---|---|---|
| `POST /news/search` | Greek news, live and historical | `searchBy {type:"natural_language", query}`, `filterBy {date {from,to} epoch ms, articleLanguage ["el"], sourceCountry ["gr"]}`, `limit` | 1 / 10 results |
| `POST /web/search` | Official pages, fresh local reports | `query`, `freshness "last_24_hours"`, `includeDomains`, `languages ["el"]` | 1 / 10 (+1 per markdown) |
| `POST /web/scrape` | Articles, bulletins, station pages, webcams, X profiles | `url`, `formats {markdown, json, screenshot, images}`, `jsonParams {schema, instructions}`, `maxAgeMs 0` | 1 (+4 JSON) |
| `POST /monitors` | Page and search change detection → webhook | `target {type:"page"|"search"}`, `schedule {type:"interval", frequency, unit}`, `webhook {url, retry {}}`; `X-Context-Signature t=..,v1=HMAC-SHA256(t + "." + body)` | 1 page / 2 search per run |
| `POST /web/answers` | Deep dive on an incident | `task`, `mode "fast"`, `json_format` | 10 |

Budget: a Greek incident costs ~20 credits to enrich and ~50/day to monitor; the whole weekend fits in under 10,000.

**AI**: Gemini via `google-genai` (`gemini-3.8-flash` for volume; `gemini-3.1-pro-preview` or `gemini-4-argon` for briefings; `gemini-nano-banana-2.1` for the optional visual; confirm IDs at aistudio.google.com/docs/models). Nemotron `nvidia/nemotron-3.5-super-vl-120b-a12b` at `https://integrate.api.nvidia.com/v1` (~40 rpm/key). Gemma via Ollama. ElevenLabs TTS (multilingual voice for Greek).

### 6.3 Data model

```sql
create extension if not exists postgis; create extension if not exists timescaledb;

create table sources (id text primary key, name text, kind text, role text, fetch text, config jsonb,
  cadence_s int, trust real, enabled bool, replayable bool, last_run timestamptz, health jsonb);

create table scenarios (id uuid primary key, name text, bbox geography(polygon), t_start timestamptz,
  t_end timestamptz, speed int, status text, datasets jsonb);          -- live rows carry scenario_id null

create table detections (time timestamptz not null, scenario_id uuid, source_id text, lat double precision,
  lon double precision, geom geography(point) generated always as (ST_MakePoint(lon,lat)::geography) stored,
  frp real, confidence text, quality real, satellite text, attrs jsonb,
  primary key (time, lat, lon, source_id));
select create_hypertable('detections','time');

create table incidents (id uuid primary key, scenario_id uuid, status text, tier text,   -- reported|probable|confirmed
  confidence real, centroid geography(point), radius_m real, hull geography(polygon),
  first_seen timestamptz, last_seen timestamptz, det_count int, frp_total real, severity int,
  place_name text, municipality text, nearest_places jsonb, exposure jsonb, land_cover jsonb, updated_at timestamptz);

create table web_signals (id uuid primary key, scenario_id uuid, incident_id uuid, source_id text, time timestamptz,
  kind text, url text, title text, published_at timestamptz, language text, place_hints jsonb,
  geom geography(point), radius_m real, extracted jsonb, summary text, quality real);

create table weather_samples (time timestamptz, scenario_id uuid, incident_id uuid, source_id text, station text,
  geom geography(point), temp_c real, rh real, wind_kmh real, wind_dir_deg real, gust_kmh real, precip_mm real, is_forecast bool);
select create_hypertable('weather_samples','time');

create table forecasts (id uuid primary key, incident_id uuid, run_at timestamptz, horizon_h int,
  polygon geography(polygon), params jsonb, narrative text, threats jsonb);

create table depots (id uuid primary key, name text, geom geography(point));
create table drones (id uuid primary key, name text, depot_id uuid, role text, airspeed_kmh real, range_km real,
  status text, position geography(point), battery real, updated_at timestamptz);
create table drone_plans (id uuid primary key, drone_id uuid, incident_id uuid, created_at timestamptz, reason text,
  path geography(linestring), eta_s int, cost jsonb, avoided jsonb);
create table drone_telemetry (time timestamptz, drone_id uuid, geom geography(point), battery real, state text);
select create_hypertable('drone_telemetry','time');

create table alerts (id uuid primary key, incident_id uuid, time timestamptz, severity int, briefing_el text,
  briefing_en text, audio_url text, delivered_to jsonb, swarm_plan jsonb);
create table monitors (id uuid primary key, incident_id uuid, source_id text, context_monitor_id text, target jsonb, created_at timestamptz);

create materialized view incident_frp_hourly with (timescaledb.continuous) as
  select time_bucket('1 hour', d.time) bucket, i.id incident_id, sum(d.frp) frp, count(*) n
  from detections d join incidents i on ST_DWithin(d.geom, i.hull, 2000) group by 1,2;
```

### 6.4 API

```
Incidents   GET /api/incidents?scenario=&bbox=&since=      GET /api/incidents/{id}      GET /api/incidents/{id}/timeline
            GET /api/incidents/{id}/forecast               POST /api/incidents/{id}/enrich   POST /api/incidents/{id}/deep-dive
            POST /api/incidents/{id}/dispatch
Sources     GET /api/sources   POST /api/sources   PATCH /api/sources/{id}   POST /api/sources/{id}/run   POST /api/sources/{id}/test
Scenarios   POST /api/scenarios   POST /api/scenarios/{id}/load   POST /api/scenarios/{id}/upload (multipart csv|geojson|jsonl + mapping)
            POST /api/scenarios/{id}/play?speed=60   /pause   /seek?t=
Weather     GET /api/wind-field?bbox=&at=      GET /api/stations?near=
Drones      GET /api/drones   POST /api/drones/dispatch {incident_id, roles}   GET /api/drones/{id}/plan   POST /api/drones/{id}/replan
Live        GET /api/events (SSE)   POST /webhooks/context   GET /api/stats
```
SSE event types: `source.run`, `detection.new`, `signal.new`, `signal.geocoded`, `incident.new`, `incident.update`, `monitor.change`, `forecast.ready`, `alert.sent`, `drone.plan`, `drone.replan`, `drone.update`, `scenario.tick`.

### 6.5 Pipeline (live and scenario use the same code; only the Clock and the data window differ)

1. **Fetch**: each enabled source runs on its cadence against `clock.now()`; in a scenario, sources read archives, historical news (date filter) or uploaded datasets for the virtual window.
2. **Normalise**: sources return Candidates (satellite pixels, FireHub points, uploaded points) or Signals (news, 112, official, webcam, EMS).
3. **Geocode and fuse**: Gemini extracts Greek toponyms and municipality from Signals; Nominatim/Overpass geocodes (Greek and transliterated); Candidates cluster with DBSCAN (eps 2 km); Signals attach to an incident within their radius or create a *reported* incident; confidence by noisy-OR; tier set.
4. **Enrich**: nearest meteo.gr stations scraped, Open-Meteo forecast, places, hotels and campsites (exposure), land cover, fire-danger category, monitors created for the incident's query and official page.
5. **Forecast**: CA model with station wind → isochrones; Gemini threat ranking and narrative.
6. **Score and alert**: severity 0–100 = FRP (0–30) + growth (0–20) + exposure inside +3 h (0–25) + web signals (evacuation +15, trapped +25, official +10) + fire danger (0–10); above 60 → alert, bilingual briefing, voice, dispatch inbox.
7. **Respond**: drone commander assigns roles; planner routes each drone; every new station sample, forecast or signal triggers a replan check.

Webhooks (`change.detected`) re-run steps 3 to 7 for that incident immediately.

---

## 7. The AI layer (the Gemini submission)

1. **Greek extraction and geocoding hints**: article or post → `{toponyms[], municipality, region, status, time_hint, evacuations[], roads[], people_at_risk, source_quote}`.
2. **Fusion**: signals + satellite → cited situation report, conflicts surfaced.
3. **Model parameterisation**: `{fuel_class, density, suppression_factor, ignition_perimeter_hint}` from land cover, fire-risk map and imagery.
4. **Threat ranking** inside isochrones (villages, hotels, campsites, roads).
5. **Briefing** (90 words, Greek and English) → ElevenLabs.
6. **Reroute explanation**: cost-grid diff → one sentence ("Rerouted 2 km south: 55 km/h gusts on the ridge from station Spili at 14:40").
7. **Vision**: webcam frame → `{smoke: bool, confidence, bearing_hint}`; daily fire-risk map image → category per region; GIBS crop → plume bearing. Gemini by default, Nemotron if entering that track.
8. **Source-quality audit**: after a new source's first 10 runs, Gemini rates extraction consistency and proposes a trust weight for the Sources panel.

**Spread model**: Alexandridis et al. (2008) cellular automaton, 100 m cells. `p_burn = p_h (1 + p_veg)(1 + p_den) p_w p_s`, `p_w = exp(c1 V) exp(V c2 (cos θ − 1))`, `p_s = exp(a θ_s)`, with p_h = 0.58, c1 = 0.045, c2 = 0.131, a = 0.078; p_veg agricultural −0.3, shrub 0.4, pine 0.4; p_den sparse −0.4, normal 0, dense 0.3 (verify against the paper). Humidity scaling of p_h; 20 Monte-Carlo runs; 50 % contour at +1/+3/+6 h; calibrate so a forest head fire at 20 km/h wind moves 1 to 2 km/h. Wind from the nearest station when fresher than 30 minutes, else forecast.

---

## 8. Drone path planning (real-time, web-driven)

**Goal**: route each drone from its depot to its task point so that it is fast, safe and explainable, and re-route whenever the world changes.

**Inputs**
- Terrain: elevation grid from Open-Meteo elevation (batched) over the planning box; climb cost per metre.
- Wind field: Open-Meteo forecast on a 10×10 grid over the box, hourly, overridden near stations by the latest meteo.gr sample (inverse-distance blend).
- No-fly: OSM `aeroway=aerodrome` buffered 5 km (Heraklion, Chania, Sitia on Crete), military areas from OSM `landuse=military`, user-configured zones.
- Hazards: smoke plume polygon (the +1 h isochrone extended downwind by wind speed × 1 h), cells with gusts above the drone limit (default 50 km/h), the exclusion zone around the active front for non-suppression drones (aerial firefighting).
- Web events: evacuation points and "people trapped" locations become scout targets; "aerial firefighting ongoing" tightens the exclusion zone; anything Gemini extracts as a location hazard adds a cost blob.
- Population: residential polygons add a mild cost so routes prefer open country when it is nearly free.

**Planner**
- 500 m grid over a 60 × 60 km box around the incident; 8-connected A* (upgrade to Theta* for any-angle paths if time allows).
- Edge cost = time: distance / groundspeed, groundspeed = airspeed + wind component along heading (floor 5 km/h), plus climb time and hazard multipliers; no-fly cells are impassable.
- Two passes: plan with hour-0 wind, compute ETA, re-plan with the wind of the arrival hour if it differs.
- Range check against battery; if unreachable, pick the next depot or report.
- Output: path LineString, ETA, energy, list of avoided hazards with reasons.

**Re-planning triggers**: new station sample near the route, new forecast, new signal that changes targets or hazards, monitor change, every 60 simulated seconds while en route. Only commit a new path if it is at least 5 % better or the old path crosses a new hazard; emit `drone.replan` with Gemini's one-line reason.

**Simulation**: state machine idle → enroute → on_task (scout loiter 3 min, suppress drop 1 min) → return → charging; position advances along the path at groundspeed each tick; telemetry to the hypertable; commander (Gemma locally or Gemini) assigns roles from incident state.

**Visuals**: deck.gl PathLayer for routes (dashed = previous plan), TripsLayer trails, IconLayer drones with heading, PolygonLayer for no-fly and plume, a toggleable cost heatmap, wind arrows from the field.

---

## 9. Historical replay and uploads

**Scenario engine**: a scenario is `{bbox, t_start, t_end, speed, sources enabled, datasets}`. A `Clock` abstraction gives `now()`; all sources take the clock and a window. In a scenario, the scheduler advances the virtual clock at `speed` and sources read:
1. FIRMS archive via the date parameter (`*_SP` for 2025 and earlier, `*_NRT` for recent months; check `data_availability`).
2. Context.dev `news/search` with `filterBy.date` on the window (historical Greek articles are real web data, scraped on demand with `web/scrape`).
3. Context.dev `web/scrape` of fixed pages: the Copernicus EMS activation page for the fire, Wikipedia's article, archived Civil Protection PDFs (fire-risk maps for those days).
4. Open-Meteo archive weather for the window (real past wind).
5. Uploaded datasets attached to the scenario.

**Uploads**: `POST /api/scenarios/{id}/upload` accepts CSV (FIRMS export columns auto-detected, or a column-mapping form), GeoJSON (points, perimeters, no-fly zones), JSONL (web signals with url, time, text). Rows get `source_id = upload:<name>` and a user-set trust weight. The UI shows a preview and the detected time range.

**Scenarios to prepare** (one is enough for the pitch; the others prove generality):

| Scenario | When | Why |
|---|---|---|
| **Rethymno / Agios Vasileios, Crete** | ignition 29 Jul 2026 ~14:30 near Krya Vrysi; 15 km front next morning; ~4,500 ha in two days; state of emergency 30 Jul | Recent, Greek, dense coverage, NRT and SP archives both likely |
| Ierapetra, Crete | 2 to 3 Jul 2025; 112 evacuations of five settlements; ~5,000 evacuated | Tourist exposure story |
| Patras / Achaia | 12 to 13 Aug 2025; hospital and care home evacuated | Urban interface |
| Kythera | 26 to 27 Jul 2025; beach evacuation by coast guard | Island logistics |
| Evros | from 19 Aug 2023; largest EU fire on record (~93,000 ha) | Scale |
| Rhodes | 18 to 25 Jul 2023; ~19,000 evacuated | Tourist exposure at scale |

Monitors cannot be replayed; the live monitor demo uses a real page or a page we control, labelled.

---

## 10. Frontend spec (the map must look like a map)

**Base**: MapLibre GL JS with MapTiler (free plan: ~100k requests/month, non-commercial): `satellite-hybrid` style for the hero view, `outdoor-v2` for the planning view; `raster-dem` source from MapTiler terrain-RGB (check the encoding value in MapTiler's docs; `mapbox` vs `terrarium`), `map.setTerrain({ exaggeration: 1.4 })`, a hillshade layer, sky and fog, max pitch 70°, default pitch 55° when zoomed to an incident, Greek labels with Latin secondary. Alternative: Google Maps JavaScript API (hybrid or terrain) with deck.gl `GoogleMapsOverlay`; Photorealistic 3D Tiles via deck.gl `Tile3DLayer` only as a cinematic stretch.

**Layer stack (bottom to top)**: basemap → hillshade → EFFIS FWI (toggle, 30 % opacity) → GIBS true colour (toggle, date) → burnt hull → isochrones (three nested translucent fills, dashed outlines) → reported-incident uncertainty circles → hotspots (deck.gl ScatterplotLayer radius by FRP, glow; HeatmapLayer at low zoom) → web-signal pins (news, 112, official, webcam icons, clustered) → stations (wind barbs rotated by direction, labelled with gust) → no-fly and plume polygons → drone routes, trails, icons → labels.

**Chrome**: dark glass panels over a photoreal map; one accent colour for alerts; typography consistent; no default MapLibre controls visible.
- **Header**: Live / Scenario switch with scenario picker and speed, Greece counters, per-source freshness dots, replay clock.
- **Ticker**: every event, timestamped, click to fly.
- **Incident panel**: tier badge and confidence bar with the contributing sources and weights ("why we believe this"), latency per source, situation report with source chips, **incident timeline** (all signals, detections, forecasts, alerts in order; scrubbing it moves the map), weather (station readings and forecast sparkline), forecast tab, Dispatch and Deep-dive buttons, bilingual audio.
- **Sources panel**: list with enable toggles, trust slider, cadence, health and last run; "Add source" wizard (web page URL → monitor + JSON schema; search query; X account; webcam URL; API URL; CSV upload) with a Test button that shows a sample extraction.
- **Scenario panel**: pick, load (progress per dataset), play/pause/seek, upload datasets with preview.
- **Drone panel**: fleet list, roles, battery, ETA, current plan reason; cost-heatmap toggle.
- **Dispatch inbox**: alert cards, voice playback, swarm plan.
- Camera choreography in scenarios: fly-to on `incident.new`, orbit slowly while idle, snap to a drone on `drone.replan`.
- Works on fixtures with no backend; phone-width tolerant.

---

## 11. Timeline (44 hours; anchor to the real deadline)

| Hours | Milestone | Demoable |
|---|---|---|
| 0–1 | Keys (incl. MapTiler), repo scaffold, contracts frozen (schema, API, Source interface, fixtures), tunnels | — |
| 1–8 | **Slice 1**: core platform with FIRMS + upload + Open-Meteo sources, fusion, scenario engine with the Rethymno scenario (FIRMS archive + Open-Meteo archive), SSE; terrain map with hotspots, hull, ticker | Rethymno replay on 3D terrain |
| 8–16 | **Slice 2**: Context.dev sources (Greek news, official pages, meteo.gr stations, X probe), Gemini extraction and geocoding, web-first incidents with tiers, monitors + webhook, Sources panel, incident timeline | First half of the pitch |
| 16–26 | **Slice 3**: CA forecast, wind field, drone planner + sim + re-planning, severity, alerts, bilingual briefing and voice, dispatch inbox, FWI and GIBS layers | Full pitch |
| 26–34 | **Phase 2**: webcams + vision, FireHub probe result, fire-risk map reading, upload UI polish, camera choreography, Tiger aggregate chart, CRACI, Nemotron or Gemma if free | Yes |
| 34–40 | Hardening: cache everything, demo mode, backup video, deck, per-track notes, README | — |
| 40+ | Buffer, nothing new | — |

Rule: nobody starts Phase 2 before Slice 3 is on the deployed URL.

---

## 12. Four agents, four branches

Contracts live in `contracts/` (schema.sql, openapi.yaml, source_interface.py, fixtures/*.json) and are owned by Agent 1; changes go through a PR that the other three acknowledge. Each agent develops against fixtures from hour 1, so nobody waits. Agent 1 merges to `main` at hours 8, 16, 26 and 34 and owns the deployed URL.

**Agent 1 — core platform and data** (`feat/core`, integrator)
> Build the FastAPI service, the schema in docs/PLAN.md §6.3, the Source registry framework (§6.1 interface, scheduler by cadence, health, enable/disable, run/test endpoints), the Clock abstraction and scenario engine (§9: load, play/pause/seek, uploads with CSV/GeoJSON/JSONL parsing and column mapping), the FIRMS source (live and archive), the Open-Meteo source (forecast, archive, elevation, wind-field grid endpoint), the upload source, DBSCAN fusion with identity matching and noisy-OR confidence/tiers, the SSE hub, all /api/incidents, /api/sources, /api/scenarios, /api/wind-field endpoints, docker-compose (API, Timescale-HA, UI), GitHub Actions with runs-on: craci, the Vultr deploy and the credit/cache middleware. Done when a fresh clone with keys loads the Rethymno scenario and streams incidents at 60x, and live Greece polls every 10 minutes. Tests: fusion, tier rules, CSV mapping, clock.

**Agent 2 — web intelligence (Context.dev + Gemini)** (`feat/web-intel`)
> Implement the Context.dev client and these sources against the §6.1 interface: Greek news queries (news/search + web/search, date and language filters, replayable), official pages (Civil Protection, Fire Service; page monitors + scrape), X probe (112Greece, pyrosvestiki via scrape; degrade gracefully), meteo.gr stations (scrape + JSON schema, nearest-3 lookup, 10-minute cadence), Copernicus EMS activations, webcam source (screenshot → Gemini vision, Phase 2), fire-risk map source (image → Gemini vision). Implement Gemini extraction/geocoding hints, Nominatim/Overpass geocoding for Greek toponyms, the fusion report with per-claim URLs, the source-quality audit, /webhooks/context with HMAC verification and dedupe, a credit ledger, and the "test source" sample extraction. Done when a scenario window produces geocoded web-first incidents before the first satellite point, and a page change on a monitored URL updates the incident within one run.

**Agent 3 — forecast, drones and response** (`feat/response`)
> Implement the CA spread model (§7) with station-first wind, Monte-Carlo isochrones as GeoJSON, the wind-field blend (forecast grid + station override), the drone planner (§8: cost grid, A*, two-pass wind, range check, hazards, no-fly from OSM aerodromes), the drone simulation and re-planning triggers with hysteresis, the commander (Gemma via Ollama with Gemini fallback), Gemini threat ranking, narrative, reroute explanations and bilingual briefing, severity scoring, alerts, ElevenLabs audio, the simulated dispatch sink and /api/drones endpoints. Done when a synthetic incident with a 30 km/h westerly produces an eastward isochrone, a route that bends around a 5 km airport buffer, and a reroute with a reason when a station sample adds a gust cell.

**Agent 4 — terrain map UI** (`feat/ui`)
> Build the Next.js app in §10 on fixtures first: MapLibre + MapTiler terrain with hillshade and sky, the full layer stack with deck.gl, Live/Scenario header, ticker, incident panel with confidence breakdown and timeline scrubber, Sources panel with the Add-source wizard and Test button, Scenario panel with upload preview, Drone panel and layers, dispatch inbox with audio, camera choreography, dark glass chrome. Then bind to the live API and SSE. Done when the §5 demo can be clicked through on fixtures by hour 10 and on the live API by hour 26, at 30 fps or better on the demo laptop.

Daily rhythm: 10-minute sync at each merge hour; blockers go to the integrator; nobody edits another branch.

---

## 13. Demo risk register

| Risk | Mitigation |
|---|---|
| Nothing burning in Greece | Scenario loads at startup; the pitch opens on it; bbox flag for a live world view if wanted |
| X blocked | Probe in hour 1; news mirrors carry 112 texts within minutes |
| meteo.gr page structure changes or blocks | Context.dev JSON extraction is selector-free; fall back to Open-Meteo current conditions |
| FireHub not scrapeable | Drop the source; FIRMS archive carries the replay |
| MapTiler quota or key leak | Key restricted by referrer; fallback OSM raster style |
| Terrain FPS | deck.gl layers, capped pitch, test by hour 12 |
| Planner too slow | 500 m grid, 60 km box → 14,400 cells; A* in numpy under 100 ms; cache the cost grid per weather tick |
| Gemini malformed JSON | Structured output, validator, one retry, last good value |
| Monitor silent during the pitch | Manual run; a page we control, labelled |
| Archive gaps for July 2026 | Check data_availability hour 1; NRT archive via date param; else Ierapetra 2025 |
| Credits runaway | Ledger hard stop 20,000; per-incident rate limit |

---

## 14. First 60 minutes

- Context.dev key (redeem `JUNCTION26`) · NASA FIRMS MAP_KEY · Gemini key (paid tier decision) · **MapTiler key** (restricted) · ElevenLabs key · NVIDIA Build keys (if Nemotron) · Tiger Cloud service (check PostGIS; else Timescale-HA in Docker) · Vultr VM + domain or cloudflared · Vercel project · Codex log if used.
- Probes: Context.dev scrape of `x.com/112Greece`, a meteo.gr station page, the FireHub viewer; FIRMS `data_availability` for July 2026.
- Freeze contracts and commit fixtures.

---

## 15. Submission checklist

- Main track: repo, deployed URL, video, pitch (web-first detection, live monitor, drone reroute).
- Context.dev note: news/search, web/search, web/scrape (json, screenshot), monitors, answers; what each drives.
- Gemini note: the eight jobs; why Gemini (structured output, multimodal, Greek).
- Tiger: hypertables (detections, weather, telemetry), continuous aggregate, PostGIS.
- ElevenLabs: bilingual briefing. CRACI: green build. Nemotron/Gemma/Codex notes if done.

---

## 16. Sources checked

Opened directly: Context.dev docs (introduction, quickstart, scrape, search overview and filters, news overview and filters, monitors, webhooks, answers, credits); FIRMS API pages (area, index, data_availability).
From official pages via search (verify hour 1): EFFIS technical background and downloads; JRC current situation (4 Nov 2026); GIBS; Open-Meteo docs; Gemini release notes and Gemini 4 Argon page; NVIDIA forum thread on the Nemotron model id; Tiger Data PostGIS docs; Copernicus EMS harvesting; meteo.gr MeteoNav stations help and NOA network pages (600+ stations, 10-minute data); civilprotection.gov.gr daily fire-risk map press releases (categories 1–5, next-day validity, season closed for 2025); NOA BEYOND FireHub (EGU 2026 abstract: Meteosat every 5 minutes); MapTiler terms and third-party plan summaries; Euronews and France 24 on Rethymno 29–31 Jul 2026; AP/Protothema on Ierapetra 2–3 Jul 2025, Patras 12–13 Aug 2025, Kythera 26–27 Jul 2025. Alexandridis et al. 2008 constants from memory.
