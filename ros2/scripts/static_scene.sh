#!/usr/bin/env bash
# Static Gazebo look development only: no PX4, DDS bridge or mission.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
set +u
source /opt/ros/jazzy/setup.bash
set -u
export GZ_PARTITION="${GZ_PARTITION:-collab_w5}"
export GZ_IP=127.0.0.1
export PYTHONPATH="$HERE/../src/emberwatch_drone${PYTHONPATH:+:$PYTHONPATH}"
exec python3 "$HERE/static_scene.py" "$@"
