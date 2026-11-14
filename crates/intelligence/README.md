# Intelligence/source adapter handoff

The Rust crate returns shared `emberwatch-domain` batches. Platform owns source
cadences/caches, retry policy, incident fusion, persistence, and SSE. No provider
key or source-run trigger belongs in the browser.

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
cargo +stable run -p emberwatch-intelligence --example probe -- fogos
```

Fixtures are synthetic test literals, not captured third-party incident records.
Fogos live extraction success/coverage and redistribution permission are separate;
see `docs/DATA_SOURCES.md` for the dated probe and open deployment requirements.
IPMA web use requires attribution and the linked IPMA logo under its conditions;
the adapter carries source attribution but does not make a legal approval claim.
