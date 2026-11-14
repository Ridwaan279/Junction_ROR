# EmberWatch — Junction 2026 build plan

Working name: **EmberWatch** (rename freely). Main track: Context.dev, "The Web Is Your Real-Time Database".
Plan written Fri 14 Nov 2026 from the two challenge briefs plus primary-source checks of every API named below.

---

## 1. Decisions in one screen

| Question | Decision |
|---|---|
| What is it | A live wildfire operations map. Satellites tell us **where** heat is; the live web tells us **what is happening to people there**; a physical spread model plus Gemini tell us **what happens next**; a simulated dispatch layer shows **what we do about it**. |
| Scope | Europe-first, **world-capable**. The map always shows live incidents globally; the pitch zooms into Europe. |
| Detection source | NASA FIRMS (VIIRS + MODIS near-real-time hotspots), not web scraping. |
| Context.dev role | The unstructured layer: local-language news, civil-protection bulletins, Copernicus EMS activations, change-detection **Monitors with webhooks**, structured JSON extraction from scraped pages, "Answers" deep-dives. |
| Spread prediction | A published cellular-automaton fire model driven by Open-Meteo wind/humidity and land cover. **Gemini parameterises and explains it; Gemini does not invent the geometry.** |
| Gemini role | Structured extraction, multi-source fusion into a cited situation report, fire-model parameterisation, threat ranking, dispatcher briefing, optional public-info image (Nano Banana). |
| Emergency services | **Simulated** dispatch inbox + voice briefing (ElevenLabs) + SMS/email to our own phones. We never contact real 112. |
| Drones | Phase 2, browser-side swarm simulation on the same map. No ROS2, no physics. |
| Demo safety | Two modes on one pipeline: **Live** (global, today) and **Replay** (a real Aug 2026 Spanish megafire re-run at 60x through the identical code). |
| Sub-tracks | Enter: Context.dev Structured Web, **Gemini API** (biggest prize on the list), Tiger Data, ElevenLabs, CRACI. Add if hands free: Nemotron, Gemma, Codex, Vultr. Skip the rest. |
| Stack | Next.js + MapLibre GL + deck.gl · FastAPI (Python) · Postgres/TimescaleDB/PostGIS on Tiger Cloud · SSE for live events · Vercel + a Vultr VM (webhooks need a public URL). |

---

## 2. What the judges score (and what that means for us)

Main track criteria, 20 % each: real-world impact · creative use of live web data · completeness of demo · technical execution (latency and accuracy) · real-timeness.

What each one needs from the build:

- **Impact**: open with the number. JRC's count as of 4 Nov 2026: 703,709 ha burned in the EU this year, 2,335 fires, against a 20-year average of 386,483 ha; 2025 was the worst year on record. Users: civil-protection duty officers, volunteer brigades, NGOs, local journalists.
- **Creative use of live web data**: fusion, not one scrape. Satellite + weather + local news + official bulletins + EMS activations + monitors. Every claim on screen carries its source URL.
- **Completeness**: deployed public URL, no login, no crash, cached fallbacks, works with the Wi-Fi down.
- **Technical execution**: show pipeline latency on screen (satellite acquisition time → on map), show confidence tiers (unconfirmed / web-confirmed / official).
- **Real-timeness**: an event ticker with timestamps, a "last refreshed" clock, and a Context.dev monitor webhook landing **live during the pitch**.

Resources: 50,000 Context.dev credits (code `JUNCTION26` at context.dev/dashboard/redeem-credits), 50 concurrent requests. Mentors: Fri until 23:00, Sat 09:00 to Sun 02:00, Sun 07:00 to end.

---

## 3. Sub-tracks: enter these

