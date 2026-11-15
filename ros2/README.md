# EmberWatch ROS 2 simulation

This workspace turns a cited EmberWatch incident snapshot into a simulated drone
mission. Gazebo provides the scene, ROS 2 connects mission and fire state, and PX4
SITL controls an x500. A lighter kinematic demo runs without PX4.

The Arganil scene uses geographic terrain and mapped features, but its seeded
fires, drone flights and water effects are synthetic. Fire growth and suppression
are illustrative. Mission completion reads simulated fire state, not a camera.

Read the [node and topic guide](../docs/ROS.md) for the connection diagram,
message types, coordinate frames and mission lifecycle.
[ARGANIL_DATA.md](ARGANIL_DATA.md) records scene sources and approximations.

## Requirements

The simulator setup uses ROS 2 Jazzy, Gazebo Harmonic, `ros_gz_sim`,
`ros_gz_bridge`, colcon and Python 3. Terrain generation also uses NumPy, SciPy,
Pillow and Matplotlib. Snapshot schema checks need `jsonschema`; tests use pytest.

For the PX4 demos, build these outside the repository:

| Dependency | Expected version/path |
|---|---|
| PX4 Autopilot | v1.17.0, `~/px4/PX4-Autopilot`, with `px4_sitl_default` and Gazebo x500 assets |
| `px4_msgs` | `release/1.17`, in `~/px4/ros2_px4_ws` |
| Micro XRCE-DDS Agent | v2.4.3, in `~/px4/ros2_px4_ws` |

The run script accepts `PX4_DIR` and `PX4_WS` to override those paths. Match the
message definitions to the installed PX4 version. The default telemetry topics
are versioned with `_v1`.

## Build

From the repository root:

```sh
source /opt/ros/jazzy/setup.bash
cd ros2
colcon build --symlink-install
source install/setup.bash
```

Source the ROS setup and this workspace's `install/setup.bash` in each terminal
that runs ROS commands. PX4 commands also need
`source ~/px4/ros2_px4_ws/install/setup.bash`.

## Inspect a mission without a simulator

From the repository root, no ROS installation is needed for this adapter:

```sh
PYTHONPATH=ros2/src/emberwatch_drone python3 -m emberwatch_drone.cli \
  --snapshot contracts/fixtures/snapshot.json
```

It preserves the incident's source links, mode and timestamps. It rejects
uncited reports, missing coordinates and points outside mainland Portugal.
Wind is taken only from a nearby observed weather record; unknown wind and
event time remain `null`.

## PX4 fire demo

After building the ROS and PX4 workspaces, run from the repository root:

```sh
ros2/scripts/run_px4_fire_demo.sh --demo-wind 270 5
```

The script plans a scenario, launches Gazebo, starts the XRCE-DDS agent on UDP
8888, starts PX4, bridges the simulation clock and runs the fire and mission
nodes. `--demo-wind` supplies labelled synthetic wind only when the report has
none. Omitting it keeps missing wind unknown and can make a route unavailable.

```sh
# Non-interactive run, with logs and a final summary.
ros2/scripts/run_px4_fire_demo.sh --demo-wind 270 5 --headless --exit-when-done

# Read the original Snapshot API from a running Rust service.
ros2/scripts/run_px4_fire_demo.sh \
  --api http://127.0.0.1:8080 --run-id synthetic-portugal --demo-wind 270 5

# Read a saved snapshot.
ros2/scripts/run_px4_fire_demo.sh --snapshot /absolute/path/to/snapshot.json
```

The GUI remains open after a mission unless `--exit-when-done` is set. Ctrl-C
stops the processes owned by the script. It refuses to start alongside another
PX4/demo instance using the same topics and ports.

## Arganil terrain demo

From the repository root, after sourcing the workspaces:

```sh
ros2/scripts/fetch_arganil_data.sh
ros2 run emberwatch_drone gen_arganil_world --out ros2/generated/arganil
ros2/scripts/run_px4_fire_demo.sh \
  --world arganil --seed 37 --distance 800 --demo-wind 270 5
```

Arganil runs default to a **simulated 4-drone containment fleet** and a canopy (crown) fire;
`--fleet 0` restores the single-drone mission and `--fire-in clearing` the small open-ground
fire. `--firefighter-eta S` sets the assumed ground-crew arrival (default 420 sim-s).

