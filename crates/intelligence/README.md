# Intelligence/source adapter handoff

## Agent 2 foundation checkpoint

The firecast `ContextDev` facade now delegates HTTP/authentication/errors/credit
accounting to `ContextClient`. Portuguese incident discovery uses dated web search;
the separate news index is not used for Portuguese coverage. Historical article
conditions are withheld from live model adjustments. Live adjustments require
literal field evidence and a publication time within the configured day window;
negated positive claims are withheld. Unsupported fields remain in review metadata.

`accuracy` contains NFC/whitespace quote checks, Portuguese quantities and bounds,
Lisbon DST conversion, conservative availability times, per-unit resource
observations, unresolved ambiguous places and agency-copy checks. Formatting-only
quote differences remain unverified in this strict first pass.

Operator commands (server environment contains `CONTEXT_DEV_API_KEY`, including
proxy placeholders when the host injects authentication):

```sh
cargo run -p emberwatch-intelligence --example probe_context -- fogos https://fogos.pt/pt/lista --ceiling 5
cargo run -p emberwatch-intelligence --example probe_context -- search Arganil --ceiling 1
cargo run -p emberwatch-intelligence --example probe_context -- news 'incêndio Arganil' 2025-08-13 2025-08-14 --ceiling 1
cargo run -p emberwatch-intelligence --example probe_context -- scrape PUBLIC_HTTPS_ARTICLE_URL --ceiling 5
```

`batch urls.json --ceiling N` shares one bounded ledger across captures. A timeout
or missing usage stops subsequent calls; no billed POST is implicitly retried.
Only confirmed response credits are reported as usage. Raw responses are redacted
and saved under Git-ignored `tests/fixtures/live/` for private review.

The offline archive command needs no key or credits:

```sh
cargo run -p emberwatch-intelligence --example probe_context -- archive ORIGINAL_RAW_DIRECTORY ABSOLUTE_PRIVATE_DESTINATION data/demo/arganil-early/context_dev/pages.json
```

It refuses a Git destination, preserves responses without overwriting different
files, checks original request IDs, and hashes exact decoded Markdown UTF-8 bytes.
The user's supplied archive yielded 23 Markdown hashes and two failed captures.
The 25-page golden manifest is an inventory, not human gold labels. Set
`EMBERWATCH_GOLDEN_RAW_DIR` to the private preserved responses to run hash/quote
audits with `cargo test -p emberwatch-intelligence golden -- --nocapture`.
Independent recall, fabrication and all documented threshold metrics remain
pending human expected labels; no gate is claimed passed by an inventory test.

## Live status

The Rust server mounts `GET /api/live/status`, following the UI fixture's keys.
Each source adds `capture_succeeded`, `positively_empty`,
`last_successful_capture_at` and `error_code`. Unknown capture times are null,
including when configuration/source checks prevented any upstream request.
The state is `check_failed` or `partial` unless there is supported active evidence
or an explicit whole-line empty statement in a successful, non-partial capture.
Live status never reads historical or synthetic replay data.

The endpoint is single-flight and cached. It checks Markdown first (bounded at
one credit); visible incident links can trigger a separate structured capture
(bounded at five). It does not retry failed captures. Optional runtime variables:

- `EMBERWATCH_LIVE_ENABLED=1`, `FOGOS_PUBLIC_PAGE_CHECKS_PASSED=1`: explicitly enable
  network reads after source checks; both are off by default.
- `LIVE_STATUS_CACHE_SECONDS`: chosen cache duration, default 300 seconds.
- `CONTEXT_LIVE_HOURLY_CREDITS`: chosen per-process hourly ceiling, default 10;
  this is an application budget, not a provider quota. Exhaustion fails the check.
- `CONTEXT_DEV_API_KEY`: server/proxy authentication; never a browser variable.

Embedded/test routers stay offline by default. The executable opts into runtime
configuration through `router_with_live_options`.

The Rust crate returns shared `emberwatch-domain` batches. Platform owns source
cadences/caches, retry policy, incident fusion, persistence, and SSE. No provider
key or source-run trigger belongs in the browser.

