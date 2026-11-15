# EmberWatch ROS 2 / Gazebo / PX4 simulated drone

One reported fire → one **simulated** drone mission in a small Gazebo scene.
No real aircraft, no real dispatch, no fire physics, no geographic reconstruction,
no collision avoidance, no suppression claims.

## Status (updated 2026-11-15)

| Part | State |
|---|---|
| Report → mission adapter (`plan_mission`, `mission.py`) | **Works**, unit-tested. Reads the contract `Snapshot` (file or `GET /api/snapshot`). |
| Input data used in the demo | **Synthetic** `contracts/fixtures/snapshot.json` (invented training report, `mode: synthetic`, `example.org` source URL). No real/historical report is checked in. |
| Wind / smoke / event time | **Unknown → null**. Wind copied only from an observed (non-forecast) `weather` item, with station distance; smoke is never set. |
| Gazebo scene + fire/smoke particles | **Representative visual only** (pattern from gz-sim `particle_emitter.sdf`). |
| Drone in Gazebo demo | **Kinematic stand-in** driven by gz `VelocityControl` — NOT PX4, NOT x500 dynamics. |
| Water drop on arrival | **Simulated visual** particle effect + status message. Fire is not changed. |
| PX4 SITL v1.17.0 + stock `gz_x500` | **Runs** (built in `~/px4`, outside the repo). Stock `commander takeoff` → 2.5 m hover → land verified. |
| PX4 ⇄ ROS 2 (uXRCE-DDS agent v2.4.3, `px4_msgs` `release/1.17`) | **Works**: 65 `/fmu/*` topics; versioned names `/fmu/out/vehicle_local_position_v1`, `..._status_v1` confirmed. |
| `px4_offboard_mission` on SITL x500 | **Run end-to-end** (2026-11-14): pre-stream → Offboard+arm → 10 m → local target NED (20, 30) → simulated drop (status only) → PX4 RTL → landed/disarmed. |
| Offboard-loss failsafe | **Tested**: node SIGKILLed during TRANSIT → PX4 `Failsafe activated` → RTL → landed/disarmed. |
| **PX4 fire demo** (`scripts/run_px4_fire_demo.sh`) | **Run end-to-end 2026-11-15** (4 runs, see below): report → scenario → hazard-aware route → x500 flies detour → hover → simulated drop → fire model → return route → land. |
| Fire/smoke/water/hazard visuals in the PX4 world | **Seen in GUI screenshots**: flames, wind-driven smoke, red hazard volume, green route markers, blue water spray; flames/smoke stop after the model reports extinguished. |
| Rust EmberWatch server as input (`--api`) | **Works** (2026-11-15): Rust 1.100.0 via rustup (`~/.cargo`, shell rc untouched); `cargo run -p emberwatch-server` (in-memory) → `plan_scenario --api http://127.0.0.1:8080` carries source URL / capture time / null event time; unknown `run_id` → 404 → rejected. Server data is still the synthetic scenario. |
| Real weather (IPMA) | **Available, not wired**: team probe got 3,545 hourly station records (only with the flagged `IPMA_NAIVE_TIMEZONE=UTC` assumption; 0 without). The server snapshot has no weather yet; adapter now picks the nearest station ≤ 30 km. |
| Real fire reports (Fogos via Context.dev) | **Blocked**: no `CONTEXT_DEV_API_KEY`; recurring scraping disabled pending source checks. |

## Arganil demo: seeded fire in a geographically grounded world

```sh
scripts/fetch_arganil_data.sh                       # ~4 MB open data -> ~/px4/geodata (outside repo)
colcon build --symlink-install && source install/setup.bash
ros2 run emberwatch_drone gen_arganil_world --out generated/arganil        # ~45 s, ~120 MB, git-ignored
cd .. && ros2/scripts/run_px4_fire_demo.sh --world arganil --seed 11 --demo-wind 270 5   # RECORDING DEMO
#   --distance 800 (default)   --headless --exit-when-done   --force-blocked (no-path test)
#   --snapshot FILE  (use an existing Snapshot instead of the generated one)
#   gen_arganil_world --no-satellite   simpler fallback world (OSM-painted texture, fewer trees)
```