| Track | Prize | Fit | Extra work | Verdict |
|---|---|---|---|---|
| Context.dev: Fully Structured Web | €750 / €250 / swag | Same code as main track; needs a note on endpoints used | 0 h | **Enter** |
| Gemini API (Google DeepMind) | **€4,000 + $1,000 GCP credits per member** / €2,000 / €1,000 | Gemini is our reasoning layer; must be visibly central | 0 h if designed in from the start | **Enter, design for it** |
| Tiger Data (TimescaleDB) | Stream Deck Mini | Hotspots and weather are time-series + geo; hypertables, continuous aggregates, PostGIS | ~2 h | **Enter** (it is the DB we need anyway) |
| ElevenLabs | Earbuds | Voice briefing to the simulated dispatcher | ~1 h | **Enter** (makes the demo dramatic) |
| CRACI | LEGO | Change one line in GitHub Actions: `runs-on: craci` | 15 min | **Enter** |
| Nemotron 3.5 Super VL (NVIDIA) | RTX 5080 ×2 | Vision analyst: reads GIBS/Sentinel imagery for smoke-plume direction, reads scraped PDF bulletins and screenshots | ~4 h | **Add at hour 20 if a person is free** |
| Gemma | €1,000 ×3, no ranking | The on-board "drone commander" running locally (Ollama) and turning incident state into swarm task assignments | ~3 h, Phase 2 | **Add with the drone layer** |
| Codex (OpenAI) | $10k / $5k / $2.5k API credits | Open; just use Codex meaningfully in the build and document it | 0 h if one teammate builds with Codex | Enter if true |
| Vultr | M5Stack | Host the backend VM there | 0 h if we host there anyway | Enter if hosting there |
| Modal | $2,000 + $3,000 credits | Would host the fire model / an open model behind a URL; the sim is CPU-light, so this is gratuitous | 3 h+ | Skip unless someone loves Modal |
| Aiven | €1,000 | Kafka/Postgres; overlaps Tiger Data; Kafka is overhead at 48 h | 3 h+ | Skip |
| Oxylabs | €1,000 voucher | A second scraping API in the core flow will look mercenary to Context.dev judges | 2 h | Skip |
| MongoDB Atlas | M5Stack kit | Overlaps Tiger Data | — | Skip |
| Verda, Cloudinary, Entire, Solana, Arduino UNO Q, Atech | — | Compute/hardware/forge not needed; drones stay virtual | — | Skip |

Prize-weighted priority: **Gemini > main track > Context.dev side prize > Nemotron > Gemma > Tiger/ElevenLabs/CRACI**. Design the AI layer so Gemini is unmistakably the brain.

---

## 4. What is flawed in the idea, and the fix for each

