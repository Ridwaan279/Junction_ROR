#!/usr/bin/env bash
# EmberWatch PX4 SITL fire demo: report -> scenario -> Gazebo world + PX4 x500 (standalone)
# -> fire_sim (illustrative model/visuals) -> px4_fire_mission (route, hover, simulated drop).
# SIMULATION ONLY. Does not modify shell rc files. Logs: ros2/run_logs/px4_fire_<time>/
#
# Usage:
#   ros2/scripts/run_px4_fire_demo.sh [--snapshot FILE | --api URL [--run-id ID]]
#       [--demo-wind FROM_DEG SPEED_MPS] [--launch-from-fire EAST_M NORTH_M] [--headless] [--exit-when-done]
#       [--world arganil [--seed N] [--distance M] [--force-blocked] [--fleet N] [--fire-in clearing]]
#   --fleet N: SIMULATED containment fleet of N PX4 drones (holds the spread until assumed ground-crew ETA)
# Defaults: --snapshot contracts/fixtures/snapshot.json (SYNTHETIC fixture), wind unknown.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROS2_DIR="$(dirname "$HERE")"
REPO="$(dirname "$ROS2_DIR")"
PX4_DIR="${PX4_DIR:-$HOME/px4/PX4-Autopilot}"
PX4_WS="${PX4_WS:-$HOME/px4/ros2_px4_ws}"
SRC=(--snapshot "$REPO/contracts/fixtures/snapshot.json")
WIND=()
HEADLESS=0
EXIT_WHEN_DONE=0
WORLD_KIND=flat
SEED=37
DISTANCE=800
FLEET=0
ARG_EXTRA=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --snapshot) SRC=(--snapshot "$(realpath "$2")"); shift 2 ;;
    --api) SRC=(--api "$2"); shift 2 ;;
    --run-id) SRC+=(--run-id "$2"); shift 2 ;;
    --demo-wind) WIND=(--demo-wind "$2" "$3"); shift 3 ;;
    --launch-from-fire) WIND+=(--launch-from-fire "$2" "$3"); shift 3 ;;
    --headless) HEADLESS=1; shift ;;
    --world) WORLD_KIND="$2"; shift 2 ;;            # flat (default) | arganil
    --seed) SEED="$2"; shift 2 ;;
    --distance) DISTANCE="$2"; shift 2 ;;
    --force-blocked) ARG_EXTRA+=(--force-blocked); shift ;;
    --fleet) FLEET="$2"; FLEET_SET=1; [[ $2 -gt 0 ]] && ARG_EXTRA+=(--fleet "$2"); shift 2 ;;
    --fire-in) ARG_EXTRA+=(--fire-in "$2"); shift 2 ;;
    --firefighter-eta) ARG_EXTRA+=(--firefighter-eta "$2"); shift 2 ;;
    --exit-when-done) EXIT_WHEN_DONE=1; shift ;;
    *) echo "unknown arg $1" >&2; exit 64 ;;
  esac
done

