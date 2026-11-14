# Portugal data-source research

Checked **14 November 2026** against the primary sources linked below.
The original planning research is followed by the dated implementation probes
below. A bounded Context.dev public-page scrape has now been executed; it did
not establish complete incident extraction, ongoing reuse rights or production readiness.

## Agent B implementation probes — 14 November 2026

The `emberwatch-intelligence` Rust crate now has Context REST/public-page,
IPMA, bounded Portuguese report, and signed webhook modules. Tests use synthetic
literals only; no captured Fogos incident dataset is committed.

| Check | Observed result | Consequence |
|---|---|---|
| Context account | Authenticated usage read succeeded; available credits covered the bounded probes. Auto-topup was disabled. | Account allocation is observed, not a general free-tier entitlement or permission to spend the balance. The server API environment key was absent in this session. |
| [Fogos robots](https://fogos.pt/robots.txt) through Context | Request `13221348-cccc-4031-8540-9ffc6f426ebd` returned a rule disallowing `/gaia` and sitemap links. `/pt` was not disallowed by the returned policy. | Do not visit `/gaia`. Robots permission is distinct from a website reuse licence, which remains unresolved. This supersedes the earlier failed robots fetch below. |
| [Fogos public map](https://fogos.pt/pt) through Context | One fresh Markdown + nullable JSON capture, request `46d9e921-543f-4a27-aef6-1f171157e59f`, returned per-output success. The rendered text had headings and an aggregate counter but no incident rows. Structured output explicitly marked coverage partial. No explicit empty statement was present. | Zero extracted rows **does not mean zero incidents**. Extraction feasibility for populated public list/detail rows remains unverified. Public links to `/pt/lista` and `/pt/tabela` were discovered, but no additional Fogos capture was made. No API/network endpoint was queried. |
| Rust Context application boundary | A local HTTP contract test checks bearer auth, fresh captures, nullable schema, partial output and actionable 429 behavior. `CONTEXT_DEV_API_KEY` was absent. | The plugin probe is a real Context request; a live request through the Rust Context client has **not** been demonstrated. Run the crate's one-shot `probe -- fogos` example after configuring the server key. |
| [IPMA documentation](https://api.ipma.pt/) | Hourly observations use `-99.0` for missing values, wind in m/s or km/h, direction codes 1/9=N, 2=NE through 8=NW, 0=undefined. The documented naive timestamp examples do not explicitly establish a timezone. | No invented gusts. Unknown timezone stays unresolved; the adapter skips unresolvable timestamp groups. Explicit `UtcConfigured` is flagged as an operator assumption. IPMA attribution/linked-logo and usage-contact conditions still apply before deployment. |
| Live Rust IPMA probe | One `cargo +stable run -p emberwatch-intelligence --example probe -- ipma` call fetched the documented station and observation JSON successfully. Default unknown-timezone policy skipped 24 naive timestamp groups and returned partial health with no normalized weather records. | Transport/payload shape is exercised live. No precise timestamp normalization is claimed without a documented/explicit timezone policy. No contact email was sent. |
| [Context webhook guide](https://docs.context.dev/webhooks.md) | Verified current raw-body `t=<unix seconds>,v1=<hex>` HMAC-SHA256 protocol, full returned signing-secret string, five-minute attempt tolerance, stable event ID across retries, header/envelope matching, durable inbox before acknowledgement. | Rust verification and durable-inbox trait are implemented. A mounts the route and owns atomic deduplication. No monitor/webhook was created or changed. |
| [Change payload reference](https://docs.context.dev/api-reference/monitors/webhook-payload.md) and [run payload reference](https://docs.context.dev/api-reference/monitors/webhook-run-completed.md) | Both render an OpenAPI block containing `paths: {}` without the advertised concrete event payload schema. | Documentation gap: actual source ID JSON pointer and data schema need a representative signed delivery. The receiver requires an explicit trusted source binding and fails closed. Verified receipts are not automatically incident evidence. |
| [Copernicus bulletin 174](https://mapping.emergency.copernicus.eu/news/information-bulletin-174-copernicus-emergency-management-service-monitors-wildfires-in-portugal/) | Official bulletin is dated 1 October 2024. It describes EMSR760 activation on 16 September and first Sever do Vouga delineation delivery the following day, with products from 17–18 September. | The bulletin itself cannot be available in a 15–18 September replay. Product-level URLs/files, exact publication instants, licence/attribution checks, and archive coverage remain unverified. No product has been downloaded/imported. |

Current provider references were read for [scrape request/response](https://docs.context.dev/api-reference/web-scraping/scrape.md),
[web search](https://docs.context.dev/api-reference/web-scraping/search.md), and
[monitor events](https://docs.context.dev/monitors/webhooks.md). Report search is
bounded to five allowlisted Portuguese sites. ANEPC-derived articles retain the
`anepc` origin group; other unreviewed reports are not treated as independent
confirmations. Deterministic cited PT/EN text fallback is implemented; Gemini,
automatic geocoding, recurring monitors and voice are not claimed as integrated.
Fogos recurring extraction stays disabled until source checks pass. Its public
redistribution permission, full coverage and real record extraction remain open.

## Decision: Context.dev reads Fogos.pt public pages

The project will scrape `https://fogos.pt/pt` through Context.dev. It will not
consume the Fogos.pt API. The [public page](https://fogos.pt/pt) exposes headings
for location, start, incident type, resources, and status, but this research
reader returned no populated incident rows. This establishes that the page is
reachable, not that a complete extraction works. Its dynamic map/list needs a
rendered-page probe before it becomes a dependable source.

Fogos.pt describes its information as frequently updated, approximate-location
data based on ANEPC records in its [about page](https://fogos.pt/en/sobre).
No guaranteed refresh interval or end-to-end latency was found. Measure capture
time and any displayed update time; never advertise second-level freshness.

## Is it free, and can we use it?

| Question | Finding | Project consequence |
|---|---|---|
| Public website access | The page was readable without a payment/login prompt in this research fetch. | Viewing availability is established; a public-data reuse licence is not. |
| Website scraping/republication permission | No explicit licence authorizing our planned extraction and redistribution was established from the pages checked. Both apex and www `robots.txt` fetches failed in the research tool. | Mark reuse/robots checks unresolved; do not interpret either failure as permission or a prohibition. Recheck before recurring ingestion/public redistribution. |
| Context.dev cost | Its [current pricing page](https://www.context.dev/pricing) lists 1,000 free monthly credits, 2 concurrent requests, and 10 requests/minute. A normal scrape starts at 1 credit; successful JSON extraction adds 4. | A free probe is feasible with an account/key and available credits. Continuous coverage is budget-dependent. Confirm the dashboard entitlement; older search results contain different prices. |
| Fogos.pt API | The [API access notice](https://fogos.pt/en/api) describes formal authorization, an individual token, `X-API-Key`, and an identifiable User-Agent. No zero-cost API entitlement was established. | Excluded from the selected architecture. Scraping is not a substitute for permission to use restricted data. |

The [API terms](https://fogos.pt/en/api-termos) permit personal/academic and
noncommercial uses conditionally, prohibit economic exploitation including ads,
donations and resale, require visible credit, and reject sole-source critical
reliance. Those are API-specific findings, not a website scraping licence.
The [access form](https://source.fogos.pt/api/request-access) also requires a
declaration of no direct or indirect monetary advantage. Prize eligibility makes
our hackathon status uncertain; do not represent that declaration as satisfied.

Conclusion: **public viewing is available; a fully free, authorized scraping and
redistribution workflow is not yet verified**. Keep the chosen scraping design,
resolve the website conditions, and retain an independent replay path. No access
application, email, or acceptance of terms was submitted during this task.

## Fogos.pt extraction acceptance criteria

Owner: Agent B. Agent A owns scheduling/storage; Agent D owns visible provenance.

1. Review the current public-page conditions and crawler policy. Record the
   checked URL, date, result, and any restriction. Do not target API URLs through
   Context.dev, replay network requests, or work around access controls.
2. Probe one rendered public list/detail page through Context.dev. Compare
   returned Markdown/JSON against the visible page and an optional screenshot.
   Discover real detail URLs from page links; do not invent routes or selectors.
3. Extract an object containing `incidents[]`, page update time if displayed,
   capture metadata, and a coverage/completeness flag. Per record preserve a
   page-visible ID/link if available, locality, municipality, raw Portuguese
   status/type, displayed start/update time, and resource counts when present.
   All absent facts are nullable. Coordinates may be copied only if present;
   otherwise geocode locality separately with an uncertainty radius.
4. Validate Context.dev per-output `success`, partial/truncated results, and the
   returned schema. Detect the difference between an explicit empty list and
   an unloaded/failed page. Never claim national completeness after scraping
   only the viewport or a single paginated section.
5. Deduplicate by stable visible identifier/URL, else a documented composite
   of locality, type, and event time. A disappearing row does not close a fire
   unless source coverage and status justify that transition.
6. Proposed initial cadence: one shared scrape every **5 minutes**, only after
   source checks and the extraction probe pass. Bound pagination/detail visits,
   back off on failures, and cache centrally. This is our budget choice, not a
   documented Fogos.pt allowance or refresh guarantee. Do not run a duplicate
   monitor and poller against the same page.
7. Label outputs `Source: Fogos.pt` with a link and `via Context.dev`; preserve
   the ANEPC lineage when supported. Fogos.pt and articles repeating the same
   ANEPC report are not independent confirmations. Do not commit captured data
   as an openly licensed fixture without verifying its redistribution rights.

For current Context.dev behavior see the official
[scrape guide](https://docs.context.dev/scrape/overview.md),
[JSON extraction guide](https://docs.context.dev/scrape/json.md), and
[cache guide](https://docs.context.dev/scrape/freshness-and-caching.md).
Use `POST https://api.context.dev/v1/web/scrape` from Rust with bearer
`CONTEXT_DEV_API_KEY`. Request `formats.markdown` and `formats.json` with a
nullable-field `jsonParams.schema`. Set `maxAgeMs: 0` for a scheduled fresh
capture; serve the shared stored result between runs. Default provider caching
can otherwise be days old. Save `request_id` and cache/usage metadata.

At 5-minute intervals, one page over 44 hours means **528 requests**: about 528
base credits, or 2,640 with successful JSON extraction, before pagination,
actions, retries, monitors, news, or AI costs. The free 1,000-credit tier cannot
cover that full structured schedule. A 15-minute structured schedule is about
880 credits for one page and leaves little headroom. The old plan's 50,000
event credits/code `JUNCTION26` remain an unverified entitlement until redeemed
and visible in the account; do not budget as if already granted.

## Independent weather and historical data

| Source | Verified evidence | How we use it / remaining check |
|---|---|---|
| IPMA | [Official API documentation and conditions](https://api.ipma.pt/) publish stations, hourly observations, and municipal fire-risk forecasts. Free non-profit reuse requires source credit; the conditions also call for its linked logo in web pages. The API intro requests an email explaining usage. | Use the documented API for weather, rather than scraping a weather API's web presentation. No email has been sent. Record source terms and attribution before deployment. |
| Open-Meteo | [Terms](https://open-meteo.com/en/terms) allow the free API for noncommercial use below 10,000/day, 5,000/hour and 600/minute, with CC BY attribution. | Forecast grid, elevation, and archive weather. Batch/cache locations and monitor the budget; no guaranteed uptime. |
| NASA FIRMS | [Area API](https://firms.modaps.eosdis.nasa.gov/api/area/) documents bbox/date queries and a free MAP_KEY, with 5,000 transactions per 10-minute interval. | Independent satellite observations and replay; obtain a key and check product/date availability. Do not claim satellite delivery latency from polling cadence. |
| Copernicus EMS | [Official event report](https://mapping.emergency.copernicus.eu/news/wildfire-in-portugal/) places fires in Oliveira de Azeméis, Sever do Vouga and Albergaria-a-Velha from 15 September 2024. [Bulletin 174](https://mapping.emergency.copernicus.eu/news/information-bulletin-174-copernicus-emergency-management-service-monitors-wildfires-in-portugal/) identifies EMSR760 and mapping products. | Primary replay candidate: Aveiro / Sever do Vouga, 15–18 September 2024. Verify/download particular products and their attribution conditions before packaging; publication time must constrain replay visibility. |

IPMA endpoints from its documentation:

```text
https://api.ipma.pt/open-data/observation/meteorology/stations/stations.json
https://api.ipma.pt/open-data/observation/meteorology/stations/observations.json
https://api.ipma.pt/open-data/observation/meteorology/stations/obs-surface.geojson
https://api.ipma.pt/open-data/forecast/meteorology/rcm/rcm-d0.json
```

Observations update hourly; `-99.0` means missing. These endpoints do not justify
the old plan's ten-minute station/gust claim. Parse actual available fields and
direction codes; keep missing gusts null. In simulation, use a forecast gust or
an explicitly synthetic gust event when no measured gust is available.

Portuguese official/news source candidates: `prociv.gov.pt` (ANEPC), `icnf.pt`,
`agif.pt`, `ipma.pt`, `rtp.pt/noticias`, and `sicnoticias.pt`, plus local outlets.
Specific article/monitor URLs, source policies, historical coverage, and current
Context.dev search filter shapes remain implementation probes. Use Portuguese
queries such as `incêndio ativo`, `incêndio rural`, `evacuação`, and municipality
names. Social accounts and webcams are optional, not baseline dependencies.

## Rust references checked

[Axum SSE](https://docs.rs/axum/latest/axum/response/sse/) supports the streaming
response in the plan; [SQLx](https://docs.rs/sqlx/latest/sqlx/) supplies async SQL.
Pin the actual compatible Rust/toolchain/crate set at scaffold time. Call
Context.dev over documented REST with reqwest; do not assume an official Rust
SDK exists. Provider model IDs, mapping tiers, host entitlements, competition
prizes, and the original Context.dev event quota were not reverified here.
