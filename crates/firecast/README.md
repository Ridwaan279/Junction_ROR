# Fire spread and smoke forecast (`emberwatch-firecast`)

Physics-based prediction of where a detected fire will spread and where its smoke
will go, built from live web data. Experimental research software: uncalibrated for
Portugal and never operational guidance.

## Why physics, not an LLM

The geometry comes from published fire-behaviour physics solved over real terrain.
A language model cannot integrate wind, slope and fuel cell by cell, is not
reproducible, and cannot be checked against data. Language models (Context.dev's
JSON extraction and Answers) are used where they are strong: reading Portuguese
reports to supply facts the physics needs, each with its source URL.

## Pipeline

1. `environment::acquire` gathers inputs, recording provenance and falling back to
   explicitly labelled synthetic values:
   - **Terrain**: Terrarium elevation tiles (zoom 11, about 58 m), else the Open-Meteo
     elevation API, resampled to 120 m; slope and aspect by Horn's method.
   - **Land cover**: OpenStreetMap polygons via Overpass, rasterised to fuel classes;
     unmapped cells take the regional vegetation reported on the web.
   - **Weather**: Open-Meteo hourly forecast at 9 points, 8 km apart. It includes
     10 m and 120 m wind, gusts, temperature, humidity, cloud, shortwave radiation,
     rain and vapour-pressure deficit.
   - **Context.dev** (`intel.rs`, credit-capped): `news/search` (dated, also works for
     replay), `web/search` (fresh official and local pages), `web/scrape` with a JSON
     schema (reported spread direction, wind, vegetation, barriers, evacuations, each
     with a verbatim excerpt), and `web/answers` (drought class, dominant vegetation,
     local wind regime, burn scars).
   - **IPMA** weather warnings for the nearest district.
2. `forecast::compute` runs the physics and publishes GeoJSON layers plus a summary.
3. `hazards::plan_drone` turns the forecast into planner hazards and plans a mission.

## Science

| Component | Method |
|---|---|
| Surface spread | Rothermel (1972) with Albini (1976) size classes, per Andrews (2018, RMRS-GTR-371). |
| Fuels | Anderson 13 models mapped to Portuguese vegetation (approximations). |
| Midflame wind | 10 m to 20 ft (÷1.15), then the unsheltered wind adjustment factor (Andrews 2012). |
| Wind and slope | Rothermel vector combination and effective wind speed, with the wind limit applied. |
| Fire shape | Anderson (1983) length-to-breadth ratio with the FARSITE −0.397 term; elliptical rates from the focus. |
| Crown fire | Van Wagner (1977) initiation; Rothermel (1991) crown rate 3.34·R10; Scott & Reinhardt (2001) crown fraction. |
| Growth | Minimum travel time (Finney 2002) by Dijkstra with a 16-neighbour stencil, hourly time-varying inputs. |
| Uncertainty | 32-member ensemble perturbing wind speed and direction, dead and live moisture, and sub-cell ignition. Outputs are burn probabilities (10/50/90 %). |
| Fuel moisture | Simard (1968) EMC; 10-h = 1-h + 1 %, 100-h = 1-h + 2 % (Rothermel 1983); rain, VPD and reported drought adjustments. |
| Intensity | Byram fireline intensity and flame length; Thomas (1963) for crown flames; suppression class. |
| Smoke | Steady Gaussian plume. Pasquill–Turner stability from wind and insolation (solar position). Briggs (1973) dispersion and Briggs (1975) plume rise from the modelled heat release. Vertical line source, mixing-layer trapping, PM2.5 emission factor 17 g/kg. |

Web facts change inputs only through deterministic rules in `derive_adjustments`.
Reported vegetation sets the regional fuel, and a reported drought class lowers live
moisture. The reported spread direction is never forced into the model. It is
compared with the model's run direction and shown as an agreement check.

## Historical replay (`historical.rs`)

When no fire is burning, a sourced historical dataset stands in for the live feeds:
dated GWIS perimeters (via GDACS), hourly ERA5 weather, a terrain snapshot and dated
web reports. See `data/historical/README.md`.

The replay forecasts from the ignition point to the first observed perimeter, then
restarts from every observed perimeter and forecasts the next one. Each forecast is
scored with the Sørensen–Dice coefficient on total and on newly burned area, plus the
share of observed growth inside the 10 % probability band and the growth direction.
Every 3 hours it also records smoke, conditions, and a drone fleet sized from the
active front and growth rate. The fleet is routed from bases 3 km outside the fire in
the cleanest air, avoiding likely fire within the hour and slowing down in smoke.

```sh
cargo run --release -p emberwatch-firecast --example build_dataset
cargo run --release -p emberwatch-firecast --example replay
```

## Running

```sh
cargo test -p emberwatch-firecast
cargo run --release -p emberwatch-firecast --example demo
cargo run --release -p emberwatch-firecast --example probe -- -8.367 40.735 "Sever do Vouga"
cargo run --release -p emberwatch-firecast --example fixture > web/public/firecast-fixture.json
```

The `demo` example needs no network or keys. The `probe` example calls the real
services and prints every input's status. The `fixture` example regenerates the
offline UI fixture from fully synthetic inputs.

Server settings: `CONTEXT_DEV_API_KEY` (optional), `FIRECAST_AUTO` (default on),
`FIRECAST_NETWORK` (default on), `FIRECAST_WEATHER=synthetic` to force synthetic
weather, `FIRECAST_CONTEXT_MAX_CREDITS` (default 25 per incident), and optional
`OPEN_METEO_URL`, `TERRAIN_TILES_URL`, `OVERPASS_URL` (`off` disables Overpass).

## Known limitations

- No calibration against Portuguese fires; fuel and canopy values are typical, not measured.
- Wind is interpolated without terrain channelling; ember spotting and suppression are not modelled.
- Spread is conditional on the reported point; location uncertainty is drawn separately.
- Smoke concentrations are order-of-magnitude, with default mixing heights.
- A what-if run applies today's real weather to a synthetic training incident and is labelled as such.
