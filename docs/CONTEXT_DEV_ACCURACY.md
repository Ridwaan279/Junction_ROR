# Context.dev accuracy plan (Agent 2)

### Delivery revision — 15 November 2026

The user explicitly removed the human benchmark from this delivery and asked
for implementation to finish quickly. Section 5 and the human-label acceptance
gate below are deferred, not passing. No independently measured recall/fabrication
rate or human accuracy certification is claimed. Deterministic quote, number,
time, provenance, ambiguity and failure checks remain.

Agent 2 now supplies the shared Context client, Rust capture-plan/batch/archive
tools, an offline demo builder, half-hour early frames, sourced hypothetical
stations, fleet sizing, regional routes, exposure priorities, simulated reports
and the live/status route. See `data/agent2/HANDOFF.md` for validation and limits.

### Foundation checkpoint — 15 November 2026

Implemented: shared transport used by firecast, bounded/redacted `probe_context`,
NFC/whitespace and Portuguese quantity/time/resource/place/lineage helpers,
stricter model-input grounding, capture-aware live status, and an offline private
archive importer. The supplied ZIP contains the original 25 scrape responses;
23 successful Markdown outputs now have exact-byte hashes in `pages.json`.
The two failures remain failures. Full articles stay outside Git.

Not claimed: independent human `expected.json` labels or benchmark acceptance.
The user subsequently removed this benchmark from scope. The hash/quote tests do not
prove field recall or zero fabrication. Some normalizer integration still needs
the full labelled cases. Stations, fleet, routing, exposure generation and finer
frames are now delivered separately. The optional second-model extraction
and Gemini advisor are removed from scope by the user's instruction.

The Windows host blocks some Cargo build scripts, the SQLx macro and selected
test executables with Application Control error 4551. Record actual per-target
results; do not turn a partial test run into a passing workspace report.

Goal: every fact EmberWatch shows from the web is pulled from the right page,
extracted correctly, traceable to a quote, and never invented. This document is
the work order for Agent 2. It is task 3 in [WORKPLAN.md](WORKPLAN.md); the
ownership and handoff rules there apply.

Ground rules (from AGENTS.md, restated because they decide many edge cases):

- Fogos.pt only through Context.dev scraping of public pages. No Fogos API client,
  key, undocumented endpoint or direct polling. Never visit `/gaia` (robots).
  Stop on any access restriction and record it.
- Scrape failure or a partial page is never evidence of zero incidents.
- Unknown values are `null`. Never infer dates, attendance, coordinates or counts.
- `CONTEXT_DEV_API_KEY` stays server-side and never appears in logs, fixtures or
  handoffs. Do not claim quotas or prices beyond what the provider's current page
  says on the date checked.

## 1. Current state (what exists today)

| Piece | Where | Status |
|---|---|---|
| Context REST client (scrape Fogos, search reports) | `crates/intelligence/src/context.rs` | Contract-tested against a local HTTP stub; a live Rust call has not been demonstrated (DATA_SOURCES.md) |
| Fogos normaliser with coverage | `crates/intelligence/src/fogos.rs` | Fixture tests |
| News normaliser, cited summary | `crates/intelligence/src/news.rs` | Fixture tests |
| Webhook HMAC verification | `crates/intelligence/src/webhook.rs` | Tested; payload schema still undocumented by provider |
| Second Context client for forecast conditions (news/web search, scrape, answers, credit budget, PT domain allowlist) | `crates/firecast/src/intel.rs` | Offline tests only; never run live |

Task 0: **one client**. Make `crates/firecast/src/intel.rs` use the
`emberwatch-intelligence` client and types so request shapes, error mapping,
redaction and usage accounting exist once. Keep firecast's credit budget and
"what-if/live/historical" modes as callers of that client.

### Findings from the first real capture (15 Nov 2026, Arganil 13–14 Aug 2025)

The early-stage demo ([DEMO_DATA.md](DEMO_DATA.md)) was captured live through
Context.dev: 9 searches and 25 scrapes for 129 credits. Build on these results:

| Finding | Consequence |
|---|---|
| `news/search` returned off-topic Portuguese results; `sourceCountry: "pt"` is rejected (accepted list has no PT); Portuguese publisher domains returned 0 | Use `web/search` + `web/scrape` for Portuguese news. Keep `news/search` for international outlets. Record the coverage gap as a monitor status, never as "no news" |
| `web/search` found the right regional and national pages, but returns no publication date | Take publication time from the scrape's `metadata.publishedTime`/`modifiedTime` and the printed date |
| `web/scrape` with markdown + JSON schema cost 5 credits per page; search cost 2 per 15–20 results | Budget numbers for section 7 |
| 2 of 24 pages `WEBSITE_BLOCKED` (correiodabeiraserra.com) | Failure state with the code; no retry loop |
| 251/267 extracted quotes verified verbatim; 16 not | Rule 1 is necessary; unverified facts stay out of automation |
| Printed time and metadata disagreed by 1 h on one site; a live-updated page (Público) carried facts from 9 h after its publication time; one page had a date but no time | Time rules in DEMO_DATA.md "Accuracy rules" §3; port them as tests |
| The extractor labelled villagers gathering in a square as an "event" | Category checks (an event needs a named event in the quote) |
| Fogos.pt incident page shows present-day meteo and resource counters next to the historical status table | Only the status table, coordinates and nature are historical; exclude the rest when replaying |
| The same Lusa story appeared on rr.pt, Sábado and Observador | Lineage grouping by headline works for agency copy |

## 2. Verify the provider contract (before writing more code)

For each endpoint we call (`web/scrape`, `web/search`, `news/search`,
`web/answers` if kept, monitors/webhooks), read the current official docs and
record in `docs/DATA_SOURCES.md`, with the date read:

- method, path, auth header, request fields we send, response fields we read;
- per-output `success`/`error_code`, partial (`isPartial`) and truncation semantics;
- cache controls (`maxAgeMs`) and what a cached response looks like;
- credit cost per call type as stated on the pricing/docs page (quote it; do not
  extrapolate), rate/concurrency limits as stated.

Any field we read that the docs do not show is a defect: either remove the read
or mark it "observed in probe <request id>, undocumented".

## 3. Live probe harness

Add `crates/intelligence/examples/probe_context.rs` (one-shot, operator-run,
never in CI):

```bash
CONTEXT_DEV_API_KEY=… cargo run -p emberwatch-intelligence --example probe_context -- fogos https://fogos.pt/pt
```

Subcommands: `fogos <url>`, `search <municipality>`, `news <query> <from> <to>`,
`scrape <url>`. Each run:

1. Prints request id, credits consumed (from response metadata), latency, cache
   age, per-output success and partial flag.
2. Writes the raw response to `crates/intelligence/tests/fixtures/live/<date>-<name>.json`
   after redaction (no key, no account metadata beyond credits consumed).
3. Refuses to run past a credit ceiling passed on the command line.

These captured responses become regression fixtures (section 5).

## 4. Extraction correctness rules (implement and test each)

1. **Quote verification.** Every extracted field carries the quote it came from.
   The normaliser checks the quote is a substring of the scraped Markdown (after
   whitespace/Unicode normalisation, NFC). Pass → `quality: "verified"`; fail →
   `"unverified"`, kept but never used for automation (fleet sizing, alerts).
2. **Value-in-quote check.** For numbers and dates the parsed value must be
   derivable from its quote (the number appears in the quote; the date words
   appear in the quote).
3. **Portuguese numbers.** `1.234,5 ha`, `1 234 ha`, `mil hectares`, `cerca de`,
   `mais de` (keep the qualifier as `approximate: true` / lower bound), ranges.
4. **Portuguese times.** Local `Europe/Lisbon` times → UTC with DST; relative
   words (`ontem`, `esta madrugada`, `ao final da tarde`) resolved against the
   article's publication time only when the publication time is known; otherwise
   `event_time: null`. Distinguish event time from publication/capture time.
5. **Operational resources.** `operacionais`, `meios terrestres`, `meios aéreos`
   extracted as separate fields with their own quotes and times; they change
   during the day, so each value is a time-stamped observation, not a property.
6. **Places.** Municipality/parish/locality names normalised against an OSM
   gazetteer for mainland Portugal; ambiguous names (same parish name in several
   municipalities) stay unresolved unless the page names the municipality.
7. **Lineage and independence.** Agency copy (Lusa) republished by several
   outlets counts as one independent source. Detect by shared byline/agency
   marker and near-duplicate text; keep all URLs, count independence once.