1. **There are almost no wildfires in Europe in mid-November.** The season is over; FIRMS over Europe this weekend will show scattered agricultural burns. A "live map of Europe" risks being empty on stage.
   *Fix:* world-capable by default (southern-hemisphere season is active: Australia, southern Africa, South America; India's crop-burning season shows thousands of hotspots in November), the EFFIS fire-danger (FWI) forecast layer so Europe is never blank, and a **Replay mode** that pushes a real 2026 Spanish megafire through the identical pipeline at 60x. Say so on stage: "live now, replayed at 60x for the story".

2. **Scraping is the wrong tool for detection.** Hotspots are a structured API. If our Context.dev usage is "scrape the FIRMS page", the "creative use of live web data" score collapses.
   *Fix:* Context.dev owns everything only the live web has: local-language news, civil-protection pages, evacuation orders, road closures, EMS activation pages, and change detection via Monitors. The satellite finds the fire; the web finds the people. That sentence is the pitch.

3. **Asking an LLM to predict fire spread is indefensible.** It will return plausible, arbitrary geometry. A judge with any fire-science background (Finland has them) will dismiss it, and an AI-generated *picture* of a spreading fire in an emergency product is a liability.
   *Fix:* compute isochrones (+1 h / +3 h / +6 h) with a small published cellular-automaton model (Section 7). Gemini chooses the model's inputs from evidence (fuel type and density from land cover and imagery, suppression assumptions), ranks what is inside the polygons, and writes the explanation. Generated imagery, if kept, is a labelled "public information visual", never the forecast.

4. **Messaging real emergency services is illegal in most of Europe and will get the team removed.**
   *Fix:* a simulated dispatch inbox in the UI, a voice briefing, and SMS/email to the team's own numbers. Label it.

5. **The drone swarm is scope creep.** Without ROS2 it is an allocation algorithm plus an animation.
   *Fix:* Phase 2 only. A browser-side deck.gl layer: a commander model emits task assignments (scout the reported "people trapped" location, drop on the predicted head of the fire), drones animate along routes from the nearest OSM fire station. No physics.

6. **"Live" for satellites means hours, not seconds.** NRT VIIRS detections appear roughly 2 to 3 hours after overpass (EFFIS states this), with about six VIIRS passes a day.
   *Fix:* poll FIRMS every 10 min and show detection latency honestly; put minute-level reactivity in the web layer (monitors, news). Judges reward honesty about latency more than fake instantaneity.

7. **FIRMS has false positives** (gas flares, steel plants, solar farms, field burning).
   *Fix:* cluster detections into incidents (DBSCAN, ~2 km), require two detections or an FRP threshold, drop low confidence, and use the web to confirm. Show the confirmation tier in the UI. This is a feature, not a bug: "unconfirmed heat → web-confirmed fire → official incident".

8. **Satellite imagery scraping is unnecessary.** NASA GIBS serves free daily true-colour tiles with no key; Sentinel-2 (10 m) needs a Copernicus account and is cloudy and 5-day revisit.
   *Fix:* GIBS WMTS as a map layer with a date picker. Skip Sentinel unless Nemotron needs a close-up.

9. **48 hours with everything in one branch.** The plan below has a demoable vertical slice at hour 8 and independent workstreams with contracts fixed at hour 1.

---

## 5. The product and the demo storyline

**Tagline:** "The satellite sees the heat. The web sees the people. EmberWatch puts both on one map and tells you what happens in the next three hours."

**Demo script (4 minutes):**

1. Open on the dark globe/Europe map. Header: "Live · 312 active incidents worldwide · 4 in Europe · satellite refresh 11:42 · web refresh 11:51". Ticker scrolling real events.
2. Toggle **Replay: Ávila–Madrid fire, Aug 2026, 60x**. Clock starts.
3. 14:02 first VIIRS hotspot. 14:20 a second pass clusters into an incident, severity 31, tier "unconfirmed heat".
4. Web panel fills: Context.dev news search returns two local Spanish articles within the hour; scrape + JSON extraction pulls "evacuation of two villages, N-110 closed". Tier flips to "web-confirmed". Each line has a source link.
5. A **monitor** on the regional civil-protection page fires live (we trigger a run during the pitch): page changed → new evacuation order extracted → severity 78 → alert.
6. Spread forecast draws +1 h/+3 h/+6 h isochrones using live Open-Meteo wind (arrow on map). Gemini's threat list: "San Bartolomé de Pinares inside +3 h envelope, 612 residents, one road out."
7. **Dispatch**: briefing appears in the dispatcher inbox and is read aloud by ElevenLabs. Drone swarm launches from the nearest fire station: two scouts to the reported trapped-people location, four suppression drones to the head of the fire.
8. Switch back to Live. Show the same panels on a real incident happening right now somewhere in the world.

---

## 6. Architecture

```
FIRMS (VIIRS/MODIS NRT) ──┐
Open-Meteo (wind/RH/T)  ──┤
OSM/Overpass (places,   ──┤   FastAPI backend ──► Postgres + TimescaleDB + PostGIS (Tiger Cloud)
  fire stations, landuse) ┤   APScheduler jobs         │
GIBS tiles (map layer)  ──┤   /webhooks/context  ◄── Context.dev Monitors (change.detected)
Context.dev news/search/──┤   /api/events (SSE)  ──► Next.js + MapLibre + deck.gl UI
  scrape+JSON/answers   ──┘   Gemini · ElevenLabs · (Nemotron VL) · (Gemma via Ollama)
```

### 6.1 Data sources (verified)

**NASA FIRMS** (free MAP_KEY from firms.modaps.eosdis.nasa.gov/api/map_key; 5,000 transactions per 10 min)

```
GET https://firms.modaps.eosdis.nasa.gov/api/area/csv/{MAP_KEY}/{SOURCE}/{west},{south},{east},{north}/{DAY_RANGE 1..5}[/{YYYY-MM-DD}]
SOURCE ∈ VIIRS_SNPP_NRT | VIIRS_NOAA20_NRT | VIIRS_NOAA21_NRT | MODIS_NRT   (+ *_SP for archive)
Europe bbox: -25,34,45,72      World: world
Archive availability check: GET /api/data_availability/csv/{MAP_KEY}/ALL
```
VIIRS CSV columns: latitude, longitude, bright_ti4, scan, track, acq_date, acq_time, satellite, instrument, confidence (l/n/h), version, bright_ti5, frp, daynight. Poll every 10 min, all three VIIRS sources plus MODIS, Europe bbox with day range 1 and world with day range 1. Dedupe on (lat, lon, acq_date, acq_time, satellite).

**Open-Meteo** (no key)
```
GET https://api.open-meteo.com/v1/forecast?latitude=..&longitude=..&hourly=temperature_2m,relative_humidity_2m,wind_speed_10m,wind_direction_10m,wind_gusts_10m,precipitation&wind_speed_unit=kmh&forecast_days=2
GET https://api.open-meteo.com/v1/elevation?latitude=a,b,c&longitude=a,b,c
```

**NASA GIBS** (no key, Web Mercator tiles, max zoom 9, 256 px)
```
https://gibs.earthdata.nasa.gov/wmts/epsg3857/best/VIIRS_SNPP_CorrectedReflectance_TrueColor/default/{YYYY-MM-DD}/GoogleMapsCompatible_Level9/{z}/{y}/{x}.jpg
```
Use yesterday's date by default (today's tiles are mostly empty). Thermal-anomaly overlay layers also exist (e.g. VIIRS_SNPP_Thermal_Anomalies_375m_Day).