Data, licences and every approximation: [ARGANIL_DATA.md](ARGANIL_DATA.md). In short:
- **Terrain:** Mapterhorn tiles (DGT LiDAR DEM, CC BY 4.0).
- **Ground texture:** Sentinel-2 cloudless 2016 by EOX (CC BY 4.0), the same imagery as the web app.
- **Forest:** placed where that image shows canopy; the trees are representative, not surveyed.
- **Map features:** OSM buildings and roads (© OpenStreetMap contributors, ODbL).
- **Fire spread:** 12 seeded points that ignite over time downwind. This is illustrative, not a
  spread prediction.

**What happens (seed → event → route → flight):**
1. `plan_arganil_scenario --seed N` picks a fire point next to a random OSM building within
   700 m of Arganil's centre, outside every building and tree footprint plus margin.
2. It writes a **SYNTHETIC `Snapshot`** (`synthetic_snapshot.json`) that is validated against
   `contracts/snapshot.schema.json`. It has `mode: synthetic`, `event_at: null` and an
   `example.org` source URL. The mission's fire position is re-derived from that snapshot
   through the shared adapter.
   - The Rust backend serves a snapshot compiled in with `include_str!`; there is no endpoint
     for adding events. Serving a generated snapshot from the server would need a small change
     by the platform owner (A), so the teammates' data contract is untouched.
3. **Start point:** the drone starts `--distance` m from the fire (default 800 m) on a seeded
   bearing. The point must be in the map, clear of obstacles, and on terrain flat enough for
   takeoff. If that fails at the requested distance, the distance shrinks and the scenario is
   labelled `distance_fallback`.
4. **Planned avoidance (`arganil/gridplan.py`):** cells are blocked where
   obstacle top + 6 m > flight altitude, or where the simplified smoke hazard lies. Blocked
   cells are dilated by 4 m. The route has three parts:
   - **cruise level** (10 m grid, z = max(start, fire ground) + 45 m) from the start to an
     entry point;
   - a **vertical descent** at the entry point;
   - a **low level** (5 m grid, fire ground + 16 m) among the buildings and trees to a
     **safe suppression waypoint** upwind of the fire, within the 10 m drop range.

   If any stage has no path, the result is `NO_PATH` and no flight takes place.
   `route_info` reports what blocked the straight lines.
5. **Collisions and measurement:** the meshes have **physics colliders** (Gazebo), but those
   are not avoidance. Avoidance comes only from the planner. The mission **measures** the
   flown track against the obstacle raster (`obstacle_entered`,
   `min_obstacle_vertical_clearance_m`) and against the hazard (`hazard_entered`).
6. **Spread and suppression:** the fire spreads to seeded points while the drone is en route.
   On arrival the drone hovers at the safe waypoint facing the fire, and the simulated water drop
   repeats (6 sim-s each, up to 10 drops). Each second of water is divided by √(burning points),
   so a larger fire needs more drops. The drops continue until the illustrative model reports
   EXTINGUISHED, read through simulated state access. All flames then stop, the hazard visual is
   removed, and the drone flies the route back and lands on its simulated launch pad.

**Verified Arganil runs (2026-11-15):**