# Arganil default: SIMULATED 4-drone containment fleet (pass --fleet 0 for the single-drone mission)
if [[ $WORLD_KIND == arganil && -z "${FLEET_SET:-}" ]]; then FLEET=4; ARG_EXTRA+=(--fleet 4); fi
LOG="$ROS2_DIR/run_logs/px4_fire_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$LOG"
set +u
source /opt/ros/jazzy/setup.bash
source "$PX4_WS/install/setup.bash"
source "$ROS2_DIR/install/setup.bash"
source "$PX4_DIR/build/px4_sitl_default/rootfs/gz_env.sh"   # PX4 models/plugins (exports only)
set -u
SHARE="$(ros2 pkg prefix emberwatch_drone)/share/emberwatch_drone"
export GZ_SIM_RESOURCE_PATH="$GZ_SIM_RESOURCE_PATH:$SHARE/worlds:$ROS2_DIR/generated/arganil"
export GZ_PARTITION="${GZ_PARTITION:-emberwatch_px4}"
# Pin Gazebo transport to loopback: by default it binds to the active network interface, and a
# Wi-Fi drop mid-flight broke PX4<->Gazebo sensor traffic (observed: sensor timeouts -> failsafe).
export GZ_IP="${GZ_IP:-127.0.0.1}"
export EMBERWATCH_REPO="$REPO"   # lets generators find contracts/ regardless of install layout
WORLD=emberwatch_px4
WORLD_SDF="$SHARE/worlds/$WORLD.sdf"
FIRE_ARGS=()
MISSION_ARGS=()
if [[ $WORLD_KIND == arganil ]]; then
  WORLD=emberwatch_arganil
  GEN="$ROS2_DIR/generated/arganil"
  WORLD_SDF="$GEN/$WORLD.sdf"
  [[ -f $WORLD_SDF ]] || { echo "missing $WORLD_SDF - run: ros2 run emberwatch_drone gen_arganil_world --out $GEN" >&2; exit 1; }
  FIRE_ARGS=(-p hazard_visual_height_m:="${HAZARD_VIS_H:-0.1}")  # planning hazard is still enforced; 0.1 m keeps the red debug columns out of the recording (HAZARD_VIS_H=28 shows them)
  MISSION_ARGS=(-p max_drops:=14 -p drop_s:=6.0 -p waypoint_radius_m:=3.0 -p arrive_radius_m:=1.2 -p marker_radius_m:=0.7)
fi

set +e
if [[ $WORLD_KIND == arganil ]]; then
  echo "== 1/6 Arganil scenario: seed $SEED, distance $DISTANCE m (${WIND[*]:-wind from event or unknown})"
  SNAP_ARG=()
  [[ ${SRC[0]} == --snapshot && ${SRC[1]} != "$REPO/contracts/fixtures/snapshot.json" ]] && SNAP_ARG=(--snapshot "${SRC[1]}")
  ros2 run emberwatch_drone plan_arganil_scenario --seed "$SEED" --distance "$DISTANCE" --world "$GEN" \
    --out-dir "$LOG" "${SNAP_ARG[@]}" "${WIND[@]}" "${ARG_EXTRA[@]}" > "$LOG/home.env" 2> "$LOG/plan.txt"
else
  echo "== 1/6 scenario from report (${SRC[*]} ${WIND[*]:-wind from feed or unknown})"
  ros2 run emberwatch_drone plan_scenario "${SRC[@]}" "${WIND[@]}" --out "$LOG/scenario.json" > "$LOG/home.env" 2> "$LOG/plan.txt"
fi
rc=$?
set -e
cat "$LOG/plan.txt"
if [[ $rc -eq 2 ]]; then echo "event REJECTED; nothing launched"; exit 2; fi
if [[ $rc -eq 3 ]]; then echo "NO PATH to safe point; not launching a flight (see $LOG/scenario.json)"; exit 3; fi
set -a; source "$LOG/home.env"; set +a

# Refuse to run next to a previous demo's nodes (they would share topics/ports).
STALE=$(pgrep -f 'lib/emberwatch_drone/(fire_sim|px4_fire_mission|fleet_mission|forest_fire_fx|mission_hud)|MicroXRCEAgent udp4 -p 8888' || true)
if [[ -n "$STALE" ]] || pgrep -x px4 > /dev/null; then
  echo "stale EmberWatch/PX4 processes are running (pids: $STALE $(pgrep -x px4 || true)); stop them first" >&2
  exit 1
fi

PIDS=()
killtree() {  # `ros2 run` forks the node as a child; kill children before the parent
  local c
  for c in $(pgrep -P "$1" 2>/dev/null); do killtree "$c"; done
  kill "$1" 2>/dev/null || true
}
cleanup() {
  echo "== stopping simulation processes"
  for p in "${PIDS[@]}"; do killtree "$p"; done
  for c in $(pgrep -P $$ 2>/dev/null); do killtree "$c"; done
}
trap cleanup EXIT INT TERM

echo "== 2/6 Gazebo world $WORLD (partition $GZ_PARTITION)"
gz sim -r -s "$WORLD_SDF" > "$LOG/gz_server.log" 2>&1 & PIDS+=($!)
if [[ $HEADLESS -eq 0 ]]; then gz sim -g > "$LOG/gz_gui.log" 2>&1 & PIDS+=($!); fi

