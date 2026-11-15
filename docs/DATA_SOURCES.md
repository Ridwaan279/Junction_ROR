# Portugal data-source research

## Agent 2 source-preservation and contract checkpoint — 15 November 2026

The user supplied the original `emberwatch-context-dev-raw-2026-11-15.zip`.
The ZIP and 35 extracted JSON responses are preserved privately outside Git.
All 25 retained page request IDs were checked when applicable; the 23 responses
with Markdown now have SHA-256 hashes in `pages.json`. Two failed captures have
no Markdown hash. Hashes use original decoded UTF-8 bytes, with no whitespace or
Unicode normalization. Archive fingerprint/counts are recorded in
`data/demo/arganil-early/context_dev/raw_archive_provenance.json`.
No full article text, authentication header or account allocation is committed.

Current contract review:

| Operation | Request / response used | Status and source |
|---|---|---|
| `POST /v1/web/scrape` | Bearer auth; URL, text/JSON formats, `maxAgeMs`, rendering wait, deadline, nullable schema. Read requested/success/data/error codes, `isPartial`, JSON truncation, request ID, cache age and consumed credits. | [Scrape](https://docs.context.dev/scrape/overview.md), [JSON](https://docs.context.dev/scrape/json.md), [errors](https://docs.context.dev/scrape/timeouts-and-errors.md). HTTP 200 is not sufficient; missing/partial output cannot imply zero incidents. |
| `POST /v1/web/search` | Query, result bound, domain/language/country filters; dated queries use discovery operators. Read result URL/title and optional per-result Markdown `code`/`finalDOMState`, cache and usage metadata. | [Search reference](https://docs.context.dev/api-reference/web-scraping/search.md). Search rank/date hints are not proof of publication or historical availability. |
| `POST /v1/news/search` | Historical raw calls remain preserved; Portuguese discovery in new code uses web search instead. | Separate index coverage is unverified for Portuguese publishers. The current reference was listed in the provider index but not retrievable in this research reader; do not claim a new documentation or live-contract verification. |
| `POST /v1/web/answers` | Explicit `fast`, task, example JSON shape and deadline. Read `json_content`, sources and partial state. | [Answers](https://docs.context.dev/answers/overview). Outputs lack field-level literal verification and cannot automatically modify physics inputs. |
| Monitor webhooks | Existing raw-body timestamped HMAC verifier retained; no monitor was created or changed. | Initial dated protocol review below remains the evidence. Concrete delivery schema still requires a representative signed payload. |

The [credit guide](https://docs.context.dev/account/credits.md) specifies one base
scrape credit, four more for successful JSON extraction, web search per ten
returned results (rounded up) plus requested content modifiers, and ten for a
successful fast answer. The [rate guide](https://docs.context.dev/optimization/rate-limits.md)
describes organization concurrency and the Free-plan ten requests/minute; account
allowances can differ. The client's configured ceiling is not an account quota.
Missing usage is unknown and halts subsequent billed requests in that budget.

The current Rust `probe_context` then made bounded live calls (15 November 2026,
03:35 UTC), using the already-configured private credential without exposing it:

| Endpoint | Request ID | Observed outcome |
|---|---|---|
| `web/search` (Arganil, Portuguese) | `089a9033-69ee-44ef-8e79-9027632a5adf` | Fresh response, 1 credit. |
| `web/scrape` (Fogos public list) | `f960c398-ac64-4062-910e-51a5e1f3c314` | Markdown/JSON succeeded; exact empty message, no rows, update time null, fresh response, 5 credits. The extractor claimed complete coverage; that claim alone is not proof of national completeness. |
| `web/answers` (fast, IPMA documentation) | `ad0af1a9-2012-49c9-8dc5-11377075b380` | Returned the documented missing-value sentinel with an IPMA URL and excerpt, 10 credits. This is a provider-contract probe, not verified physics input. |

Together these calls used 16 confirmed credits. Captures are redacted and
Git-ignored. No monitor or recurring source was enabled.

The reproducible original-capture audit checked all 23 Markdown hashes. Strict
NFC/whitespace matching supported 222/267 candidate news quotes; the quantity
parser supported 72/90 candidate numeric values. Formatting and unsupported
claims require review; these fractions are not independent field recall,
fabrication or numeric-error benchmark results. The human-labelled threshold
gate remains open. The original source records were unchanged apart from the
three added hash/provenance properties.

The live-status contract was read from Agent 1 commit `33bc89fb` and
`docs/UI.md`; its reference snapshot is copied to
`data/demo/live/fogos-list-check-2026-11-15.json` without changing `web/**`.
An explicit empty statement in a successful full public-page capture is labelled
as that source's statement at capture time, not independent proof of national
activity. Disabled, unconfigured, failed and partial checks never show all-clear.

Last setup/probe check: **15 November 2026**. Initial research: 14 November 2026.
Agent 2 plan checkpoint, 15 November 2026: branch `codex/agent-2-intelligence`
starts at integration commit `21e03ce`. The user's subsequent instruction removes
Gemini and authorizes the corresponding plan/README/demo-guide updates outside
the routine Agent 2 ownership list. No application or UI code changes in this
checkpoint. Validation: Markdown link targets checked and `git diff --check`
passed; Rust/web tests are not applicable to these documentation-only edits.
The original planning research is followed by the dated implementation probes
below. A bounded Context.dev public-page scrape has now been executed; it did
not establish complete incident extraction, ongoing reuse rights or production readiness.

## Portuguese news/language check — 15 November 2026

Portuguese page retrieval and AI interpretation were tested separately from the
earlier empty Fogos table. These are bounded feasibility checks, not a guarantee
for every Portuguese site or a licence to redistribute articles.

| Check | Observed result |
|---|---|
| Portuguese discovery | A Portuguese wildfire query with `languages: ["pt"]`, country PT and an RTP domain filter returned the matching Portuguese article. Request `334f8fdf-8cb7-4dd2-92b7-9a1537b23328`; 1 credit. |
| Fresh Portuguese article | [RTP's Portel report](https://www.rtp.pt/noticias/pais/cinco-meios-aereos-e-mais-de-300-bombeiros-combatem-fogo-em-portel_n1765736), displayed update 16 September 2026, was freshly captured at 01:30:31–01:30:42 UTC. Request `464b5b83-4039-4f08-8aed-73fa776f1e98`; Markdown and JSON succeeded, cache age 0 ms, 5 credits. Portuguese extraction instructions and a nullable schema were used. This is a historical article, not current fire activity. |
| Meaning and factual fields | Output preserved the accented Portuguese title, Portel, 367 firefighters, 129 vehicles, five aircraft, one active front, displayed update text and the raw Tuesday alert time. It correctly interpreted the article's negation as no populations at risk, left uncounted burned area null, and produced a faithful English summary. Values were compared with the publisher's article text. |
| Evidence formatting | The risk excerpt was a literal Markdown substring. The resource and unknown-area excerpts matched visible text after removing Markdown bold markers and normalizing whitespace, but were **not** exact raw Markdown substrings. JSON instructions alone do not guarantee verbatim evidence. Existing strict source validators must not silently accept rewritten/unsupported evidence. |

Official [Search reference](https://docs.context.dev/api-reference/web-scraping/search.md)
explicitly includes Portuguese (`pt`), alongside English, Spanish, French, German,
Italian and many other search language filters. That enum describes discovery,
not equal extraction accuracy across languages. No per-language accuracy guarantee
was found in the reviewed guides. The live check establishes European Portuguese
HTML/article support; it does not benchmark other languages, scanned Portuguese
PDFs, speech, every site layout, or populated Fogos records.

Documented limits remain: [JSON extraction](https://docs.context.dev/scrape/json.md)
uses AI over page Markdown/JSON-LD/meta tags; required non-nullable fields can
introduce guesses. It reads up to 100,000 Markdown characters plus 20,000 metadata
characters and can truncate longer pages. [Scrape errors](https://docs.context.dev/scrape/timeouts-and-errors.md)
include access walls, unsupported/oversized content and timeouts; HTTP 200 alone
does not establish successful or complete outputs. Dynamic pages may remain
incomplete. [Browser profiles](https://docs.context.dev/scrape/browser-profiles.md)
for authorized logins require a paid plan and do not support MFA/CAPTCHA/passkeys.
Keep the public-page-only configuration and stop on restrictions. Missing
coordinates, event timestamps, resource counts or source updates stay unknown;
language support cannot recover facts absent from the source.

## Context connection and fresh table probe — 15 November 2026

| Check | Observed result | Consequence |
|---|---|---|
| Codex hosted MCP | Registered `https://mcp.context.dev/mcp`; browser OAuth completed. A new Codex app-server connection reported `authStatus=oAuth` and discovered 50 tools, including `web-scrape` and `get-usage`, without discovery errors. No model turn or monitor was started. | Configuration and authenticated discovery work. This existing chat's tool catalog did not refresh; restart the MCP server/open a new chat to expose tools there. |
| Application key | Masked local entry saved a Windows DPAPI-encrypted key under ignored `.state`. Usage request `246426fe-f21d-4742-8c24-a7da080d9cb3` authenticated successfully, showed sufficient credits and auto-topup disabled. | The application key is usable; no key value is stored in Git, browser code, command arguments or chat. This is separate from Codex OAuth. |
| Public browser comparison | Both [table](https://fogos.pt/pt/tabela) and [list](https://fogos.pt/pt/lista) displayed `SEM REGISTO DE INCÊNDIOS` without incident rows. | A genuine page empty message was observed; it is not independent verification of national activity or source availability. |
| Fresh Context REST table capture | At 01:09:37–01:09:51 UTC, request `d8577776-adaa-485b-afe1-be0c807598d2` used Markdown + nullable JSON, `maxAgeMs: 0`, an 8-second render wait and 30-second return-partial deadline. Both outputs succeeded; cache age was 0 ms; cost was 5 credits. Markdown contained `Sem registo de incêndios`; JSON copied that exact empty text, returned `incidents: []`, null update time and `coverage: unknown`. The optional provider partial flag was absent. | Returned content matched the visible page. No incident locations, coordinates, resource counts or event timestamps were invented. Absence of the partial flag does not establish completeness. Local diagnostic output stays ignored, not a redistributed dataset. |
| Live Rust execution | At 01:15:18–01:15:30 UTC, the current compiled probe made fresh request `0f913442-69fd-44ba-aada-871574854da8` to the public table. Markdown/JSON succeeded, the empty text matched literally, coverage was unknown and cache age was 0 ms; cost was 5 credits. | The real Rust client and normalizer returned `Partial`, zero validated rows, `complete=false`, `explicit_empty=false`. The executable came from the successful Cargo test build and was checked against source modification times before execution. |
| Local validation | Formatting and workspace Clippy passed; 61 Rust tests passed, including the empty-label/unknown-coverage regression. One explicit PostgreSQL test was skipped. Ordinary `cargo run`/standalone build attempts hit Windows Application Control error 4551 on a `serde_core` build executable even outside the sandbox; Cargo test succeeded and produced the current runnable example. | `infra/context.ps1 -Action probe -UseBuilt` reuses a current executable and rejects stale source/configuration. No security policy was changed. Both successful live probes together used 10 credits; failed build attempts made no web requests. |

The public no-record label is recognized only alongside separately validated
coverage. Normalization still returns partial/unknown activity when coverage is
unknown. Populated incident-row extraction, complete national coverage and
website redistribution rights remain unverified. Recurring ingestion remains
disabled. No Fogos API request, paid browser action, billing change or monitor
was made. Setup follows the [MCP guide](https://docs.context.dev/install-mcp.md);
rendering follows [waiting and rendering](https://docs.context.dev/scrape/waiting-and-rendering.md).

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
