"""PX4 SITL offboard mission for ONE simulated x500 (verified on PX4 v1.17.0 SITL gz_x500, target_mode=local; geo mode not yet flown).

Targets PX4 v1.17 + px4_msgs ``release/1.17`` over uXRCE-DDS (``MicroXRCEAgent udp4 -p 8888``).
Pattern follows PX4's official ``px4_ros_com`` offboard example:

* Pre-stream: ``OffboardControlMode`` + ``TrajectorySetpoint`` at 10 Hz (PX4 needs > 2 Hz
  "proof of life") for >= 1 s BEFORE requesting Offboard mode and arming.
* If the stream stops for longer than ``COM_OF_LOSS_T`` PX4 leaves Offboard and runs the
  ``COM_OBL_RC_ACT`` failsafe.  This node keeps streaming from a timer and, if PX4 leaves
  Offboard for any reason (failsafe / operator), it stops and does NOT re-request it.
* ``TrajectorySetpoint`` is NED metres in PX4's local frame (z negative = up).

Target selection (``target_mode``):
* ``local`` (default): fixed NED offset ``local_target_ned`` from the local origin. Use with
  stock SITL, whose home is PX4's default (not Portugal).
* ``geo``: project the incident lat/lon into local NED using ``ref_lat/ref_lon`` from
  ``VehicleLocalPosition`` (same projection as PX4).  Only meaningful if PX4 was started with
  ``PX4_HOME_LAT/PX4_HOME_LON`` near the incident; refused beyond ``max_geo_range_m``.

Topic names: versioned messages get a ``_vN`` suffix (VehicleLocalPosition v1, VehicleStatus
v1 in 1.17).  Check ``ros2 topic list`` and override parameters if your PX4 differs.
"""

from __future__ import annotations

import json
import math
import time

import rclpy
from rclpy.node import Node
from rclpy.qos import DurabilityPolicy, HistoryPolicy, QoSProfile, ReliabilityPolicy
from std_msgs.msg import String

try:
    from px4_msgs.msg import OffboardControlMode, TrajectorySetpoint, VehicleCommand, VehicleLocalPosition, VehicleStatus
except ImportError as exc:  # pragma: no cover - depends on external workspace
    raise SystemExit(
        "px4_msgs not found. Build px4_msgs (branch release/1.17, matching PX4 v1.17) in a "
        "workspace outside this repo and source it first. See ros2/README.md."
    ) from exc

from .geo import geodetic_to_local_ned
from .mission import MissionError, fetch_snapshot, load_snapshot_file, mission_from_snapshot