**EFFIS / Copernicus** (free): fire-danger forecast (ECMWF FWI) as a WMS layer, documented at forest-fire.emergency.copernicus.eu/downloads-instructions. Copernicus EMS Rapid Mapping has a public activations API and per-activation GeoRSS feeds with a "Wildfire" category. Both are structured, so they are a backend fetch, not a scrape.

**OSM Overpass**: places with population, `amenity=fire_station`, and `landuse`/`natural` polygons around an incident for the fuel map. Nominatim for reverse geocoding (1 req/s, set a User-Agent, cache).

**Context.dev** (base `https://api.context.dev/v1`, header `Authorization: Bearer ctxt_secret_...`; SDKs `pip install context.dev`, `npm install context.dev`)

| Endpoint | We use it for | Key fields | Credits |
|---|---|---|---|
| `POST /news/search` | Local news about an incident, live and historical (works for Replay too) | `searchBy: {type:"natural_language", query}`, `filterBy: {date:{from,to} (epoch ms), articleLanguage:["es"], sourceCountry:["es"]}`, `limit` | 1 per 10 results |
| `POST /web/search` | Official pages and fresh reports | `query`, `freshness:"last_24_hours"`, `includeDomains:[civil-protection domains]`, `languages`, `numResults` | 1 per 10 results (+1 per result with markdown) |
| `POST /web/scrape` | Read a bulletin/article and extract structured facts | `url`, `formats:{markdown:true, json:true, screenshot:true}`, `jsonParams:{schema, instructions}`, `maxAgeMs:0` for fresh | 1 (+4 when JSON succeeds) |
| `POST /monitors` | Change detection on the search query and on official pages; webhook to us | `target:{type:"search"|"page", ...}`, `schedule:{type:"interval",frequency,unit}`, `webhook:{url, retry:{}}`; events `change.detected`, `run.completed`; signature header `X-Context-Signature: t=..,v1=HMAC-SHA256(t + "." + body)` with the `whsec_` secret; reply 2xx within 15 s | 2 per run (search), 1 (page) |
| `POST /web/answers` | "Deep dive" button: researched answer with sources | `task`, `mode:"fast"` (≤30 s, 10 credits) or `"ultra"` (≤50 s, 100 credits), `json_format` | 10 / 100 |

Credit budget: enrichment of one incident ≈ 2 searches + 3 JSON scrapes ≈ 17 credits; hourly search monitor ≈ 48/day. Enriching 100 incidents/day and monitoring the top 20 ≈ 2,700 credits/day. We have 50,000.

Official-source domains to seed `includeDomains` and page monitors (verify each on day 1, add regional ones per incident): proteccioncivil.es, miteco.gob.es (ES) · prociv.gov.pt, fogos.pt (PT; fogos.pt also has an open JSON API) · civilprotection.gov.gr (GR) · protezionecivile.gov.it, vigilfuoco.it (IT) · meteofrance.com vigilance, prefecture sites (FR) · mapping.emergency.copernicus.eu, forest-fire.emergency.copernicus.eu "Fire News" (EU).

**AI**
- Gemini (SDK `google-genai`): `gemini-3.8-flash` (GA, fast, cheap) for extraction, fusion and ranking at volume; `gemini-3.1-pro-preview` or `gemini-4-argon` (paid tier) for the final briefing; images via `gemini-nano-banana-2.1` or `gemini-3-pro-image`. Confirm IDs on the models page (aistudio.google.com/docs/models); the Gemini 3 Pro preview was retired and IDs move fast.
- Nemotron 3.5 Super VL: OpenAI-compatible, base `https://integrate.api.nvidia.com/v1`, model `nvidia/nemotron-3.5-super-vl-120b-a12b` (community-confirmed; check the `model` field in the response), ~40 requests/min per key, one key per teammate, retries with backoff.
- Gemma: latest Gemma via Ollama on a laptop for the drone commander. Only with Phase 2.
- ElevenLabs: `POST /v1/text-to-speech/{voice_id}` for the briefing audio.