Downloads go to `~/px4/geodata/`; generated meshes, textures and obstacle rasters
go to the ignored `ros2/generated/` directory. Review
[ARGANIL_DATA.md](ARGANIL_DATA.md) for optional imagery and regeneration details.
`gen_arganil_world --no-satellite` selects the painted fallback.

The scenario generator chooses a seeded synthetic fire and a launch point,
then plans cruise, approach and return geometry around the obstacle raster and
configured smoke hazard. Requested launch distance may shrink to find a valid
site; that fallback is recorded. A route failure returns `NO_PATH` and prevents
takeoff. Use `--force-blocked` to exercise that refusal.

### Canopy fire and containment fleet (simulated)

- **Fuel:** the seed and every spread point lie on tree cells of the obstacle raster
  (`fire.fuel = tree_canopy`). Spread is a seeded tree of points (`parent` index): a point can
  only ignite once its parent burns. `forest_fire_fx` draws large flame and smoke sprites on
  the canopy at each burning point. These are visuals only, not fire physics.
- **Fleet:** each drone is a separate PX4 SITL instance (`-i k`, namespace `/px4_k`) on its own
  level pad. Drones launch 8 s apart and fly the planned route in their own altitude layer
  (+5 m each) to an upwind staging point.
- **Containment:** a coordinator sends each drone to the most urgent unburned *front* point
  that is not yet wet. The drone hovers above the canopy just outside the front and wets that
  point. Wet fuel cannot ignite for `--wet-duration` s, so the fire is **held, not
  extinguished**.
- **Logistics:** each drone carries `--water-capacity-s` of spray. When empty, it flies to a
  simulated forward refill tank on open ground upwind of the fire.
- **Handover:** at `--firefighter-eta` the spread model stops (handover to ground crews) and
  the drones fly their routes back and land.
- **Timer:** `mission_hud` opens an always-on-top window showing fire time (HH:MM:SS, simulated
  clock), the crew ETA countdown, the number of held and wet points, and each drone's phase and
  water.
- **Smooth flight:** both missions stream look-ahead setpoints along the planned 3-D path
  (10 m at cruise, 3.5 m at low level, below the 4 m planning margin) with yaw limited to
  35-40 °/s.

Verified run (2026-11-15, seed 37, wind 270°/5 m/s, `--firefighter-eta 300`):

- All 4 drones finished DONE (mission exit 0).
- 13 front-point sprays and 2 refills.
- No drone went below a modelled obstacle top; minimum vertical clearance was 5.85 m.
- No transit leg entered the planning hazard.
- 67 of 160 points had burned at handover. Without wetting, the same spread tree reaches 96
  points by 300 s.
- Station-to-station legs are flown straight at the station altitude (highest canopy in the
  fire area + 20 m), so they can pass above burning points. The minimum horizontal distance to a
  burning point was 0.1 m, at that altitude.


## Kinematic demo without PX4

From the repository root, after building and sourcing the ROS workspace:

```sh
ros2 launch emberwatch_drone kinematic_demo.launch.py \
  snapshot_file:="$(realpath contracts/fixtures/snapshot.json)"
```

Use `headless:=true exit_when_done:=true` for a non-interactive run. This demo
drives Gazebo's `VelocityControl` system through an explicit ROS/Gazebo bridge.
Its water effect is visual only; it does not run the PX4 fire suppression loop.

`px4_offboard_mission` is a separate minimal PX4 example with takeoff, a local
target, a status-only drop and return-to-launch. Run one mission controller at a
time. Geographic target mode requires the reported point to be near PX4's home;
the default PX4 home is outside Portugal and is rejected for the Portugal fixture.

## Tests and run output

From the repository root:

```sh
python3 -m pytest -c ros2/pytest.ini
```

The Python tests cover coordinate conversion, provenance, unknown conditions,
scenario generation, hazard avoidance and scene helpers. They do not replace an
end-to-end PX4 flight check.

Each fire demo writes `ros2/run_logs/px4_fire_<timestamp>/` with `scenario.json`,
`summary.json` and process logs. The summary records mission outcome, simulated
drops, hazard entry and obstacle clearance when the scenario provides a raster.

| Exit code | Meaning |
|---|---|
| `0` | Mission process completed; inspect the summary for its outcome |
| `2` | Input rejected before launch |
| `3` | No planned path; flight was not launched |
| Other nonzero | Startup or runtime failure; inspect process logs |

For topic inspection and troubleshooting, continue with
[the ROS connection guide](../docs/ROS.md#inspect-the-running-system).