class Px4OffboardMission(Node):
    def __init__(self) -> None:
        super().__init__("emberwatch_px4_offboard_mission")
        p = self.declare_parameter
        p("snapshot_file", "")
        p("api_base", "")
        p("run_id", "synthetic-portugal")
        p("incident_id", "")
        p("target_mode", "local")
        p("local_target_ned", [20.0, 30.0])
        p("max_geo_range_m", 3000.0)
        p("altitude_m", 10.0)
        p("arrive_radius_m", 1.0)
        p("water_drop_s", 6.0)
        p("topic_local_position", "/fmu/out/vehicle_local_position_v1")
        p("topic_vehicle_status", "/fmu/out/vehicle_status_v1")
        gp = lambda n: self.get_parameter(n).value  # noqa: E731

        try:
            snap = load_snapshot_file(gp("snapshot_file")) if gp("snapshot_file") else fetch_snapshot(gp("api_base"), gp("run_id"))
            self.mission = mission_from_snapshot(snap, gp("incident_id") or None)
        except (MissionError, OSError, ValueError) as exc:
            self.get_logger().error(f"no mission: {exc}")
            raise SystemExit(2) from exc

        self.target_mode = gp("target_mode")
        self.local_target = [float(v) for v in gp("local_target_ned")]
        self.max_geo = float(gp("max_geo_range_m"))
        self.alt = float(gp("altitude_m"))
        self.r_arrive = float(gp("arrive_radius_m"))
        self.drop_s = float(gp("water_drop_s"))

        # /fmu/out is best-effort + transient-local (as in px4_ros_com); /fmu/in uses defaults.
        px4_qos = QoSProfile(reliability=ReliabilityPolicy.BEST_EFFORT, durability=DurabilityPolicy.TRANSIENT_LOCAL,
                             history=HistoryPolicy.KEEP_LAST, depth=1)
        latched = QoSProfile(depth=1, reliability=ReliabilityPolicy.RELIABLE, durability=DurabilityPolicy.TRANSIENT_LOCAL)
        self.ocm_pub = self.create_publisher(OffboardControlMode, "/fmu/in/offboard_control_mode", 10)
        self.sp_pub = self.create_publisher(TrajectorySetpoint, "/fmu/in/trajectory_setpoint", 10)
        self.cmd_pub = self.create_publisher(VehicleCommand, "/fmu/in/vehicle_command", 10)
        self.status_pub = self.create_publisher(String, "/emberwatch/mission_status", latched)
        self.create_subscription(VehicleLocalPosition, gp("topic_local_position"), self._on_lpos, px4_qos)
        self.create_subscription(VehicleStatus, gp("topic_vehicle_status"), self._on_status, px4_qos)

        self.lpos: VehicleLocalPosition | None = None
        self.vstatus: VehicleStatus | None = None
        self.target_ned: tuple[float, float, float] | None = None
        self.phase = "PRESTREAM"
        self.phase_t0 = time.monotonic()
        self.prestream_count = 0
        self.was_offboard = False
        self.create_timer(0.1, self._tick)  # 10 Hz
        self._status("mission accepted; pre-streaming setpoints")

    def _now_us(self) -> int:
        return self.get_clock().now().nanoseconds // 1000

    def _on_lpos(self, msg) -> None:
        self.lpos = msg

    def _on_status(self, msg) -> None:
        self.vstatus = msg

    def _status(self, note: str) -> None:
        m = self.mission
        body = {"label": m.label, "phase": self.phase, "note": note, "incident_id": m.incident_id,
                "data_mode": m.data_mode, "target_lon_lat": [m.target_lon, m.target_lat],
                "target_mode": self.target_mode, "target_ned": self.target_ned,
                "sources": [r.source_url for r in m.reports],
                "wind_speed_mps": m.conditions.wind_speed_mps, "wind_from_deg": m.conditions.wind_from_deg}
        self.status_pub.publish(String(data=json.dumps(body, ensure_ascii=False)))
        self.get_logger().info(f"[{self.phase}] {note}")

    def _set_phase(self, phase: str, note: str) -> None:
        self.phase, self.phase_t0 = phase, time.monotonic()
        self._status(note)

    def _command(self, command: int, param1: float = 0.0, param2: float = 0.0) -> None:
        msg = VehicleCommand()
        msg.command = command
        msg.param1, msg.param2 = float(param1), float(param2)
        msg.target_system = msg.target_component = 1
        msg.source_system = msg.source_component = 1
        msg.from_external = True
        msg.timestamp = self._now_us()
        self.cmd_pub.publish(msg)

    def _stream(self, ned: tuple[float, float, float]) -> None:
        ocm = OffboardControlMode()
        ocm.position, ocm.velocity, ocm.acceleration = True, False, False
        ocm.attitude, ocm.body_rate = False, False
        ocm.timestamp = self._now_us()
        self.ocm_pub.publish(ocm)
        sp = TrajectorySetpoint()
        sp.position = [float(v) for v in ned]
        nan = float("nan")
        sp.velocity = [nan, nan, nan]
        sp.acceleration = [nan, nan, nan]
        sp.yaw = nan  # let PX4 keep heading
        sp.timestamp = self._now_us()
        self.sp_pub.publish(sp)

    def _resolve_target(self) -> bool:
        if self.target_mode == "geo":
            if not (self.lpos and self.lpos.xy_global):
                return False
            n, e = geodetic_to_local_ned(self.mission.target_lat, self.mission.target_lon, self.lpos.ref_lat, self.lpos.ref_lon)
            if math.hypot(n, e) > self.max_geo:
                self._set_phase("ABORTED", f"geo target {math.hypot(n, e):.0f} m from PX4 origin > {self.max_geo} m; "
                                           "start PX4 with PX4_HOME_LAT/LON near the incident or use target_mode=local")
                return False
            self.target_ned = (n, e, -self.alt)
        else:
            self.target_ned = (self.local_target[0], self.local_target[1], -self.alt)
        return True

    def _dist(self, ned) -> float:
        lp = self.lpos
        return math.dist((lp.x, lp.y, lp.z), ned)

    def _tick(self) -> None:
        if self.phase in ("DONE", "ABORTED"):
            return
        offboard = bool(self.vstatus and self.vstatus.nav_state == VehicleStatus.NAVIGATION_STATE_OFFBOARD)
        armed = bool(self.vstatus and self.vstatus.arming_state == VehicleStatus.ARMING_STATE_ARMED)
        if self.was_offboard and not offboard and self.phase in ("TAKEOFF", "TRANSIT", "SIM_WATER_DROP"):
            self._set_phase("ABORTED", "PX4 left Offboard (failsafe or operator); not re-requesting")
            return
        self.was_offboard = self.was_offboard or offboard

        if self.lpos is None or not self.lpos.xy_valid or not self.lpos.z_valid:
            if self.phase == "PRESTREAM" and time.monotonic() - self.phase_t0 > 30:
                self._set_phase("ABORTED", "no valid VehicleLocalPosition after 30 s (agent running? topic names?)")
            return
        home_hold = (self.lpos.x, self.lpos.y, -self.alt)

        if self.phase == "PRESTREAM":
            if self.target_ned is None and not self._resolve_target():
                return
            self.home_xy = (self.lpos.x, self.lpos.y)
            self._stream((self.home_xy[0], self.home_xy[1], -self.alt))
            self.prestream_count += 1
            if self.prestream_count == 10:  # >= 1 s of setpoints before the mode switch
                self._command(VehicleCommand.VEHICLE_CMD_DO_SET_MODE, 1.0, 6.0)  # custom mode 6 = Offboard
                self._command(VehicleCommand.VEHICLE_CMD_COMPONENT_ARM_DISARM, 1.0)
                self._set_phase("ARMING", "requested Offboard + arm")
        elif self.phase == "ARMING":
            self._stream((self.home_xy[0], self.home_xy[1], -self.alt))
            if offboard and armed:
                self._set_phase("TAKEOFF", f"armed in Offboard; climbing to {self.alt} m")
            elif time.monotonic() - self.phase_t0 > 10:
                self._set_phase("ABORTED", "PX4 did not enter Offboard+armed within 10 s (preflight checks?)")
        elif self.phase == "TAKEOFF":
            sp = (self.home_xy[0], self.home_xy[1], -self.alt)
            self._stream(sp)
            if self._dist(sp) < self.r_arrive:
                self._set_phase("TRANSIT", f"transit to target NED {tuple(round(v, 1) for v in self.target_ned)}")
        elif self.phase == "TRANSIT":
            self._stream(self.target_ned)
            if self._dist(self.target_ned) < self.r_arrive:
                self._set_phase("SIM_WATER_DROP", "ARRIVED - SIMULATED water drop (status only; no payload modelled)")
        elif self.phase == "SIM_WATER_DROP":
            self._stream(self.target_ned)
            if time.monotonic() - self.phase_t0 > self.drop_s:
                self._command(VehicleCommand.VEHICLE_CMD_NAV_RETURN_TO_LAUNCH)
                self._set_phase("DONE", "simulated drop finished; commanded PX4 Return-to-Launch")
        else:
            self._stream(home_hold)


def main() -> None:
    rclpy.init()
    node = None
    try:
        node = Px4OffboardMission()
        while rclpy.ok() and node.phase not in ("DONE", "ABORTED"):
            rclpy.spin_once(node, timeout_sec=0.1)
    except (KeyboardInterrupt, SystemExit):
        pass
    finally:
        if node is not None:
            node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()


if __name__ == "__main__":
    main()