8. **Coverage honesty.** Fogos pages: a loading shell, an aggregate counter
   without rows, or a partial capture yields `coverage: partial|unknown`, never
   an empty incident list. Test each case.
9. **Failure mapping.** 401/403 (auth), 402/insufficient credits, 429 (rate),
   5xx, timeout (outcome and credits unknown) → distinct error codes surfaced in
   the UI source-status card. No implicit retry of billed POSTs.
10. **Freshness.** Use `maxAgeMs: 0` for live incident pages; record cache age
    when a cached answer is accepted for background enrichment.
11. **High-impact review.** For area burned, evacuations and people at risk,
    verify the quote, number, date and locality against retained source evidence
    and human-reviewed golden labels. Conflicting or unsupported facts stay
    `unverified` and cannot drive automation. No second model provider is used.

## 5. Golden set and metrics

Build `crates/intelligence/tests/fixtures/golden/`: 25–40 hand-labelled pages,
each a captured Markdown/JSON response plus an `expected.json` written by a person
reading the page, not by the extractor. Sources:

- the Arganil/Piódão 2025 articles already cited in
  `crates/firecast/src/historical.rs` (`arganil_2025_events`) and
  `data/historical/arganil-piodao-2025/dataset.json`;
- captured Fogos.pt public list/detail pages (if extraction of rows proves
  feasible; if not, record that);
- IPMA warnings and ANEPC/Prociv notices;
- for people-at-risk (WORKPLAN 5.4): event listings (festas, feiras, romarias)
  and campsite pages in affected municipalities, including pages that do not
  state attendance (expected `people: null`).

Include negative cases: a page about a different fire with the same place name,
an old article, a page with no numbers, a paywalled/partial capture, an English
syndication.

Metrics computed by a test (`cargo test -p emberwatch-intelligence golden`):

| Metric | Threshold |
|---|---|
| Fabrication: fields present in output but absent from page | 0 |
| Quote verification rate on fields the page states | ≥ 95 % |
| Field recall on stated fields | ≥ 85 % |
| Dates/times exact (to the minute stated) | 100 % of stated |
| Area/resources numeric error | ≤ 1 % (parsing should be exact) |
| Place resolution correct (when resolvable) | ≥ 90 %; ambiguous → unresolved, never wrong |
| Duplicate-lineage detection on syndicated pairs | 100 % of labelled pairs |
| Partial/failed capture reported as zero incidents | 0 |

Report the metrics table in the Gate 3 handoff. If a threshold is missed, list
the failing pages and the reason; do not lower the threshold silently.

## 6. People at risk and nearby events

Search: Context.dev web/news search in Portuguese, restricted to municipal,
parish, tourism and regional-press domains (keep an allowlist file in `data/`),
date window = forecast window ± 1 day, municipalities = those intersecting the
6-h forecast envelope. Extract with a schema: `name, kind, place_text, start,
end, people, people_quote, url`. Fixed sites (schools, care homes, hospitals,
campsites) from OSM Overpass, cited as OSM with the query.

Rules: attendance only when stated (`people_basis: "stated"`); otherwise a kind
default from a documented table (`"kind_default"`) shown as an assumption.
Events outside the window or in a different municipality are dropped with a
reason in the debug output, not silently.

## 7. Credit budgeting

- Per incident per hour budget (configurable, default small), enforced in the
  shared client; the source-status card shows credits used and remaining budget.
- Cache by URL + capture window; never re-scrape the same page inside the
  window unless it is a live Fogos page.
- Replay data is generated once offline and committed as JSON, so the demo spends
  no credits.

## 8. Deliverables and handoff

1. Single Context client (task 0) with tests passing.
2. DATA_SOURCES.md provider-contract table refreshed with dates.
3. `probe_context` example and at least one redacted live capture per endpoint
   (requires the key; if absent, say so and stop at fixtures).
4. Rules 1–10 implemented with a failing-case test each.
5. Golden set + metrics test + metrics table.
6. Exposure extraction feeding `ExposureSite` (WORKPLAN 7.2) for the Arganil replay.
7. Handoff: commit, scope check output, `cargo fmt/clippy/test` results, the
   metrics table, env var names used (`CONTEXT_DEV_API_KEY` and any optional
   source-provider keys). No Gemini key or model configuration is required.