| Run | Result |
|---|---|
| seed 42, 3 km, headless and GUI (before the forest/satellite update) | 3,000 m start, flew 2,988 m (measured). Detours past trees at cruise level and buildings at low level. Hazard and obstacles never entered. 2 drops, EXTINGUISHED, landed. Real-time factor 1.0. |
| seed 7, 800 m, headless, spread | 12 points ignited. 6 drops (0.70 → … → 0.16 → out), landed. Hazard and obstacles never entered. 272 s. |
| seed 7, 800 m, GUI, 18,500 trees + satellite | Same sequence, real-time factor **1.0**. After regenerating the forest the fire moved, so the straight lines were clear and the summary reports `avoidance_executed=False`. |
| `--force-blocked` | NO_PATH, exit 3, PX4 not launched. |
| generated snapshot fed back via `--snapshot` | same fire re-derived through the shared adapter. A schema-invalid snapshot is rejected (exit 2). |
| seed 7 with unknown wind | NO_PATH: no hover point outside the enlarged conservative circle (honest refusal). |
| first PX4 attempt on terrain | x500 tipped on a slope ("Attitude failure (roll)") and arming was refused. Fixed with the level launch pad. |



## Backend for real data input

```sh
export PATH="$HOME/.cargo/bin:$PATH"          # rustup was installed with --no-modify-path
CARGO_PROFILE_DEV_DEBUG=0 cargo run -p emberwatch-server     # http://127.0.0.1:8080, in memory
ros2/scripts/run_px4_fire_demo.sh --api http://127.0.0.1:8080 --run-id synthetic-portugal --demo-wind 270 5
```
(`CARGO_PROFILE_DEV_DEBUG=0` keeps `target/` ~0.6 GB; disk is tight.)

## PX4 fire demo: report → wind/smoke → hazard route → hover → simulated drop

```sh
cd <repo>   # needs ~/px4 setup below and `colcon build` in ros2/
ros2/scripts/run_px4_fire_demo.sh --demo-wind 270 5        # GUI; Ctrl-C stops everything
ros2/scripts/run_px4_fire_demo.sh                          # wind unknown (the fixture has none)
ros2/scripts/run_px4_fire_demo.sh --api http://127.0.0.1:8080 --run-id synthetic-portugal
#   options: --launch-from-fire EAST_M NORTH_M  --headless  --exit-when-done
```
Each run writes `ros2/run_logs/px4_fire_<time>/` (`scenario.json`, `summary.json`, PX4/gz/node
logs). Exit codes: 0 done, 2 event rejected (nothing launched), 3 no path (nothing launched).

**Pipeline** (`plan_scenario` → Gazebo world `emberwatch_px4` → uXRCE agent → PX4 SITL in
`PX4_GZ_STANDALONE` mode → `fire_sim` → `px4_fire_mission`):

