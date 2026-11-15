#!/usr/bin/env bash
# Independent SIMULATED recon world. Does not start or command PX4.
set -euo pipefail
RECON_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
set +u
source /opt/ros/jazzy/setup.bash
set -u
export PYTHONPATH="$RECON_ROOT/ros2/src/emberwatch_drone:${PYTHONPATH:-}"
export GZ_PARTITION="${RECON_GZ_PARTITION:-emberwatch_recon_$$}"
export GZ_IP=127.0.0.1
export ROS_DOMAIN_ID="${RECON_ROS_DOMAIN_ID:-94}"
exec 9>"/tmp/emberwatch-recon-$(id -u)-${ROS_DOMAIN_ID}.lock"
flock -n 9 || { echo "A recon demo already owns ROS domain $ROS_DOMAIN_ID; stop it or choose RECON_ROS_DOMAIN_ID." >&2; exit 2; }
RECON_LOG="$RECON_ROOT/ros2/generated/recon/$GZ_PARTITION"
mkdir -p "$RECON_LOG"
python3 - "$RECON_ROOT" "$RECON_LOG" <<'PY'
from pathlib import Path
import sys
from emberwatch_drone.recon import make_world
make_world(Path(sys.argv[1]) / 'ros2/src/emberwatch_drone/worlds/emberwatch_scene.sdf', Path(sys.argv[2]) / 'recon.sdf')
PY
RECON_PIDS=()
cleanup() {
  trap - EXIT INT TERM
  # Each group below was started by this launcher; never signal shared simulator processes.
  for recon_pid in "${RECON_PIDS[@]}"; do kill -TERM -- "-$recon_pid" 2>/dev/null || true; done
  for _ in {1..30}; do
    recon_alive=0
    for recon_pid in "${RECON_PIDS[@]}"; do kill -0 -- "-$recon_pid" 2>/dev/null && recon_alive=1; done
    [[ $recon_alive == 0 ]] && break
    sleep .1
  done
  for recon_pid in "${RECON_PIDS[@]}"; do kill -KILL -- "-$recon_pid" 2>/dev/null || true; wait "$recon_pid" 2>/dev/null || true; done
}
trap cleanup EXIT INT TERM
echo "SIMULATION ONLY: partition=$GZ_PARTITION ROS_DOMAIN_ID=$ROS_DOMAIN_ID logs=$RECON_LOG"
# Sensor frames need a rendering context. EGL headless rendering works with ogre2.
setsid gz sim -r -s --headless-rendering "$RECON_LOG/recon.sdf" > "$RECON_LOG/gazebo.log" 2>&1 & RECON_PIDS+=($!)
setsid "/opt/ros/jazzy/lib/ros_gz_bridge/parameter_bridge" \
  '/model/ember_drone/cmd_vel@geometry_msgs/msg/Twist]gz.msgs.Twist' \
  '/model/ember_drone/odometry@nav_msgs/msg/Odometry[gz.msgs.Odometry' \
  '/emberwatch/recon/camera@sensor_msgs/msg/Image[gz.msgs.Image' \
  --ros-args -r /model/ember_drone/cmd_vel:=/emberwatch/drone/cmd_vel \
  -r /model/ember_drone/odometry:=/emberwatch/drone/odom > "$RECON_LOG/bridge.log" 2>&1 & RECON_PIDS+=($!)
setsid python3 -m emberwatch_drone.recon_bridge "$@" > "$RECON_LOG/recon.log" 2>&1 & RECON_PIDS+=($!)
wait "${RECON_PIDS[2]}"
