# EmberWatch agent instructions

## Read first

Read this file, [docs/PLAN.md](docs/PLAN.md), and
[docs/DATA_SOURCES.md](docs/DATA_SOURCES.md) before implementing a task.
The repository currently contains planning documents, not a working application.
Paths below describe the intended scaffold; do not report them as implemented.

## Product decisions

- Focus on **mainland Portugal**, bbox `[-9.6, 36.9, -6.1, 42.2]` in
  `[west, south, east, north]` order. Display Portuguese and English; store UTC and
  display Portuguese local time with `Europe/Lisbon`.
- Use **Rust** for the service, ingestion, replay, fusion, spread simulation, and
  drone planning: Axum, Tokio, Serde, reqwest, SQLx, PostgreSQL/PostGIS, and optional
  TimescaleDB. Use Next.js/TypeScript, MapLibre, and deck.gl for the browser UI.
- Obtain Fogos.pt page content through **Context.dev web scraping**. Do not add a
  Fogos.pt API client, key, undocumented endpoint fallback, or direct API polling.
- Keep extraction limited to public page content. Respect applicable site rules;
  stop on access restrictions. Public visibility does not establish reuse rights.
  Record outstanding source checks in `docs/DATA_SOURCES.md` without claiming that
  scraping or public redistribution has already been approved.
- Every observation carries its source URL, origin/lineage, event time when
  known, capture time, and extraction quality. Unknown values remain null.
- Distinguish live, historical, and synthetic data. Drones and dispatch are
  simulations. Keep source attribution visible alongside derived information.

## Work split

Follow PLAN section 12. When parallel implementation is requested, use four
workstreams: the coordinator implements platform/integration while three workers
implement intelligence, response, and UI. This fits four active agent slots.
This planning task itself does not launch implementation agents.

| Owner | Branch | Exclusive write scope once scaffolded |
|---|---|---|
| A: platform / coordinator | `codex/core` | Root build/config files; `contracts/**`; `crates/domain/**`; `crates/platform/**`; `crates/server/**`; `migrations/**`; `infra/**`; integration tests; this file; `docs/PLAN.md` |
| B: intelligence / sources | `codex/intelligence` | `crates/intelligence/**`; its tests and local fixtures; `docs/DATA_SOURCES.md` |
| C: forecast / response | `codex/response` | `crates/response/**`; its tests and local fixtures |
| D: terrain UI | `codex/ui` | `web/**`, including its manifest, lockfile, and tests |

Use separate worktrees for concurrent branches; never switch the shared checkout
under another agent. If isolated worktrees are unavailable, keep disjoint file
ownership and let only the coordinator perform Git integration. Do not revert,
overwrite, or move another agent's changes.

## Contracts and handoffs

1. A publishes the hour-1 baseline: domain types, OpenAPI, JSON Schemas, SSE
   envelope, Source trait, Clock, and fixtures. All workers acknowledge it before
   integrating; fixtures unblock development while external sources are checked.
2. B implements source adapters and returns normalized evidence/weather batches.
   A owns incident identity, fusion, persistence, scheduling, and the source
   registry. B supplies a webhook handler module; A mounts it in the server.
3. C consumes domain inputs and returns forecast/route/briefing outputs. A owns
   database writes and route registration. C keeps CPU work off async executors.
4. D consumes JSON/OpenAPI and fixtures, never Rust internals. A owns the SSE
   server; D owns reconnection, resync, attribution, and freshness presentation.
5. Request shared-contract or root Cargo dependency changes from A with the
   proposed shape, affected consumers, and fixture changes. Do not edit those
   files concurrently. A updates the workspace manifest and `Cargo.lock`.
6. Handoffs include commit/reference, owned files, behavior delivered, validation
   commands/results, unresolved issues, and required environment variable names.
   Never include secret values. Integrate at hours 8, 16, 26, and 34, earlier for
   small compatible changes. Rebase on the integrated baseline after each gate.

## Implementation and verification

- Freeze Rust toolchain/crate versions during scaffolding; keep secrets server
  side. Context.dev uses `CONTEXT_DEV_API_KEY` and Rust HTTP calls to its REST API.
- Use Serde domain types and boundary validation. GeoJSON coordinates are
  `[longitude, latitude]`; do distance/grid calculations in metres.
- Deduplicate source runs and webhooks; preserve timestamps across retries.
  Scrape failure or a partial page is never evidence of zero active incidents.
- Scope all persisted results and SSE events to a live/scenario run; a seek
  rebuilds deterministically without future observations leaking into the past.
- Add meaningful tests for extraction/failure cases, provenance, replay, fusion,
  and route/model invariants as those features are built. Documentation-only
  changes need link/consistency/diff checks, not application tests.
- Once code exists, run `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo test --workspace`; also run the web manifest's lint/typecheck/build
  scripts for UI changes. Report checks that could not run and why.
- Keep the scope ordered: Portugal map + cited incident + replay, then web
  enrichment, then spread/drone/voice simulation. Stretch tracks wait until the
  complete demo works. Do not invent model IDs, quotas, incident counts, or
  historical event timestamps.
