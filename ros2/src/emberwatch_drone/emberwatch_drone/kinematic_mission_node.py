"""Gazebo-only *kinematic* mission (no PX4, no flight dynamics).

A stand-in drone model in ``worlds/emberwatch_scene.sdf`` is driven by Gazebo's
VelocityControl system.  ROS <-> Gazebo topics are connected explicitly by
``config/bridge.yaml`` (ros_gz_bridge); nothing here assumes a Gazebo topic is a ROS
topic on its own.

Scene mapping: the scene is NOT a reconstruction of the incident site.  The fire is
placed at a fixed scene position (``scene_target_enu``) and the mission's real
lat/lon + provenance are carried in the status messages.  On arrival a clearly
labelled *simulated* water-drop particle effect is toggled; the fire emitter is not
changed (we claim nothing about suppression).
"""

from __future__ import annotations

import json
import math
import subprocess
import time

import rclpy
from geometry_msgs.msg import Twist
from nav_msgs.msg import Odometry
from rclpy.node import Node
from rclpy.qos import DurabilityPolicy, QoSProfile, ReliabilityPolicy
from std_msgs.msg import String

from .mission import MissionError, fetch_snapshot, load_snapshot_file, mission_from_snapshot

PHASES = ("WAIT_ODOM", "TAKEOFF", "TRANSIT", "SIM_WATER_DROP", "RETURN", "LAND", "DONE", "ABORTED")


