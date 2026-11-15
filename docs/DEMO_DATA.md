# Demo data: Arganil / Piódão, first 42 hours (13–14 Aug 2025)

The early-stage demo replays the first 42 hours of a **real** fire using only
information that was public at the time. Nothing in it is invented. The web
content was captured through Context.dev. Each item is revealed in the replay only
from the moment it was provably public.

| What | Where |
|---|---|
| Replay files the UI loads | `web/public/fixtures/v2/arganil-early/index.json` and `frames/00.json` … `frames/14.json` (≈ 0.6 MB) |
| Context.dev captures (facts + short quotes, request IDs, credits) | `data/demo/arganil-early/context_dev/pages.json`, `search_results.json` |
| Place coordinates | `data/demo/arganil-early/context_dev/places.json` (GeoNames) |
| Exact Context.dev request bodies and capture tools | `data/demo/arganil-early/capture/` |
| Build script | `python3 -I data/demo/arganil-early/build.py` (offline; needs no key) |

## Sources and what each contributes

| Monitor | Provider | Contributes | Status in this capture |
|---|---|---|---|
| Official incident page | Context.dev `web/scrape` of `fogos.pt/pt/fogo/20251075979` | Start 05:08 local, Despacho 05:14, 1st-alert dispatch 05:20, Em Curso 06:16, official point 40.23083, −7.81972, nature "Mato" | ok. The page's meteo block and resource counters show **present-day** values and are excluded |
| News (index) | Context.dev `news/search` with a 13–14 Aug window | Nothing usable | Coverage gap: off-topic Portuguese results, `sourceCountry: "pt"` rejected, Portuguese publisher domains return 0 |
| News (web) | Context.dev `web/search` + `web/scrape` with a JSON schema | Origin (dry lightning), evacuations, shelters, resources over the day, spread direction, threatened villages, road closures | 20 pages ok, 2 `WEBSITE_BLOCKED` (correiodabeiraserra.com) |
| Events near the fire | Context.dev `web/search` + `web/scrape` | Festa de Verão do Piódão: due to start Fri 15 Aug, cancelled (Observador, 14 Aug 18:03) | ok. Facebook/Instagram announcements found by search were not scraped |
| Warnings / fire danger | Context.dev, dated news reports of IPMA notices | ~120 municipalities at maximum fire risk (11 Aug); orange heat warning extended to Thursday (12 Aug) | ok. IPMA's API only serves current warnings |
| Weather | Open-Meteo archive (ERA5), nearest of 9 grid points (8.1 km) | Hourly temperature, RH, 10 m and 100 m wind speed/direction, gusts, cloud, radiation, rain, VPD | ok |
| Satellite hotspots | NASA FIRMS | — | not run (needs `FIRMS_MAP_KEY`) |
| Terrain / fuel | Terrarium DEM; vegetation altitudinal assumption | Model inputs | ok (OSM land cover unavailable) |
| Model | firecast (Rothermel ensemble + Gaussian plume) | Burn probability, 6/12 h envelopes, smoke, wind, head behaviour | Existing 3-hourly replay frames |

Capture run: 9 searches + 25 scrapes, 129 credits, request IDs recorded per call.

## Accuracy rules applied (all in `build.py`)

1. **Quote verification.** Each extracted quote must appear verbatim in the scraped
   page text (after whitespace/Unicode normalisation): 251 of 267 facts pass and are
   `quality: "verified"`. The rest are kept as `"unverified"` and must not drive automation.
2. **Number check.** `number_check: "in_quote"` when the number is literally in the
   quote; `"not_literal_in_quote"` when written in words (e.g. "cinco centenas").
3. **When an item becomes known (`known_at`).** The later of page metadata
   `publishedTime`, printed time (Europe/Lisbon → UTC) and `modifiedTime`.
   - Printed and metadata time disagree → the later is used and flagged (As Beiras: printed 13h31, metadata 13:31 UTC).
   - Live-updated pages that mention later clock times are revealed at the latest time mentioned (Público: published 07:59, cites ANEPC "às 17h" → 16:00 UTC).
   - Date only → end of that local day (SIC).
   - Fogos.pt statuses → each at its own status time.
4. **Category check.** The extractor's category is not trusted alone: "event" requires
   a named event in the quote, otherwise it becomes `people_present` (one Sábado fact).