* **Event → local frame.** Origin (ENU 0,0,0) = *simulated launch site* placed
  `--launch-from-fire` (default 35 m E, 12 m N) from the reported fire; it is PX4 home
  (`PX4_HOME_LAT/LON/ALT`, which PX4 also writes to Gazebo's spherical coordinates). The fire's
  position is the report lat/lon projected about that origin (PX4 projection). Rejected (exit 2,
  nothing launched): no incidents, no cited source, position outside the Portugal bbox (catches
  swapped lat/lon), fire > 150 m from origin, unreachable API. Terrain: flat, elevation 0.
  In flight, scenario origin vs PX4 EKF origin was measured at ~2 cm.
* **Wind** (`wind.from_deg` meteorological FROM, clockwise from true N; `speed_mps` m/s):
  `feed` (observed weather item) > `simulated_demo_value` (`--demo-wind`, labelled) > `unknown`.
  Drives only the smoke visual and hazard geometry, **not** drone dynamics.
* **Smoke** = gz particle emitter tilted downwind (speed = wind, rise 1.2 m/s), lifetime set so it
  drifts ≈ the plume length. Visual only; not sensing, not collision.
* **Hazard** (simplified, full-height column): fire core circle (r = max modelled fire radius)
  + downwind trapezoid (25 m, 15° spread); wind unknown → conservative circle (r + 25 m).
  Planner: 2-D A* (0.5 m grid, 2.5 m margin) + line-of-sight shortening to a *safe point*
  upwind of the fire (or outside the conservative circle). No path / start or goal in hazard →
  NO_PATH, never a straight-line fallback. The drone avoids the hazard only because the
  planner routed around these shapes; smoke particles do nothing to PX4.
* **Avoidance is measured**, not assumed: the mission computes the drone's ENU position from
  PX4 local position each tick and records `hazard_entered` / `min_clearance_m`;
  `avoidance_executed` = direct line blocked AND hazard never entered.
* **Fire model** (`fire_model.py`, illustrative, NOT validated physics): logistic growth of
  intensity, suppression only while a drop is active within `drop_effective_range_m` (10 m),
  extinguished below 0.05 and stays out. Runs on Gazebo sim time (`/clock` bridged explicitly).
  The mission reads `/emberwatch/fire_state` = **simulated state access**, not recognition.
* **Mission:** 10 Hz Offboard stream, ≥1 s pre-stream, arm, climb 10 m, route, hover facing
  fire, simulated drop for 8 *sim* seconds (PX4 lockstep time), re-drop up to 2× if still
  active, fly the route back, PX4 `NAV_LAND`. If PX4 leaves Offboard the node aborts and does
  not fight it — PX4's own RTL failsafe is **not hazard-aware**.

**Verified runs (2026-11-15, synthetic fixture, after the stale-process fix below):**

| Run | Result |
|---|---|
| demo wind 270°/5 m/s, GUI, with a deliberately injected foreign `fire_sim` | direct line blocked → 5-waypoint detour; hazard never entered (min clearance 2.34 m); drop in range; own model EXTINGUISHED after 4.2 sim-s of water; 97 foreign fire-state msgs ignored; returned; landed/disarmed; RTF 1.01. |
| wind unknown, headless | conservative circle; direct route (`avoidance_executed=False`, none needed); 2 drops out of range; fire still ACTIVE (0.35→0.49); returned; landed; RTF 1.01. |
| launch site inside plume (script) | exit 3 NO PATH; PX4 not launched. |
| swapped lat/lon, outside Portugal, empty incidents, uncited, API down | exit 2 REJECTED; nothing launched (also unit-tested). |
| kinematic stand-in demo (headless) | full sequence to DONE. |
| process cleanup | after each run no PX4/agent/bridge/node processes remain. |

**Fixed during verification:** earlier versions left `ros2 run` child processes (old `fire_sim`
nodes) alive after a run; they kept publishing `/emberwatch/fire_state`, so fire outcomes from
the 2nd–4th runs of the first session were **not trustworthy** (only the first run was clean).
Now each scenario has a unique `scenario_id`; `fire_sim` and the mission ignore messages for any
other id, the script kills whole process trees and refuses to start if stale demo nodes exist.

First GUI run ran ~0.1–0.3× real time (first launch + another gz sim on this machine using
~150 % CPU); timing now uses sim time so the outcome does not depend on RTF.

## Machine inspected

Ubuntu 24.04.4 · 16 cores · 15 GiB RAM · Intel UHD + RTX 3080 Laptop (driver 535) ·
ROS 2 **Jazzy** at `/opt/ros/jazzy` · Gazebo **Harmonic** (gz-sim 8.11, via ROS vendor
packages) · `ros_gz_bridge` installed · no PX4, no `px4_msgs`, no `MicroXRCEAgent`.
This matches PX4's documented set: Ubuntu 24.04 + ROS 2 Jazzy + Gazebo Harmonic.

## Run the Gazebo demo (works without PX4)

```sh
cd ros2
source /opt/ros/jazzy/setup.bash
colcon build --symlink-install
source install/setup.bash

# from the checked-in synthetic fixture
ros2 launch emberwatch_drone kinematic_demo.launch.py \
  snapshot_file:=$(realpath ../contracts/fixtures/snapshot.json)

# or from the running EmberWatch server (cargo run -p emberwatch-server)
ros2 launch emberwatch_drone kinematic_demo.launch.py api_base:=http://127.0.0.1:8080 run_id:=synthetic-portugal

# headless check that exits when the mission ends
ros2 launch emberwatch_drone kinematic_demo.launch.py headless:=true exit_when_done:=true snapshot_file:=...
```

Watch progress: `ros2 topic echo /emberwatch/mission_status` (JSON, transient-local).
Phases: `TAKEOFF → TRANSIT → SIM_WATER_DROP → RETURN → LAND → DONE` (or `ABORTED`).
Print the mission only: `ros2 run emberwatch_drone plan_mission --snapshot ../contracts/fixtures/snapshot.json`.

Tests: `cd src/emberwatch_drone && python3 -m pytest -q test` (geo + adapter, no ROS needed).

### Interfaces

* Input: contract `Snapshot` (`incidents[].position` is `[lon, lat]`, `evidence[].provenance`).
  Rejected (no mission): no incidents (never read as "zero fires"), missing position,
  position outside mainland Portugal bbox (catches swapped lat/lon), no evidence `source_url`.
* Output: `/emberwatch/mission_status` (`std_msgs/String` JSON) incl. label, phase, incident,
  `data_mode`, target lon/lat, source URLs, wind (null if unknown), drone scene position.
* Gazebo ⇄ ROS only through `config/bridge.yaml`: `/emberwatch/drone/cmd_vel` → gz
  `/model/ember_drone/cmd_vel`, gz `/model/ember_drone/odometry` → `/emberwatch/drone/odom`,
  `/clock`. The water emitter is toggled with `gz topic ... gz.msgs.ParticleEmitter`
  (no ROS message exists for it).
* Kinematic demo: the stand-in lands ~0.3 m above its start height (landing tolerance).
* Mismatch to note: the server's own `dispatch` / `drone_plans` are not consumed yet; the
  ROS side reads incidents/evidence directly and does not report back to the web app.
* The scene is not geographic: the fire sits at scene ENU (12, 8) m; the real lat/lon is
  only carried in the status message.

## Frames: ENU vs NED, global → local

* Contract/GeoJSON: `[longitude, latitude]` WGS84.
* ROS/Gazebo: ENU (x East, y North, z Up). PX4: NED (x North, y East, z Down).
  `n = y_enu, e = x_enu, d = -z_enu`; `yaw_ned = π/2 − yaw_enu`.
* Global → local (`geo.py`) uses PX4's azimuthal-equidistant `MapProjection` about the
  EKF origin (`ref_lat/ref_lon` from `VehicleLocalPosition`). Short ranges only.

## PX4 SITL path (what was actually run, no sudo)

Everything lives in `~/px4` (outside the repo). Disk was the constraint (~3 GB free), so:
shallow clone, unused submodules dropped, Python deps in an isolated dir, Fast-DDS/CDR from
ROS Jazzy. `ubuntu.sh` was **not** run (needs sudo); the deps below were already present or
pip-installable. Nothing was added to `~/.bashrc`; `~/px4/env.sh` is sourced by hand.

```sh
# one-time setup (done on this machine)
mkdir -p ~/px4/tools && cd ~/px4
curl -sSfLo tools/pip.pyz https://bootstrap.pypa.io/pip/pip.pyz      # system python has no pip
python3 -I tools/pip.pyz install --target ~/px4/pydeps ninja kconfiglib "empy==3.3.4" \
    pyros-genmsg future toml jinja2 jsonschema pyyaml packaging lark
# ~/px4/env.sh: PYTHONPATH/PATH += ~/px4/pydeps, then source /opt/ros/jazzy/setup.bash
git clone --depth 1 -b v1.17.0 --recurse-submodules --shallow-submodules \
    https://github.com/PX4/PX4-Autopilot.git
cd PX4-Autopilot      # SITL-only: drop NuttX, gazebo-classic, jMAVSim, FlightGear, JSBSim
for p in platforms/nuttx/NuttX/nuttx platforms/nuttx/NuttX/apps \
  Tools/simulation/gazebo-classic/sitl_gazebo-classic Tools/simulation/jmavsim/jMAVSim \
  Tools/simulation/flightgear/flightgear_bridge Tools/simulation/jsbsim/jsbsim_bridge; do
  git submodule deinit -f "$p" && rm -rf ".git/modules/$p"; done
source ~/px4/env.sh && make px4_sitl                       # ~500 MB build dir

mkdir -p ~/px4/ros2_px4_ws/src && cd ~/px4/ros2_px4_ws/src
git clone --depth 1 -b release/1.17 https://github.com/PX4/px4_msgs.git   # matches PX4 v1.17
git clone --depth 1 -b v2.4.3 https://github.com/eProsima/Micro-XRCE-DDS-Agent.git
cd .. && source /opt/ros/jazzy/setup.bash && colcon build --cmake-args \
    -DCMAKE_BUILD_TYPE=Release -DUAGENT_USE_SYSTEM_FASTDDS=ON -DUAGENT_USE_SYSTEM_FASTCDR=ON
# (px4_msgs takes ~15 min to generate/compile)
```

Run (three terminals):

```sh
# 1. PX4 SITL + stock x500 in Gazebo (equivalent to `make px4_sitl gz_x500`, but daemon mode).
#    GZ_PARTITION keeps it apart from any other gz sim running on the machine.
source ~/px4/env.sh && export GZ_PARTITION=emberwatch
cd ~/px4/PX4-Autopilot/build/px4_sitl_default
PX4_SYS_AUTOSTART=4001 PX4_SIM_MODEL=gz_x500 ./bin/px4 -d
#    No QGroundControl here, so arming fails "No connection to the GCS". SIM-ONLY workaround
#    (stored in this SITL instance's params, not global):
./bin/px4-param set NAV_DLL_ACT 0
#    stock check: ./bin/px4-commander takeoff ; ./bin/px4-commander land

# 2. uXRCE-DDS agent (its colcon setup does not export the lib path)
LD_LIBRARY_PATH=~/px4/ros2_px4_ws/install/microxrcedds_agent/lib \
  ~/px4/ros2_px4_ws/install/microxrcedds_agent/bin/MicroXRCEAgent udp4 -p 8888

# 3. one simulated mission from the report
source /opt/ros/jazzy/setup.bash && source ~/px4/ros2_px4_ws/install/setup.bash
cd <repo>/ros2 && source install/setup.bash
ros2 run emberwatch_drone px4_offboard_mission --ros-args \
  -p snapshot_file:=$(realpath ../contracts/fixtures/snapshot.json) -p target_mode:=local
```

PX4's SITL home is its default (Zürich, ~47.398 N 8.546 E), so `target_mode:=geo` with the
Portugal fixture is correctly **refused** (> 3 km). To fly to the incident coordinates, start
PX4 with `PX4_HOME_LAT`/`PX4_HOME_LON` within 3 km of it (not yet tried).
Known log noise: `libGstCameraSystem.so` not found (camera streaming plugin; irrelevant),
SITL battery warnings on long sessions.

Offboard handling (per PX4 docs and `px4_ros_com` example; SITL values `COM_OF_LOSS_T`=1.0 s, `COM_OBL_RC_ACT`=0 → without RC PX4 fell back to RTL in the test): `OffboardControlMode` +
`TrajectorySetpoint` (NED, position) streamed at 10 Hz from a timer; ≥1 s pre-stream before
`DO_SET_MODE(1, 6)` + arm; stream must stay above 2 Hz or PX4 exits Offboard after
`COM_OF_LOSS_T` and runs `COM_OBL_RC_ACT`. If PX4 leaves Offboard the node aborts and does
not re-request it. After the simulated drop it commands Return-to-Launch. PX4 ≥1.16 suffixes
versioned topics (`/fmu/out/vehicle_local_position_v1`, `/fmu/out/vehicle_status_v1`);
verify with `ros2 topic list` and override the `topic_*` parameters if needed.
Note: the PX4 x500 Gazebo world and topics are separate from this scene; Gazebo topics such
as `/world/.../x500_0/...` are not PX4 ROS 2 topics — PX4 data reaches ROS only via uXRCE-DDS.