spawn_box() {  # name x y top sx sy h r g b : static box collider+visual with its top at z=top
  local f="$LOG/$1.sdf"
  cat > "$f" <<SDF
<sdf version="1.9"><model name="$1"><static>true</static><pose>$2 $3 $(python3 -c "print($4 - $7 / 2)") 0 0 0</pose>
<link name="l"><collision name="c"><geometry><box><size>$5 $6 $7</size></box></geometry></collision>
<visual name="v"><geometry><box><size>$5 $6 $7</size></box></geometry><material><ambient>$8 $9 ${10} 1</ambient><diffuse>$8 $9 ${10} 1</diffuse></material></visual>
<visual name="mark"><pose>0 0 $(python3 -c "print($7 / 2 + 0.005)") 0 0 0</pose><geometry><cylinder><radius>1.6</radius><length>0.01</length></cylinder></geometry><material><ambient>0.95 0.75 0.1 1</ambient><diffuse>0.95 0.75 0.1 1</diffuse></material></visual>
</link></model></sdf>
SDF
  for _ in $(seq 60); do
    gz service -s "/world/$WORLD/create" --reqtype gz.msgs.EntityFactory --reptype gz.msgs.Boolean --timeout 2000 \
      --req "sdf_filename: \"$f\" name: \"$1\"" 2>/dev/null | grep -q true && { echo "   $1 at $2,$3,$4"; return 0; }
    sleep 1
  done
  return 1
}
if [[ $FLEET -gt 0 ]]; then
  IFS=';' read -ra FPOSES <<< "$EMBER_FLEET_POSES"
  for k in "${!FPOSES[@]}"; do
    IFS=, read -r PX PY PZ <<< "${FPOSES[$k]}"
    spawn_box "launch_pad_$k" "$PX" "$PY" "$PZ" 5 5 1.2 0.25 0.27 0.30
  done
  IFS=, read -r TX TY TZ <<< "$EMBER_TANK_POSE"
  # SIMULATED forward water tank (visual + collider): a 6 m blue folding tank, 1.5 m tall
  spawn_box "sim_forward_water_tank" "$TX" "$TY" "$(python3 -c "print($TZ + 1.5)")" 6 6 3 0.12 0.35 0.75
elif [[ -n "${EMBER_PAD_POSE:-}" ]]; then
  IFS=, read -r PX PY PZ <<< "$EMBER_PAD_POSE"
  PAD_SDF="$LOG/launch_pad.sdf"
  cat > "$PAD_SDF" <<SDF
<sdf version="1.9"><model name="launch_pad"><static>true</static><pose>$PX $PY $(python3 -c "print($PZ - 0.6)") 0 0 0</pose>
<link name="l"><collision name="c"><geometry><box><size>5 5 1.2</size></box></geometry></collision>
<visual name="v"><geometry><box><size>5 5 1.2</size></box></geometry><material><ambient>0.25 0.27 0.30 1</ambient><diffuse>0.25 0.27 0.30 1</diffuse></material></visual>
<visual name="mark"><pose>0 0 0.605 0 0 0</pose><geometry><cylinder><radius>1.6</radius><length>0.01</length></cylinder></geometry><material><ambient>0.95 0.75 0.1 1</ambient><diffuse>0.95 0.75 0.1 1</diffuse></material></visual>
</link></model></sdf>
SDF
  for _ in $(seq 60); do   # world must be up before the pad (and before PX4 spawns the x500 on it)
    gz service -s "/world/$WORLD/create" --reqtype gz.msgs.EntityFactory --reptype gz.msgs.Boolean --timeout 2000 \
      --req "sdf_filename: \"$PAD_SDF\" name: \"launch_pad\"" 2>/dev/null | grep -q true && { echo "   launch pad at $EMBER_PAD_POSE"; break; }
    sleep 1
  done
fi