### 6.2 Data model (Postgres + TimescaleDB + PostGIS)

```sql
create extension if not exists postgis; create extension if not exists timescaledb;

create table detections (time timestamptz not null, lat double precision, lon double precision,
  geom geography(point) generated always as (ST_MakePoint(lon,lat)::geography) stored,
  frp real, confidence text, satellite text, instrument text, source text, daynight char(1),
  primary key (time, lat, lon, satellite));
select create_hypertable('detections','time');

create table incidents (id uuid primary key, status text, tier text,            -- unconfirmed | web_confirmed | official
  centroid geography(point), hull geography(polygon), first_seen timestamptz, last_seen timestamptz,
  det_count int, frp_total real, severity int, country text, place_name text, nearest_places jsonb,
  land_cover jsonb, updated_at timestamptz);

create table weather_samples (time timestamptz, incident_id uuid, temp_c real, rh real,
  wind_kmh real, wind_dir_deg real, gust_kmh real, precip_mm real, is_forecast bool);
select create_hypertable('weather_samples','time');

create table web_signals (id uuid primary key, incident_id uuid, time timestamptz,
  kind text,                                   -- news | page | monitor | answer | ems_activation
  url text, title text, published_at timestamptz, source text, extracted jsonb, summary text, confidence real);

create table forecasts (id uuid primary key, incident_id uuid, run_at timestamptz, horizon_h int,
  polygon geography(polygon), params jsonb, narrative text, threats jsonb);

create table alerts (id uuid primary key, incident_id uuid, time timestamptz, severity int,
  briefing text, audio_url text, delivered_to jsonb, swarm_plan jsonb);

create table monitors (id uuid primary key, incident_id uuid, context_monitor_id text, target jsonb, created_at timestamptz);

-- Tiger Data showcase: hourly FRP per incident, served to the chart without recomputation
create materialized view incident_frp_hourly with (timescaledb.continuous) as
  select time_bucket('1 hour', d.time) as bucket, i.id as incident_id, sum(d.frp) frp, count(*) n
  from detections d join incidents i on ST_DWithin(d.geom, i.hull, 2000) group by 1,2;
```
If PostGIS is not on the Tiger free plan (check at the booth), run `timescale/timescaledb-ha:pg17` in Docker; it ships PostGIS.

### 6.3 Backend API (FastAPI)

```
GET  /api/incidents?bbox=&since=              GeoJSON FeatureCollection (markers + hulls)
GET  /api/incidents/{id}                      detail: weather, signals (with urls), forecast, alerts, tier, latency
GET  /api/incidents/{id}/forecast             GeoJSON polygons for +1/+3/+6 h + params + narrative
POST /api/incidents/{id}/enrich               manual re-enrichment (used on stage)
POST /api/incidents/{id}/deep-dive            Context.dev Answers (fast mode)
POST /api/incidents/{id}/dispatch             creates alert, briefing, audio, swarm plan
GET  /api/events                              SSE stream of {type, incident_id, payload, ts}
POST /webhooks/context                        verify X-Context-Signature, dedupe on X-Context-Id, enqueue
POST /api/replay/{scenario}?speed=60          start/stop replay clock
GET  /api/stats                               header counters from continuous aggregates
```
Event types: `detection.new`, `incident.new`, `incident.update`, `signal.new`, `monitor.change`, `forecast.ready`, `alert.sent`, `drone.update`, `clock.tick` (replay).

### 6.4 Pipeline

1. **Ingest** (every 10 min): FIRMS pulls → upsert detections. In Replay, the same job reads the archived day with the date parameter and emits rows against the replay clock.
2. **Cluster** (after every ingest): DBSCAN over detections in the last 24 h (eps 2 km, min_samples 2). Match clusters to existing incidents by centroid distance (< 3 km) to keep identity. Compute centroid, hull, FRP sum, count, trend.
3. **Enrich** (new or growing incident, max 1 per 30 min per incident): reverse geocode → nearest places and fire stations (Overpass) → land cover → Open-Meteo now + 12 h forecast → Context.dev news (last 24 h, local language) + web search (official domains) → scrape top 3 with JSON schema `{status, evacuations[], roads_closed[], people_trapped_or_missing, casualties, resources_deployed, official_source, quotes[]}` → Gemini fuses into a situation report with per-claim source URLs and sets the tier → create monitors (search target hourly; page target on the matched official page) → emit events.
4. **Forecast** (after enrich, and on each weather refresh): CA model → isochrones → Gemini threat ranking + narrative.
5. **Score and alert**: severity 0–100 = FRP (log-scaled, 0–30) + growth (0–20) + population inside +3 h envelope (0–25) + web signals (evacuation +15, trapped people +25, official incident +10) + FWI class (0–10). Above 60 → alert → briefing → voice → dispatch inbox → swarm plan.
6. **Webhook**: `change.detected` → re-enrich that incident immediately → `monitor.change` event with the diff summary.