5. **Relative dates.** "sexta-feira" is resolved to the next Friday after the
   publication date, and the basis is stored.
6. **Lineage.** Identical headlines form one lineage. The Lusa story carried by
   rr.pt, Sábado and Observador counts as one independent source.
7. **Places.** GeoNames match within 30 km of the official ignition point;
   unresolved names stay unresolved (9 of 34, including Foz de Égua). Municipality names are context, not exposure sites.
8. **No silent zeros.** Failed captures are listed with their error code; they never
   mean "nothing happened".

## Format

All times are UTC (RFC 3339). Coordinates are `[lon, lat]`. Unknown values are `null`.

### `index.json`

| Field | Meaning |
|---|---|
| `mode` | `"historical"` |
| `window` | `{from, to}` of the replay |
| `ignition` | Official Fogos.pt point, start text, nature, place; `model_ignition` is the point the model frames were run from (0.4 km away) |
| `timeline[]` | Fogos.pt statuses `{at, status, source, url, request_id}` |
| `frames[]` | `{t, file, evidence, exposures, area_ha_p50}` |
| `pages[]` | One per capture: `id, url, publisher, headline, request_id, captured_at, credits, status, error_code, mentions_lusa, lineage, published_at, modified_at, time_basis, known_at, time_flags[]` |
| `evidence[]` | One per fact: `id, page, category, extracted_category, checks[], event, statement_en, quote (≤300 chars, Portuguese), places[{name, geonames, position}], time_text, number, unit, quality, number_check, known_at, url, publisher, lineage, mode` |
| `searches[]` | `{name, endpoint, request_id, credits, error, results}` |
| `monitors[]` | `{id, category, provider, target, status, items, note}`; status is `ok`, `coverage_gap`, `not_run` or `failed` |
| `limitations[]` | Shown in the UI's source panel |

Evidence categories: `ignition_cause, ignition_time, fire_status, spread_direction,
evacuation, threatened_place, road_closure, resources, weather, fire_danger_warning,
event_gathering, people_present, shelter, infrastructure, other`.

### `frames/NN.json`

| Field | Meaning |
|---|---|
| `t`, `mode` | Frame time; `"historical"` |
| `weather` | ERA5 hour: `temp_c, rh_pct, wind_mps, wind_from_deg, gust_mps, wind_upper_mps, wind_upper_from_deg, cloud_pct, shortwave_wm2, precip_mm, vpd_kpa, point, distance_km` |
| `model` | `area_ha_p10/p50/p90, front_km, growth_ha_h, head_ros_m_min, head_intensity_kw_m, fire_type` |
| `predicted` | Modelled burned area so far, `p10/p50/p90` MultiPolygons |
| `forecast` | `h6`, `h12` envelopes (MultiPolygon) |
| `smoke[]`, `smoke_axis`, `plume`, `wind[]` | Surface PM2.5 contours by threshold, plume axis, plume rise/heat, wind arrows |
| `fogos_status` | Latest official status at `t` |
| `evidence_ids[]` | Evidence known at `t` (look up in `index.evidence`) |
| `exposures[]` | Places named in known facts: `id (geonames-…), name, kind[] (evacuated, threatened, shelter, people_present, event, road_closure), position, people, people_basis, population_geonames, distance_from_ignition_km, events[], evidence[], in_modelled_burn_p50, in_forecast_6h, in_forecast_12h` |
| `automation[]` | `{step, at, basis, detail, url, latency_s}`; `basis: "observed"` for Fogos.pt/news steps, `"simulated"` for EmberWatch's own actions |
| `drones_legacy` | Simulated drones from the old heuristic (bases, routes) |
| `fleet`, `sitreps`, `fleet_check` | `null` / `[]` until Agent 2 delivers WORKPLAN 5.2–5.5 |

## Limitations

- Pages were captured on 15 Nov 2026, so they are today's versions; later edits to an article cannot be ruled out. Rule 3 keeps reveal times conservative.
- Model frames are 3-hourly. Finer early frames (every 15–30 min in the first 3 h) are an Agent 2 task.
- Drones, dispatch and EmberWatch's automation steps are simulations, and are labelled `basis: "simulated"`.