echo "== 3/6 uXRCE-DDS agent udp 8888"
LD_LIBRARY_PATH="$PX4_WS/install/microxrcedds_agent/lib:${LD_LIBRARY_PATH:-}" \
  "$PX4_WS/install/microxrcedds_agent/bin/MicroXRCEAgent" udp4 -p 8888 > "$LOG/agent.log" 2>&1 & PIDS+=($!)

if [[ $FLEET -gt 0 ]]; then
  echo "== 4/6 PX4 SITL fleet: $FLEET x500 instances (namespaces /px4_0../px4_$((FLEET - 1)))"
  for k in "${!FPOSES[@]}"; do
    IFS=, read -r PX PY PZ <<< "${FPOSES[$k]}"
    ( cd "$PX4_DIR/build/px4_sitl_default" && PX4_GZ_STANDALONE=1 PX4_GZ_WORLD=$WORLD PX4_SYS_AUTOSTART=4001 \
        PX4_SIM_MODEL=gz_x500 PX4_GZ_MODEL_POSE="$PX,$PY,$(python3 -c "print($PZ + 0.3)"),0,0,0" \
        PX4_UXRCE_DDS_NS="px4_$k" exec ./bin/px4 -i "$k" -d ) > "$LOG/px4_$k.log" 2>&1 & PIDS+=($!)
    for _ in $(seq 120); do grep -q 'Startup script returned' "$LOG/px4_$k.log" 2>/dev/null && break; sleep 1; done
    grep -q 'Startup script returned' "$LOG/px4_$k.log" || { echo "PX4 instance $k did not start; see $LOG/px4_$k.log"; exit 1; }
    "$PX4_DIR/build/px4_sitl_default/bin/px4-param" --instance "$k" set NAV_DLL_ACT 0 > /dev/null   # SIM-ONLY, no GCS
  done
  for k in "${!FPOSES[@]}"; do
    for _ in $(seq 90); do grep -q 'home set' "$LOG/px4_$k.log" && break; sleep 1; done
  done
  cp "$LOG/px4_0.log" "$LOG/px4.log"
else
echo "== 4/6 PX4 SITL x500 (standalone, world origin $PX4_HOME_LAT,$PX4_HOME_LON alt $PX4_HOME_ALT, spawn ${PX4_GZ_MODEL_POSE:-0,0,0})"
( cd "$PX4_DIR/build/px4_sitl_default" && PX4_GZ_STANDALONE=1 PX4_GZ_WORLD=$WORLD PX4_SYS_AUTOSTART=4001 \
    PX4_SIM_MODEL=gz_x500 exec ./bin/px4 -d ) > "$LOG/px4.log" 2>&1 & PIDS+=($!)
for _ in $(seq 120); do grep -q 'Startup script returned' "$LOG/px4.log" 2>/dev/null && break; sleep 1; done
grep -q 'Startup script returned' "$LOG/px4.log" || { echo "PX4 did not start; see $LOG/px4.log"; exit 1; }
# SIM-ONLY: no QGroundControl here, so disable the GCS-link-loss failsafe that blocks arming.
"$PX4_DIR/build/px4_sitl_default/bin/px4-param" set NAV_DLL_ACT 0 > /dev/null
for _ in $(seq 60); do grep -q 'home set' "$LOG/px4.log" && break; sleep 1; done
fi
sleep 3
if [[ $HEADLESS -eq 0 ]]; then
  # Point the GUI camera at the launch site + fire (default camera faces +X only).
  CAM_REQ=$(python3 - "$LOG/scenario.json" <<'PY'
import json, math, sys
sc = json.load(open(sys.argv[1]))
fx, fy = sc["fire"]["enu"]
tx, ty = fx / 2, fy / 2
L = math.hypot(fx, fy) or 1.0
px, py = -fy / L, fx / L                      # side-on to the launch->fire line
cam = (tx + px * 26, ty + py * 26, 18.0)
if cam[1] < ty:                               # prefer viewing from the north side
    cam = (tx - px * 26, ty - py * 26, 18.0)
dx, dy, dz = tx - cam[0], ty - cam[1], 2.0 - cam[2]
yaw, pitch = math.atan2(dy, dx), math.atan2(-dz, math.hypot(dx, dy))
cp, sp, cy, sy = math.cos(pitch / 2), math.sin(pitch / 2), math.cos(yaw / 2), math.sin(yaw / 2)
print(f"pose: {{position: {{x: {cam[0]:.2f}, y: {cam[1]:.2f}, z: {cam[2]:.2f}}}, "
      f"orientation: {{x: {-sp * sy:.4f}, y: {sp * cy:.4f}, z: {cp * sy:.4f}, w: {cp * cy:.4f}}}}}")
PY
)
  # Leave the Arganil camera under manual user control. Fixed presets are in static_scene.sh.
  if [[ $WORLD_KIND != arganil ]]; then
    gz service -s /gui/move_to/pose --reqtype gz.msgs.GUICamera --reptype gz.msgs.Boolean \
      --timeout 3000 --req "$CAM_REQ" > /dev/null 2>&1 || echo "(could not move GUI camera)"
  fi