---

## 7. The AI layer in detail (this is the Gemini submission)

**Gemini jobs, each visible in the UI**

1. *Extraction*: scraped markdown → JSON schema (Gemini's structured output) with a `source_quote` per field, so every fact is attributable.
2. *Fusion*: N signals + satellite facts → situation report with confidence tiers and conflicting-report handling ("local radio says 3 villages, prefecture says 2").
3. *Model parameterisation*: land-cover summary, imagery and bulletins → `{fuel_class, density, suppression_factor, ignition_perimeter_hint}` for the CA model. This is how Gemini "predicts" without inventing geometry.
4. *Threat ranking*: isochrone polygons × places/roads → ordered threats with reasons.
5. *Briefing*: a 90-word dispatcher briefing (then voiced by ElevenLabs).
6. *Optional*: Nano Banana public-information visual from the map screenshot, watermarked "illustrative".

**Spread model**: cellular automaton from Alexandridis et al. (2008, *Applied Mathematics and Computation* 204), built for a Greek island wildfire. Cells 100 m. Each burning cell ignites a fuel neighbour with

```
p_burn = p_h · (1 + p_veg) · (1 + p_den) · p_w · p_s
p_w    = exp(c1·V) · exp(V·c2·(cos θ − 1))      V wind speed, θ angle between wind and spread direction
p_s    = exp(a·θ_s)                             θ_s slope angle toward the neighbour
p_h = 0.58, c1 = 0.045, c2 = 0.131, a = 0.078
p_veg: agricultural −0.3, shrub 0.4, pine forest 0.4 · p_den: sparse −0.4, normal 0, dense 0.3
```
Verify the constants against the paper before the pitch. Add a humidity scaling of p_h (RH 20 % → ×1.2, RH 60 % → ×0.6). Run 20 Monte-Carlo passes, take the 50 % burn-probability contour at +1/+3/+6 h as the isochrone, and calibrate the time step so that a forest head fire at 20 km/h wind advances about 1–2 km/h. Fuel grid from OSM landuse; default to "forest, normal density" where OSM is empty. Slope from Open-Meteo elevation on the grid corners (skip in v1 if time is short).

**Nemotron (optional track)**: feed it the GIBS true-colour crop and the latest scraped screenshot; ask for smoke-plume bearing, visible burn-scar extent and anything in a PDF bulletin. Its outputs become `ignition_perimeter_hint` and an "imagery" signal with the image attached. Submission text must say what it does and how we improved its outputs (few-shot examples, JSON mode, a second pass that checks the bearing against Open-Meteo wind).

**Gemma (optional track)**: the drone commander. Input: incident state + fire-station location + reported trapped-people points + isochrones. Output: JSON task list per drone. Runs locally on a laptop via Ollama. The story: "cloud model for the command centre, open model at the edge".

---

## 8. Frontend spec

Dark ops-centre look, one screen, no login.

- **Map** (MapLibre GL, globe projection, dark basemap): incident markers sized by FRP, coloured by severity, pulsing when updated in the last 10 min; hulls; isochrone polygons (toggle); wind arrow at the incident; GIBS true-colour layer with date picker; EFFIS FWI layer toggle; drone layer (deck.gl TripsLayer) in Phase 2.
- **Header**: Live/Replay switch, worldwide and Europe counters, satellite and web "last refreshed" clocks, replay clock and speed.
- **Ticker** (left or bottom): every SSE event with a timestamp and a link to the incident.
- **Incident panel** (right, opens on click with a fly-to): tier badge, severity bar, detection latency, weather now + 12 h sparkline, situation report with inline source chips, signals list (news, pages, monitor changes, EMS activation), forecast tab (isochrones + threat list + narrative), "Deep dive" and "Dispatch" buttons, briefing audio player, FRP chart from the continuous aggregate.
- **Dispatch inbox** (modal): alerts as cards, "sent to: simulated dispatcher, +358… (team phone)", voice playback, swarm plan.
- Must work at phone width for judges wandering past.

---

## 9. Phases and timeline

Anchor T0 to the actual hacking start and check the real submission deadline; this assumes about 40 productive hours.

| Hours | Milestone | Demoable? |
|---|---|---|
| 0–1 | Accounts and keys (Section 12), repo scaffold, contracts (Sections 6.2–6.3) frozen, tunnels for webhooks | — |
| 1–8 | **Slice 1**: FIRMS → detections → DBSCAN incidents → map markers (world + Europe) → click → weather panel → SSE ticker | **Yes**: a live global fire map |
| 8–16 | **Slice 2**: Context.dev news + search + scrape/JSON → Gemini fusion → situation report with sources → tier badges → monitors + webhook → Replay mode with the Ávila scenario | **Yes**: the pitch's first half |
| 16–26 | **Slice 3**: CA spread model → isochrones → Gemini threats and narrative → severity → alerts → ElevenLabs briefing → dispatch inbox → GIBS layer | **Yes**: full Phase 1 + alerts |
| 26–34 | **Phase 2**: drone swarm layer + commander (Gemma or Gemini); Nemotron imagery signal if a person is free; CRACI CI; Tiger continuous aggregate chart | Yes |
| 34–40 | Hardening: cache every external call, demo-mode flag, pre-recorded backup video, pitch deck, per-track submission notes, README | — |
| 40+ | Buffer. Nothing new. | — |

Rule: nobody starts Phase 2 before Slice 3 is on the deployed URL.

---

## 10. Workstreams for sub-agents (or teammates)

Run each as a separate Claude Code / Codex session on its own branch and worktree. Contracts are the DB schema and API in Section 6; nobody changes a contract without telling the integrator. Each agent gets the prompt below plus this document.

**Agent A — Ingest and incidents** (`feat/ingest`)
> Build the FastAPI service skeleton, the Postgres/Timescale/PostGIS schema in docs/PLAN.md §6.2, the FIRMS poller (all VIIRS NRT sources + MODIS, Europe bbox and world, 10-min APScheduler job, idempotent upserts), DBSCAN clustering into incidents with identity matching, Open-Meteo enrichment, Overpass nearest places and fire stations, the SSE /api/events hub, /api/incidents endpoints returning GeoJSON, and Replay mode (date parameter + replay clock). Done when a fresh clone with keys shows live world incidents in GeoJSON within 2 minutes and the Ávila replay emits events at 60x. Write tests for the clustering and the identity matching with fixture CSVs.

**Agent B — Web intelligence** (`feat/web-intel`)
> Implement the Context.dev client (news/search with date and language filters, web/search with freshness and includeDomains, scrape with formats json+markdown+screenshot and a JSON schema, monitors create/run/delete, answers fast mode) and the Gemini extraction + fusion prompts with structured output. Produce web_signals rows and the situation report with per-claim source URLs and a tier. Implement /webhooks/context with HMAC verification (t + "." + raw body, 300 s tolerance, dedupe on X-Context-Id) that re-enriches the incident and emits monitor.change. Keep a credit ledger. Done when enriching a given incident id produces a cited report and a page change on a monitored URL updates it within one monitor run.

**Agent C — Forecast and alerting** (`feat/forecast`)
> Implement the Alexandridis cellular-automaton spread model in numpy on a 100 m grid around an incident (fuel from OSM landuse, wind/RH from weather_samples, optional slope), Monte-Carlo isochrones at +1/+3/+6 h as GeoJSON, the Gemini parameterisation/threat/narrative/briefing prompts, the severity score, alerts, ElevenLabs audio, and /api/incidents/{id}/dispatch with a simulated dispatcher sink (inbox table + optional SMS/email to configured team numbers only). Done when a synthetic incident with 30 km/h westerly wind produces an eastward-elongated polygon, a cited threat list and an MP3 briefing.

**Agent D — Frontend** (`feat/ui`)
> Build the Next.js + MapLibre + deck.gl app in docs/PLAN.md §8 against the API in §6.3, with mock JSON fixtures first so it is not blocked on the backend. Live/Replay switch, ticker from SSE, incident panel, forecast layer, GIBS and EFFIS layers, dispatch inbox, FRP chart. Phone-width layout. Done when the full demo script in §5 can be clicked through on fixtures and then on the live API.

**Agent E — Drone swarm (Phase 2)** (`feat/swarm`)
> Browser-side swarm simulation: commander (Gemma via Ollama with a Gemini fallback) returns a JSON task list; drones launch from the nearest fire station, follow straight-line routes with simple turning, scouts go to reported trapped-people points, suppressors to the isochrone head; render with deck.gl TripsLayer; emit drone.update events for the ticker. No physics.

**Agent F — Integration, infra, submission** (integrator, `main`)
> Repo scaffold, docker-compose (API + Timescale-HA + UI), GitHub Actions with `runs-on: craci`, Vultr VM + Caddy for the backend, Vercel for the UI, env templates, cache-everything middleware, demo-mode flag, README, pitch deck, per-track submission notes (Context.dev endpoints used; Gemini role; Nemotron explanation text; Tiger usage; ElevenLabs usage), backup video. Merges the feature branches and owns the deployed URL.

Suggested split for 4 people: A+F, B, C, D. For 5: add E. For 3: A+C, B+F, D, and skip E and Nemotron.

---

## 11. Demo risk register

| Risk | Mitigation |
|---|---|
| No fires in Europe on stage | World counters + FWI layer + Replay; say it out loud |
| An external API is down or rate-limited | Every outbound call goes through a cache with a 24 h TTL; demo-mode serves the cache |
| Context.dev monitor does not fire during the 4 minutes | Trigger a run manually via the API; keep one monitor on a page we control and change it live, and say it stands in for the real civil-protection page |
| Gemini returns malformed JSON | Structured output + a validator + one retry; fall back to the last good report |
| Venue Wi-Fi | Backup video; hotspot from a phone; the UI works on fixtures |
| Forecast looks wrong | Show the wind arrow and the parameters next to the polygon; call it a "first-order forecast" |
| Credits burned by a runaway loop | Ledger with a hard stop at 20,000; enrichment rate limit per incident |
| FIRMS archive for August not yet in the SP sources | Check `/api/data_availability` on hour 1; NRT archive with the date parameter usually covers it; else pick a more recent fire |

---

## 12. First 60 minutes: accounts and keys

- Context.dev: sign up, redeem `JUNCTION26`, create a key, confirm 50 concurrent requests.
- NASA FIRMS MAP_KEY (instant, email).
- Gemini API key (AI Studio); decide whether the paid tier is on for `gemini-4-argon`.
- ElevenLabs key.
- NVIDIA Build key per teammate (if Nemotron).
- Tiger Cloud service; check PostGIS; else Timescale-HA in Docker.
- Vultr VM (or any VM with a public IP) + domain or `cloudflared` tunnel for webhooks.
- Vercel project.
- Codex access if one teammate builds with it; keep a log of what it built.

---

## 13. Submission checklist

- Main track: repo, deployed URL, 3-minute video, pitch. Lead with impact numbers and the live monitor webhook.
- Context.dev side prize: note listing `news/search`, `web/search`, `web/scrape` (json), `monitors` (webhooks), `web/answers`, and why each.
- Gemini: paragraph on the six Gemini jobs and why Gemini (structured output, multimodal, long context for fusion).
- Tiger Data: hypertables, continuous aggregate, PostGIS clustering; screenshot of the FRP chart.
- ElevenLabs: the briefing audio.
- CRACI: Actions file with `runs-on: craci` and a green build.
- Nemotron (if done): text in the form naming the model, its job, and the quality loop.
- Gemma (if done): the commander prompt and a clip of it assigning tasks.
- Codex (if used): short write-up of what Codex built.

---

## 14. Sources checked while writing this

- Main and tech challenge briefs (the two uploaded documents).
- Context.dev docs: introduction, quickstart, scrape reference, search overview and filters, news overview and filter/sort, monitors overview, webhooks, credits.
- NASA FIRMS API pages: area, data_availability (URL shapes, sources, quota).
- EFFIS technical background (NRT latency, FIRMS as source), downloads instructions (FWI WMS).
- JRC "Current wildfire situation in Europe" (4 Nov 2026 figures).
- NASA GIBS WMTS endpoint pattern and VIIRS true-colour layer.
- Open-Meteo forecast docs (variable names).
- Gemini release notes and model pages (3.8-flash GA, 3-pro-preview retired, Gemini 4 Argon, Nano Banana IDs).
- NVIDIA developer forum threads on the Nemotron 3.5 Super VL model ID on integrate.api.nvidia.com.
- Tiger Data docs on PostGIS.
- Copernicus EMS mapping data harvesting pages (activations API, GeoRSS).
- Alexandridis et al. 2008 CA wildfire model (constants from memory; verify).
