# Response simulation

`emberwatch-response` consumes the shared domain contracts and returns computed
outputs. It does not write to a database, register routes, contact a model/voice
provider, or dispatch a real vehicle. No environment variables are required.

The server should construct a shared `ComputePool` and call `pool.run(move ||
demo_bundle(&context, &incident, &evidence, &weather)).await`. Excess calls return
`ComputeError::Capacity` immediately. A permit stays inside `spawn_blocking`,
including when the caller disconnects. Each job is bounded by grid and step
limits; blocking jobs cannot be force-aborted once running.

`demo_bundle` accepts only `DataMode::Synthetic`. It builds a 21 × 21 grid with
500 m cells, an explicitly synthetic depot and obstacle, seed 760, and eight
60-second spread steps. Fresh supplied wind/humidity can be used; otherwise the
training fallback is labelled synthetic (4 m/s wind from west; 20% humidity).
Gust remains unknown when absent. The 12 m/s cruise, 120-second task, 3600-second
nominal battery and 20% reserve are demonstration configuration, not aircraft
specifications. Live/historical callers must supply their own validated inputs
to the lower-level functions instead of using the demo helper.

## Illustrative model

The grid uses a local east/north Cartesian plane in metres. Extent is at most
60 km in either direction and 40,000 cells. Fuel/humidity are in [0,1]. No model
has been calibrated against a Portuguese incident, and no published model or
empirical accuracy is claimed.

Each seeded step considers only the previous step's burning cells and their
four edge-adjacent neighbors. For a target cell:

```
wind_factor  = max(0, 1 + dot(wind_mps, neighbor_direction) / 5)
slope_factor = clamp(1 + elevation_difference_m / cell_size_m, 0.1, 4)
p = clamp(base_probability * fuel * (1-humidity) * wind_factor * slope_factor, 0, 1)
```

The divisor 5, slope bounds, and base probability are illustrative controls,
not scientific constants. Burning cells remain burning for this bounded toy
model; there is no combustion duration, ember spotting or physical spread-rate
calibration. Blocked/nonburnable cells never ignite. A fixed xorshift64 stream
and row-major traversal give reproducibility; seed zero maps to a fixed
nonzero stream. Output polygons show cell footprints and first-arrival times
for one realization, not measured perimeters, probabilities or confidence bands.

Geometry uses a spherical local tangent-plane approximation with explicit
Portugal origin and longitude/latitude output. Mainland bbox checks do not
constitute a coastline/border polygon. This approximation is adequate for an
illustration; measured perimeters and terrain clearance need better inputs and
projection handling.

## Routing, replay and text

A* uses eight neighbors, blocks diagonal corner cuts, and minimizes travel
seconds including wind-relative ground speed, climb and configured smoke cost.
Crosswind cancellation requires sufficient airspeed; an impossible directed
edge is rejected. Return travel is planned independently with reversed wind
effects. Outbound + task + return must leave the configured battery reserve.
Configured gust limits are enforced when gust input exists. No aviation,
airspace completeness or measured-gust safety assurance is implied.

Hard blocked cells and newly blocked diagonal edges bypass replan hysteresis.
The previous outbound and return paths are repriced with current wind, smoke,
climb, task and battery inputs. A previous path that no longer preserves reserve
also bypasses hysteresis. Soft travel-time improvements compare both routes
under the same current costs and need the caller's minimum improvement threshold.
Replan errors mean the mission is invalid; callers must not continue the old
route. Telemetry samples absolute run-clock time through idle, enroute, on-task,
return and charging. Position interpolation is simplified by route-distance
fraction, not a flight dynamics model. Recreate state when the replay epoch
changes; equal input clock times give equal telemetry.

Shared outputs retain run/incident/epoch and generated time from `RunContext`.
Evidence availability/publication/event times prevent future input leakage;
weather availability is also checked. A supplied forecast may have a future
validity time if already available. The server remains responsible for
persistence, deterministic input release, output invalidation and SSE delivery.

Portuguese/English text is a deterministic cited fallback with no optional voice
dependency. It quotes source status/evidence labels without promoting them to
official confirmation. `propose_severity` produces a separate heuristic review
priority using explicit caller-supplied wind/humidity/age thresholds; missing
complete fresh weather remains `Unknown`. It does not alter incident status.

## Verification

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p emberwatch-response --example measure_runtime
```

Use `+stable` locally if the exactly pinned toolchain is not installed. The
runtime example prints 30 synthetic bundle samples (median/p95/max) and one
120 × 120 grid / 60-step spread run, along with architecture, parallelism and
debug/release profile. These measurements are observations on the running
machine, not latency guarantees.