fi

echo "== 5/6 fire_sim (illustrative model + visuals, on Gazebo sim time)"
# Explicit bridge: Gazebo world clock -> ROS /clock (a gz topic is not a ROS topic by itself).
ros2 run ros_gz_bridge parameter_bridge "/world/$WORLD/clock@rosgraph_msgs/msg/Clock[gz.msgs.Clock" \
  --ros-args -r "/world/$WORLD/clock:=/clock" > "$LOG/clock_bridge.log" 2>&1 & PIDS+=($!)
ros2 run emberwatch_drone fire_sim --ros-args -p scenario_file:="$LOG/scenario.json" -p world:=$WORLD \
  -p use_sim_time:=true "${FIRE_ARGS[@]}" > "$LOG/fire_sim.log" 2>&1 & PIDS+=($!)
if [[ $WORLD_KIND == arganil ]] && python3 -c "import json,sys; sys.exit(json.load(open('$LOG/scenario.json'))['fire'].get('fuel') != 'tree_canopy')"; then
  # SIMULATED crown fire on the tree canopy (visual layer driven by fire_sim's model state)
  ros2 run emberwatch_drone forest_fire_fx --ros-args -p scenario_file:="$LOG/scenario.json" -p world:=$WORLD \
    -p density:="${FIRE_DENSITY:-1.0}" > "$LOG/forest_fire_fx.log" 2>&1 & PIDS+=($!)
fi
if [[ $HEADLESS -eq 0 && -n "${DISPLAY:-}" ]]; then
  ros2 run emberwatch_drone mission_hud > "$LOG/mission_hud.log" 2>&1 & PIDS+=($!)   # HH:MM:SS timer overlay
fi
sleep 4

set +e
if [[ $FLEET -gt 0 ]]; then
  echo "== 6/6 fleet_mission ($FLEET drones, SIMULATED containment)"
  ros2 run emberwatch_drone fleet_mission --ros-args -p scenario_file:="$LOG/scenario.json" \
    -p summary_file:="$LOG/summary.json" 2>&1 | tee "$LOG/mission.log" \
    | grep --line-buffered -E '\]: \[drone|BELOW|Traceback|fleet|handover'
  mrc=${PIPESTATUS[0]}
else
echo "== 6/6 px4_fire_mission"
ros2 run emberwatch_drone px4_fire_mission --ros-args -p scenario_file:="$LOG/scenario.json" \
  -p summary_file:="$LOG/summary.json" -p world:=$WORLD "${MISSION_ARGS[@]}" 2>&1 | tee "$LOG/mission.log" \
  | grep --line-buffered -E '\]: \[|ENTERED|Traceback'
mrc=${PIPESTATUS[0]}
fi
set -e
echo "== mission exit code $mrc; summary: $LOG/summary.json"
[[ -f "$LOG/summary.json" ]] && cat "$LOG/summary.json"
if [[ $EXIT_WHEN_DONE -eq 0 && $HEADLESS -eq 0 ]]; then
  echo "== simulation left running for viewing; press Ctrl-C to stop everything"
  while true; do sleep 3600; done
fi
exit $mrc