class KinematicMission(Node):
    def __init__(self) -> None:
        super().__init__("emberwatch_kinematic_mission")
        p = self.declare_parameter
        p("snapshot_file", "")
        p("api_base", "")
        p("run_id", "synthetic-portugal")
        p("incident_id", "")
        p("scene_target_enu", [12.0, 8.0])
        p("cruise_alt_m", 8.0)
        p("hover_over_fire_m", 6.0)
        p("max_speed_mps", 4.0)
        p("gain", 0.8)
        p("arrive_radius_m", 0.6)
        p("water_drop_s", 6.0)
        p("odom_timeout_s", 1.0)
        p("water_emitter_topic", "/model/ember_drone/link/base_link/particle_emitter/sim_water/cmd")
        gp = lambda n: self.get_parameter(n).value  # noqa: E731

        snap_file, api = gp("snapshot_file"), gp("api_base")
        try:
            snap = load_snapshot_file(snap_file) if snap_file else fetch_snapshot(api, gp("run_id"))
            self.mission = mission_from_snapshot(snap, gp("incident_id") or None)
        except (MissionError, OSError, ValueError) as exc:
            self.get_logger().error(f"no mission: {exc}")
            raise SystemExit(2) from exc

        self.target_xy = tuple(float(v) for v in gp("scene_target_enu"))
        self.cruise = float(gp("cruise_alt_m"))
        self.hover = float(gp("hover_over_fire_m"))
        self.vmax = float(gp("max_speed_mps"))
        self.k = float(gp("gain"))
        self.r_arrive = float(gp("arrive_radius_m"))
        self.drop_s = float(gp("water_drop_s"))
        self.odom_timeout = float(gp("odom_timeout_s"))
        self.water_topic = gp("water_emitter_topic")

        latched = QoSProfile(depth=1, reliability=ReliabilityPolicy.RELIABLE, durability=DurabilityPolicy.TRANSIENT_LOCAL)
        self.cmd_pub = self.create_publisher(Twist, "/emberwatch/drone/cmd_vel", 10)
        self.status_pub = self.create_publisher(String, "/emberwatch/mission_status", latched)
        self.create_subscription(Odometry, "/emberwatch/drone/odom", self._on_odom, 10)

        self.pos: tuple[float, float, float] | None = None
        self.home: tuple[float, float, float] | None = None
        self.last_odom = 0.0
        self.phase = "WAIT_ODOM"
        self.phase_t0 = time.monotonic()
        self.create_timer(0.05, self._tick)  # 20 Hz
        self._status("mission accepted")
        self.get_logger().info(json.dumps(self.mission.to_dict(), ensure_ascii=False))

    # ------------------------------------------------------------------ helpers
    def _on_odom(self, msg: Odometry) -> None:
        p = msg.pose.pose.position
        self.pos = (p.x, p.y, p.z)
        self.last_odom = time.monotonic()

    def _status(self, note: str) -> None:
        m = self.mission
        body = {
            "label": m.label,
            "phase": self.phase,
            "note": note,
            "incident_id": m.incident_id,
            "data_mode": m.data_mode,
            "target_lon_lat": [m.target_lon, m.target_lat],
            "sources": [r.source_url for r in m.reports],
            "scene_target_enu": list(self.target_xy),
            "scene_is_geographic": False,
            "drone_pos_enu": [round(v, 2) for v in self.pos] if self.pos else None,
            "wind_speed_mps": m.conditions.wind_speed_mps,
            "wind_from_deg": m.conditions.wind_from_deg,
            "smoke": m.conditions.smoke,
            "wall_time_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        }
        self.status_pub.publish(String(data=json.dumps(body, ensure_ascii=False)))
        self.get_logger().info(f"[{self.phase}] {note}")

    def _set_phase(self, phase: str, note: str) -> None:
        self.phase = phase
        self.phase_t0 = time.monotonic()
        self._status(note)

    def _water(self, on: bool) -> None:
        # ros_gz_interfaces has no ParticleEmitter msg, so use the documented gz CLI command.
        cmd = ["gz", "topic", "-t", self.water_topic, "-m", "gz.msgs.ParticleEmitter",
               "-p", f"emitting: {{data: {'true' if on else 'false'}}}"]
        # Non-blocking: the gz CLI takes ~1 s to start and must not stall the control loop
        # (a blocked loop looks like stale odometry and trips the failsafe).
        try:
            self._water_proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except OSError as exc:
            self.get_logger().warning(f"water emitter toggle failed: {exc}")

    def _fly_to(self, goal: tuple[float, float, float]) -> float:
        assert self.pos is not None
        d = [g - c for g, c in zip(goal, self.pos)]
        dist = math.sqrt(sum(v * v for v in d))
        v = [self.k * x for x in d]
        n = math.sqrt(sum(x * x for x in v))
        if n > self.vmax:
            v = [x * self.vmax / n for x in v]
        t = Twist()
        t.linear.x, t.linear.y, t.linear.z = v  # yaw held at 0 => body frame == world ENU
        self.cmd_pub.publish(t)
        return dist

    def _stop(self) -> None:
        self.cmd_pub.publish(Twist())

    # --------------------------------------------------------------- state machine
    def _tick(self) -> None:
        now = time.monotonic()
        if self.phase in ("DONE", "ABORTED"):
            self._stop()
            return
        if self.pos is None:
            if now - self.phase_t0 > 15.0:
                self._set_phase("ABORTED", "no odometry from Gazebo bridge within 15 s")
            return
        if now - self.last_odom > self.odom_timeout:
            # Failsafe analogue: never keep commanding on stale state.
            self._stop()
            self._set_phase("ABORTED", f"odometry stale > {self.odom_timeout}s; holding zero velocity")
            return

        tx, ty = self.target_xy
        if self.phase == "WAIT_ODOM":
            self.home = self.pos
            self._set_phase("TAKEOFF", f"home at {tuple(round(v, 2) for v in self.home)}; climbing to {self.cruise} m")
        elif self.phase == "TAKEOFF":
            if self._fly_to((self.home[0], self.home[1], self.cruise)) < self.r_arrive:
                self._set_phase("TRANSIT", f"transit to scene fire position {self.target_xy}")
        elif self.phase == "TRANSIT":
            if self._fly_to((tx, ty, self.hover)) < self.r_arrive:
                self._stop()
                self._water(True)
                self._set_phase("SIM_WATER_DROP", "ARRIVED - SIMULATED water drop (visual effect only, no suppression modelled)")
        elif self.phase == "SIM_WATER_DROP":
            self._fly_to((tx, ty, self.hover))  # hold position
            if now - self.phase_t0 > self.drop_s:
                self._water(False)
                self._set_phase("RETURN", "simulated drop finished; returning to launch")
        elif self.phase == "RETURN":
            if self._fly_to((self.home[0], self.home[1], self.cruise)) < self.r_arrive:
                self._set_phase("LAND", "over launch point; descending")
        elif self.phase == "LAND":
            if self._fly_to(self.home) < 0.3:
                self._stop()
                self._set_phase("DONE", "landed (simulation complete)")


def main() -> None:
    rclpy.init()
    node = None
    try:
        node = KinematicMission()
        while rclpy.ok() and node.phase not in ("DONE", "ABORTED"):
            rclpy.spin_once(node, timeout_sec=0.1)
        if node.phase == "DONE":
            for _ in range(5):  # let the latched status flush
                rclpy.spin_once(node, timeout_sec=0.1)
    except (KeyboardInterrupt, SystemExit):
        pass
    finally:
        if node is not None:
            node._stop()
            node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()
    if node is None or node.phase != "DONE":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