Observation IDs identify immutable capture/revision records using the run, source,
stable upstream identity, original capture time and content hash. A later capture
or changed payload gets a new ID; retrying the same stored capture retains its ID
and timestamps. Callers reprocessing a stored capture must keep its original
capture time in `RunContext.clock`. `provenance.external_id` remains an actual
visible ID, URL or station/municipality ID. Fogos composite fallback identity is
separately labelled in `attributes.source_identity` and `source_identity_basis`;
platform can group revisions without inventing a visible upstream ID. When no
visible ID or event time exists, the fallback includes its excerpt to preserve
ambiguous same-place reports rather than force them into one incident.

- `ContextClient` calls documented Context REST (`/web/scrape`, `/web/search`)
  with deadlines, bounded bodies and error categories. It never targets a Fogos
  API. Billed POSTs have no implicit retry; an unknown timeout may have cost credits.
- `FogosSource` implements `Source`. Its default policy is disabled pending
  source checks. Normalization requires literal captured page support; complete
  coverage additionally requires explicit operator validation. A shell, failed
  output, missing rows or a disappeared record cannot close incidents. Last-good
  data/stale presentation is platform's responsibility.
- `IpmaSource` fetches documented station/observation endpoints. `-99.0` is null,
  wind is m/s, direction is meteorological **from**, and unavailable gusts are null.
  Naive upstream timestamps are skipped unless the caller explicitly selects
  `TimeBasis::UtcConfigured`; this assumption is visibly flagged. The probe uses
  `IPMA_NAIVE_TIMEZONE=UTC` only if deliberately configured. Daily fire risk uses a
  UTC calendar-day anchor, explicitly not an event/issuance time.
- `PortugueseNewsSource` is bounded to five allowlisted reports. ANEPC mentions
  share `anepc` lineage. Other reports remain `unreviewed_public_report`; these
  inputs cannot automatically become independent confirmations. Geocoding and
  publication/event times are deliberately unresolved. `cited_summary` supplies
  PT/EN text fallback with original Portuguese excerpts and source links. It is
  deterministic, with no invented AI model ID or live Gemini claim.
- `WebhookReceiver` verifies the documented `t=<seconds>,v1=<hex>` HMAC-SHA256
  protocol against **raw bytes** before parsing, enforces a five-minute tolerance,
  checks event headers and trusted source binding, and exposes `receive` against
  `WebhookInbox`. A must atomically persist with `UNIQUE(source_id,id)`, reject a
  changed body hash for a duplicate ID, and acknowledge only after success. The
  verified event remains a receipt, not automatic evidence. Concrete upstream ID
  JSON pointer/event data schema needs a representative delivery; current public
  payload references omit their schemas. Keep the receiver disabled until its
  binding is verified. Never select a secret using an unverified body/header.

Server variables: `CONTEXT_DEV_API_KEY`; `CONTEXT_WEBHOOK_SECRET` or an encrypted
per-source secret; optional explicit `IPMA_NAIVE_TIMEZONE=UTC`. A chooses receiver
binding variables. No `FOGOS_API_KEY` exists.

```powershell
cargo +stable test -p emberwatch-intelligence
cargo +stable clippy -p emberwatch-intelligence --all-targets -- -D warnings
cargo +stable run -p emberwatch-intelligence --example probe -- ipma
# After setting CONTEXT_DEV_API_KEY server-side; one bounded feasibility request:
cargo +stable run -p emberwatch-intelligence --example probe -- fogos table
# Alternative reviewed pages: fogos list / fogos map. Default: table.
```

Windows can use `infra/context.ps1` from the repository root for masked key setup,
encrypted account-local storage, usage checks, and the probe/server process. See
the root README. `-Action probe -UseBuilt` can use a current test-built executable
without rebuilding; stale/missing executables are rejected. Optional
`CONTEXT_PROBE_CAPTURE=1` saves public-page output and
request/usage metadata under ignored `.state/context-probe`; never commit those
captures as licensed fixtures. Each probe is a fresh billed request with an
8-second render wait, a 30-second provider deadline, and no automatic retry.

Fixtures are synthetic test literals, not captured third-party incident records.
Fogos live extraction success/coverage and redistribution permission are separate;
see `docs/DATA_SOURCES.md` for the dated probe and open deployment requirements.
IPMA web use requires attribution and the linked IPMA logo under its conditions;
the adapter carries source attribution but does not make a legal approval claim.
