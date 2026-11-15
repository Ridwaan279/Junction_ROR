# Rust-generated early replay

The delivery is `web/public/generated/arganil-early/index.json` and `frames/00.json`
through `frames/19.json`. Agent 1 can switch the fixture URL to this index.
The first three model hours have 30-minute spacing; later frames have 3-hour spacing.
Original fixtures and retained capture exports are preserved.

Rebuild from the repository root (no network calls or API credits):

```sh
cargo run -p emberwatch-firecast --example replay -- data/historical/arganil-piodao-2025/dataset.json .state/arganil-replay-rust.json
# Set EMBERWATCH_GOLDEN_RAW_DIR to the private verified-originals directory.
cargo run -p emberwatch-firecast --example build_early -- .state/arganil-replay-rust.json
```

The builder checks original request IDs and exact Markdown hashes, then
recomputes quality and conservative availability. Full article text stays private.
It reuses the frozen fixture index's display metadata, cited GeoNames matches and
official status timeline; it does not run Python. The retrospective captures
cannot prove that later edits were available during the 2025 incident. ERA5 is
retrospective reanalysis, not an operational forecast.

For a new billed capture, use Rust `probe_context` with an explicit `--ceiling`:
`capture-plan data/demo/arganil-early/capture/queries.json`, `batch urls.json`,
`scrape URL`, `fogos URL`, `search MUNICIPALITY`, or `news QUERY FROM TO`.
The latter uses web search with date operators; unverified Portuguese news-index
requests in the old capture plan are skipped. New captures cannot replace the
fingerprinted originals silently.

Stations use three real OSM city nodes. Depots, inventory, aircraft, transfer
speed and priority weights are hypothetical demonstration settings. Routing
uses real regional terrain, A* wind/climb pricing, Gaussian surface smoke,
return reserve and blocked unknown hazards. Ground legs are schematic rather
than road routes. `flight_duration_s` consumes aircraft endurance;
`ground_duration_s` adds only wall-clock mission time.

Exposure priority uses the ensemble's six-hour probability at the site, not
binary polygon membership. Unstated attendance stays null; explicit `kind_default`
weights are 40 for shelters and 20 for other exposure kinds. Cancelled events and
unsupported evidence cannot trigger exposure tasks. Eligible sites within 1 km
form one deployment cluster. Existing GeoNames coordinates retain attribution;
unresolved names stay unmapped.

Reports are `automation` events with `step: first_report` and `basis: simulated`.
They sample the simulated mission at the cursor, including ground transfer or
return status. They are never real sensor observations. Advisor fields remain
`sitreps: []` and `fleet_check: null`; no Gemini integration exists.

The user removed the human benchmark. Verified means deterministic source
grounding, not independently measured extraction accuracy.
