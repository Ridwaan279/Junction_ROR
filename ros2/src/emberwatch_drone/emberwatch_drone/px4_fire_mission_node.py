"""``px4_fire_mission``: event -> hazard-aware route -> hover + simulated water drop, on PX4 SITL.

Flow: load scenario -> plan a 2-D route from the launch site (ENU origin) to the safe point
around the simplified smoke hazard (planner.py) -> pre-stream Offboard setpoints -> arm ->
climb -> fly the route waypoints -> hover facing the fire -> SIMULATED water drop for
``drop_s`` -> read ``/emberwatch/fire_state`` (SIMULATED STATE ACCESS, not camera
recognition) -> optionally repeat -> fly the route back -> PX4 NAV_LAND at the launch site.

The avoidance happens only because the planner routes around the hazard shapes it was given;
smoke particles do not make PX4 avoid anything.  Whether the flown track actually stayed out
of the hazard is MEASURED from PX4's local position and reported (``min_clearance_m``).

Frames: route is ENU about the scenario origin; PX4 setpoints are NED about PX4's EKF origin
(``ref_lat/ref_lon``).  origin->ref offset is computed with the PX4 projection, then
NED = (off_n + y_enu, off_e + x_enu, -alt).
Failsafes: setpoints at 10 Hz (PX4 needs > 2 Hz; ``COM_OF_LOSS_T`` triggers ``COM_OBL_RC_ACT``).
If PX4 leaves Offboard the node aborts and does not fight it. NOTE: PX4's own RTL failsafe
flies straight home and is NOT aware of the hazard region.
"""

from __future__ import annotations

import json
import math
import time

import rclpy
from geometry_msgs.msg import PoseStamped
from nav_msgs.msg import Path
from rclpy.node import Node
from rclpy.qos import DurabilityPolicy, HistoryPolicy, QoSProfile, ReliabilityPolicy
from std_msgs.msg import String

try:
    from px4_msgs.msg import OffboardControlMode, TrajectorySetpoint, VehicleCommand, VehicleLocalPosition, VehicleStatus
except ImportError as exc:  # pragma: no cover
    raise SystemExit("px4_msgs not found: source ~/px4/ros2_px4_ws/install/setup.bash (see ros2/README.md)") from exc

from . import gz_util as gz
from .geo import geodetic_to_local_ned, yaw_enu_to_ned
from .hazard import Hazard
from .planner import PlanningError, plan


class Px4FireMission(Node):
    def __init__(self) -> None:
        super().__init__("emberwatch_px4_fire_mission")
        p = self.declare_parameter
        p("scenario_file", "")
        p("summary_file", "")
        p("world", "emberwatch_px4")
        p("spawn_route_markers", True)
        p("arrive_radius_m", 1.0)
        p("waypoint_radius_m", 1.5)
        p("drop_s", 8.0)
        p("max_drops", 2)
        p("marker_radius_m", 0.35)
        p("fire_state_timeout_s", 3.0)
        p("topic_local_position", "/fmu/out/vehicle_local_position_v1")
        p("topic_vehicle_status", "/fmu/out/vehicle_status_v1")
        gp = lambda n: self.get_parameter(n).value  # noqa: E731
        with open(gp("scenario_file"), encoding="utf-8") as f:
            self.sc = json.load(f)
        self.summary_file = gp("summary_file")
        self.world = gp("world")
        self.r_arrive = float(gp("arrive_radius_m"))
        self.r_wp = float(gp("waypoint_radius_m"))
        self.drop_s = float(gp("drop_s"))
        self.max_drops = int(gp("max_drops"))
        self.marker_r = float(gp("marker_radius_m"))
        self.fs_timeout = float(gp("fire_state_timeout_s"))
        self.scenario_id = self.sc.get("scenario_id")
        self.alt = float(self.sc["cruise_alt_m"])
        self.zoff = None  # PX4 EKF origin altitude - scenario origin altitude
        self.fire = tuple(self.sc["fire"]["enu"])
        self.hazard = Hazard.from_dict(self.sc["hazard"]["shapes"])
        self.margin = float(self.sc["hazard"]["planning_margin_m"])

        latched = QoSProfile(depth=1, reliability=ReliabilityPolicy.RELIABLE, durability=DurabilityPolicy.TRANSIENT_LOCAL)
        px4_qos = QoSProfile(reliability=ReliabilityPolicy.BEST_EFFORT, durability=DurabilityPolicy.TRANSIENT_LOCAL,
                             history=HistoryPolicy.KEEP_LAST, depth=1)
        self.status_pub = self.create_publisher(String, "/emberwatch/mission_status", latched)
        self.path_pub = self.create_publisher(Path, "/emberwatch/route", latched)
        self.water_pub = self.create_publisher(String, "/emberwatch/water_drop", 10)
        self.ocm_pub = self.create_publisher(OffboardControlMode, "/fmu/in/offboard_control_mode", 10)
        self.sp_pub = self.create_publisher(TrajectorySetpoint, "/fmu/in/trajectory_setpoint", 10)
        self.cmd_pub = self.create_publisher(VehicleCommand, "/fmu/in/vehicle_command", 10)
        self.create_subscription(VehicleLocalPosition, gp("topic_local_position"), self._on_lpos, px4_qos)
        self.create_subscription(VehicleStatus, gp("topic_vehicle_status"), self._on_status, px4_qos)
        self.create_subscription(String, "/emberwatch/fire_state", self._on_fire, latched)

        self.lpos = None
        self.vstatus = None
        self.fire_state: dict | None = None
        self.fire_state_t = 0.0
        self.off_ned: tuple[float, float] | None = None
        self.phase = "PLAN"
        self.phase_t0 = time.monotonic()
        self.result: dict = {"drops": [], "hazard_entered": False, "min_clearance_m": None}
        self.track_min = math.inf
        self.prestream = 0
        self.was_offboard = False
        self.wp_i = 0
        self.drops = 0

        self.start = tuple(self.sc.get("start_enu", (0.0, 0.0, 0.0)))
        self.obstacles = None
        if self.sc.get("no_path"):
            self._set_phase("NO_PATH", f"scenario has no safe path ({self.sc.get('stage')}: {self.sc.get('reason')}); "
                                       "NOT arming (no straight-line fallback)")
            return
        if "route3d" in self.sc:  # pre-planned 3-D route (terrain/buildings/trees/smoke), e.g. Arganil
            self.route = [tuple(p) for p in self.sc["route3d"]]
            ri = self.sc.get("route_info", {})
            direct_clear = bool(ri.get("direct_cruise_line_clear")) and bool(ri.get("direct_low_line_clear"))
            self.result["plan"] = {"waypoints_enu": self.sc["route3d"], "direct_line_clear": direct_clear,
                                   "detour": not direct_clear, "length_m": ri.get("length_m"), "route_info": ri}
            if self.sc.get("obstacle_raster"):
                import numpy as np
                r = np.load(self.sc["obstacle_raster"])
                self.obstacles = {k: r[k] for k in r.files}
                self.result.update({"obstacle_entered": False, "min_obstacle_vertical_clearance_m": None,
                                    "start_to_fire_m": self.sc.get("start_distance_m")})
        else:
            try:
                res = plan((0.0, 0.0), tuple(self.sc["safe_point_enu"]), self.hazard, self.margin)
            except PlanningError as exc:
                res, why = None, str(exc)
            else:
                why = "planner found no path around the hazard"
            if res is None:
                self._set_phase("NO_PATH", f"{why}; NOT arming (no straight-line fallback)")
                return
            self.route = [(x, y, self.alt) for x, y in res.waypoints]
            direct_clear, res_detour = res.direct_clear, res.detour
            self.result["plan"] = {"waypoints_enu": [[round(a, 2), round(b, 2)] for a, b in res.waypoints],
                                   "direct_line_clear": res.direct_clear, "detour": res.detour,
                                   "length_m": round(res.length_m, 2), "direct_length_m": round(res.direct_length_m, 2)}
        self.track_xy: list = []
        self._publish_path()
        if gp("spawn_route_markers"):
            self._spawn_markers()
        self._set_phase("PRESTREAM", f"route ({len(self.route)} waypoints, detour={self.result['plan']['detour']}, "
                                     f"direct line clear={self.result['plan']['direct_line_clear']}); pre-streaming")
        self.create_timer(0.1, self._tick)
        self.create_timer(1.0, lambda: self._status("tick"))

    # ------------------------------------------------------------- io helpers
    def _on_lpos(self, m) -> None:
        self.lpos = m

    def _on_status(self, m) -> None:
        self.vstatus = m

    def _on_fire(self, m: String) -> None:
        try:
            d = json.loads(m.data)
        except ValueError:
            return
        if d.get("scenario_id") != self.scenario_id:  # stale/foreign fire_sim: never trust it
            self.foreign_fire_msgs = getattr(self, "foreign_fire_msgs", 0) + 1
            return
        self.fire_state = d
        self.fire_state_t = time.monotonic()

    def _px4_s(self) -> float:
        """PX4 time (lockstep SITL => simulation time) from the latest local position."""
        return self.lpos.timestamp * 1e-6 if self.lpos is not None else 0.0

    def _now_us(self) -> int:
        return self.get_clock().now().nanoseconds // 1000

    def _enu(self) -> tuple[float, float, float] | None:
        if self.lpos is None or self.off_ned is None:
            return None
        return (self.lpos.y - self.off_ned[1], self.lpos.x - self.off_ned[0], -self.lpos.z + self.zoff)

    def _ned(self, x: float, y: float, z: float) -> tuple[float, float, float]:
        """Scenario ENU (z = world z) -> PX4 local NED about the EKF origin."""
        return (self.off_ned[0] + y, self.off_ned[1] + x, -(z - self.zoff))

    def _status(self, note: str) -> None:
        pos = self._enu()
        fs = self.fire_state
        body = {"label": "SIMULATED PX4 SITL mission - no real aircraft or dispatch", "phase": self.phase,
                "note": note, "drone_enu": [round(v, 2) for v in pos] if pos else None,
                "fire_enu": list(self.fire), "safe_point_enu": self.sc["safe_point_enu"],
                "wind": self.sc["wind"], "hazard_mode": self.sc["hazard"]["mode"],
                "fire_state_simulated_access": ({k: fs[k] for k in ("active", "intensity", "radius_m")} if fs else None),
                "sources": [r["source_url"] for r in self.sc["mission"]["reports"]], **self.result}
        self.status_pub.publish(String(data=json.dumps(body)))
        if note != "tick":
            self.get_logger().info(f"[{self.phase}] {note}")

    def _set_phase(self, phase: str, note: str) -> None:
        self.phase, self.phase_t0 = phase, time.monotonic()
        self.phase_sim_t0 = self._px4_s()
        if phase == "TAKEOFF":
            self.flight_t0 = (time.monotonic(), self._px4_s())
        if phase == "DONE" and getattr(self, "flight_t0", None):
            wall, sim = time.monotonic() - self.flight_t0[0], self._px4_s() - self.flight_t0[1]
            self.result["flight_sim_s"], self.result["flight_wall_s"] = round(sim, 1), round(wall, 1)
            self.result["real_time_factor"] = round(sim / wall, 2) if wall > 0 else None
        self._status(note)
        if phase in ("DONE", "ABORTED", "NO_PATH") and self.summary_file:
            with open(self.summary_file, "w", encoding="utf-8") as f:
                json.dump({"final_phase": phase, "note": note, **self.result}, f, indent=2)

    def _publish_path(self) -> None:
        msg = Path()
        msg.header.frame_id = "emberwatch_enu"
        for x, y, z in self.route:
            ps = PoseStamped()
            ps.header.frame_id = "emberwatch_enu"
            ps.pose.position.x, ps.pose.position.y, ps.pose.position.z = x, y, z
            msg.poses.append(ps)
        self.path_pub.publish(msg)

    def _spawn_markers(self) -> None:
        vis = "".join(
            f'<visual name="wp{i}"><pose>{gz.pose(x, y, z)}</pose><geometry><sphere><radius>{self.marker_r}</radius>'
            f'</sphere></geometry><material><ambient>0.1 0.9 0.2 1</ambient><diffuse>0.1 0.9 0.2 1</diffuse>'
            f'<emissive>0 0.5 0 1</emissive></material></visual>' for i, (x, y, z) in enumerate(self.route)
            if 0 < i < len(self.route) - 1 or len(self.route) <= 2)  # keep start/hover point clear of markers
        gz.spawn_sdf(self.world, f'<sdf version="1.9"><model name="planned_route"><static>true</static><link name="l">'
                                 f'{vis}</link></model></sdf>', "planned_route")

    def _check_obstacles(self, pos) -> None:
        """Measured (not assumed) clearance of the flown track vs the modelled obstacle raster:
        vertical clearance above the tallest obstacle/terrain top within 1 cell; 'entered' if below it."""
        R = self.obstacles
        res, x0, y0 = float(R["res"]), float(R["x0"]), float(R["y0"])
        c, r = int(round((pos[0] - x0) / res)), int(round((pos[1] - y0) / res))
        top = R["top"]
        if not (1 <= r < top.shape[0] - 1 and 1 <= c < top.shape[1] - 1):
            return
        near_start = math.dist(pos[:2], self.start[:2]) < 15.0 and pos[2] < self.start[2] + 8.0
        if near_start:  # take-off / landing column over the launch point
            return
        vc = pos[2] - float(top[r - 1:r + 2, c - 1:c + 2].max())
        cur = self.result["min_obstacle_vertical_clearance_m"]
        self.result["min_obstacle_vertical_clearance_m"] = round(vc if cur is None else min(cur, vc), 2)
        if vc < 0 and not self.result["obstacle_entered"]:
            self.result["obstacle_entered"] = True
            self.get_logger().warning(f"drone BELOW a modelled obstacle/terrain top at {pos}")
        if not self.track_xy or math.dist(self.track_xy[-1], pos[:2]) > 25:
            self.track_xy.append([round(pos[0], 1), round(pos[1], 1)])

    def _command(self, command: int, p1: float = 0.0, p2: float = 0.0) -> None:
        m = VehicleCommand()
        m.command, m.param1, m.param2 = command, float(p1), float(p2)
        m.target_system = m.target_component = m.source_system = m.source_component = 1
        m.from_external = True
        m.timestamp = self._now_us()
        self.cmd_pub.publish(m)

    def _stream(self, ned, yaw: float = float("nan")) -> None:
        o = OffboardControlMode()
        o.position, o.velocity, o.acceleration, o.attitude, o.body_rate = True, False, False, False, False
        o.timestamp = self._now_us()
        self.ocm_pub.publish(o)
        sp = TrajectorySetpoint()
        nan = float("nan")
        sp.position = [float(v) for v in ned]
        sp.velocity = [nan] * 3
        sp.acceleration = [nan] * 3
        sp.yaw = float(yaw)
        sp.timestamp = self._now_us()
        self.sp_pub.publish(sp)

    def _water(self, active: bool) -> None:
        pos = self._enu()
        self.water_pub.publish(String(data=json.dumps({"active": active, "pos_enu": list(pos) if pos else None,
                                                       "scenario_id": self.scenario_id})))

    def _at(self, x: float, y: float, alt: float, r: float) -> bool:
        pos = self._enu()
        return pos is not None and math.dist(pos, (x, y, alt)) < r

    # ------------------------------------------------------------- state machine
    def _tick(self) -> None:
        if self.phase in ("DONE", "ABORTED", "NO_PATH"):
            return
        st = self.vstatus
        offboard = bool(st and st.nav_state == VehicleStatus.NAVIGATION_STATE_OFFBOARD)
        armed = bool(st and st.arming_state == VehicleStatus.ARMING_STATE_ARMED)
        flying = self.phase in ("TAKEOFF", "FOLLOW_ROUTE", "HOVER_DROP", "RETURN_ROUTE")
        if self.was_offboard and not offboard and flying:
            self._water(False)
            self._set_phase("ABORTED", "PX4 left Offboard (failsafe/operator); not re-requesting. "
                                       "PX4 RTL is NOT hazard-aware")
            return
        self.was_offboard |= offboard
        lp = self.lpos
        if lp is None or not (lp.xy_valid and lp.z_valid and lp.xy_global):
            if time.monotonic() - self.phase_t0 > 40:
                self._set_phase("ABORTED", "no valid global local-position from PX4 after 40 s")
            return
        if self.off_ned is None:
            o = self.sc["origin"]
            self.off_ned = geodetic_to_local_ned(o["lat"], o["lon"], lp.ref_lat, lp.ref_lon)
            self.zoff = float(lp.ref_alt) - float(o.get("alt_m", 0.0))
            self.get_logger().info(f"scenario origin is NED {tuple(round(v, 2) for v in self.off_ned)} from PX4 EKF "
                                   f"origin; EKF origin is {self.zoff:.2f} m above the scenario origin")

        pos = self._enu()
        if flying and self.obstacles is not None:
            self._check_obstacles(pos)
        if flying and pos[2] > self.start[2] + 1.0:
            d = self.hazard.distance(pos[:2])
            self.track_min = min(self.track_min, d)
            self.result["min_clearance_m"] = round(self.track_min, 2)
            if d < 0 and not self.result["hazard_entered"]:
                self.result["hazard_entered"] = True
                self.get_logger().warning(f"drone ENTERED the hazard region at {pos}")

        fx, fy = self.fire
        if self.phase == "PRESTREAM":
            self._stream(self._ned(self.start[0], self.start[1], self.route[0][2]))
            self.prestream += 1
            if self.prestream == 10:
                self._command(VehicleCommand.VEHICLE_CMD_DO_SET_MODE, 1.0, 6.0)
                self._command(VehicleCommand.VEHICLE_CMD_COMPONENT_ARM_DISARM, 1.0)
                self._set_phase("ARMING", "requested Offboard + arm")
        elif self.phase == "ARMING":
            self._stream(self._ned(self.start[0], self.start[1], self.route[0][2]))
            if offboard and armed:
                self._set_phase("TAKEOFF", f"armed in Offboard; climbing to z {self.route[0][2]:.1f} m (world)")
            elif time.monotonic() - self.phase_t0 > 10:
                self._set_phase("ABORTED", "PX4 did not enter Offboard+armed within 10 s")
        elif self.phase == "TAKEOFF":
            self._stream(self._ned(self.start[0], self.start[1], self.route[0][2]))
            if self._at(self.start[0], self.start[1], self.route[0][2], self.r_arrive):
                self.wp_i = 1
                self._set_phase("FOLLOW_ROUTE", f"following planned route to safe point {self.sc['safe_point_enu']}")
        elif self.phase == "FOLLOW_ROUTE":
            x, y, z = self.route[self.wp_i]
            self._stream(self._ned(x, y, z))
            last = self.wp_i == len(self.route) - 1
            if self._at(x, y, z, self.r_arrive if last else self.r_wp):
                if last:
                    self._start_drop()
                else:
                    self.wp_i += 1
                    self._status(f"waypoint {self.wp_i}/{len(self.route) - 1}")
                    self.get_logger().info(f"-> waypoint {self.wp_i}: {self.route[self.wp_i]}")
        elif self.phase == "HOVER_DROP":
            sx, sy, sz = self.route[-1]
            self._stream(self._ned(sx, sy, sz), yaw_enu_to_ned(math.atan2(fy - sy, fx - sx)))
            if self._px4_s() - self.phase_sim_t0 >= self.drop_s:  # simulation seconds
                self._water(False)
                self._after_drop()
        elif self.phase == "RETURN_ROUTE":
            x, y, z = self.ret[self.wp_i]
            self._stream(self._ned(x, y, z))
            if self._at(x, y, z, self.r_wp if self.wp_i < len(self.ret) - 1 else self.r_arrive):
                if self.wp_i == len(self.ret) - 1:
                    self._command(VehicleCommand.VEHICLE_CMD_NAV_LAND)
                    self._set_phase("LANDING", "over launch site; commanded PX4 land")
                else:
                    self.wp_i += 1
        elif self.phase == "LANDING":
            if not armed:
                self._set_phase("DONE", self._final_note())

    def _start_drop(self) -> None:
        self.drops += 1
        self._water(True)
        self._set_phase("HOVER_DROP", f"ARRIVED at safe point; hovering facing fire; SIMULATED water drop "
                                      f"#{self.drops} for {self.drop_s:.0f} sim-s")

    def _after_drop(self) -> None:
        fs = self.fire_state
        fresh = fs is not None and time.monotonic() - self.fire_state_t < self.fs_timeout
        rec = {"drop": self.drops, "fire_state_simulated_access": fs if fresh else None}
        self.result["drops"].append(rec)
        if not fresh:
            note = "fire state UNKNOWN (no fresh /emberwatch/fire_state); not claiming any effect"
        elif not fs["active"]:
            note = "simulated fire model reports EXTINGUISHED"
        elif self.drops < self.max_drops:
            self._water(True)
            self.phase_t0, self.phase_sim_t0 = time.monotonic(), self._px4_s()
            self.drops += 1
            self._status(f"simulated fire still active (intensity {fs['intensity']}); drop #{self.drops}")
            self.get_logger().info(f"[HOVER_DROP] fire still active; SIMULATED water drop #{self.drops}")
            return
        else:
            note = f"simulated fire still ACTIVE after {self.drops} drops (intensity {fs['intensity']})"
        self.result["fire_outcome"] = note
        # return: reverse the route, ending above the launch point at the first cruise altitude
        self.ret = list(reversed(self.route)) + [(self.start[0], self.start[1], self.route[0][2])]
        self.wp_i = 1 if len(self.ret) > 1 else 0
        self._set_phase("RETURN_ROUTE", f"{note}; returning along planned route")

    def _final_note(self) -> str:
        self.result["foreign_fire_state_msgs_ignored"] = getattr(self, "foreign_fire_msgs", 0)
        if self.track_xy:
            self.result["flown_track_xy_every_25m"] = self.track_xy
            far = max(math.dist(p, self.start[:2]) for p in self.track_xy)
            self.result["max_distance_from_start_m"] = round(far, 1)
        plan_ = self.result.get("plan", {})
        avoided = ((not plan_.get("direct_line_clear", True)) and not self.result["hazard_entered"]
                   and not self.result.get("obstacle_entered", False))
        self.result["avoidance_executed"] = avoided
        return (f"landed & disarmed. avoidance_executed={avoided} (direct line clear={plan_.get('direct_line_clear')}, "
                f"hazard entered={self.result['hazard_entered']}, min clearance={self.result['min_clearance_m']} m, "
                f"obstacle entered={self.result.get('obstacle_entered')}, min obstacle vertical clearance="
                f"{self.result.get('min_obstacle_vertical_clearance_m')} m, max distance from start="
                f"{self.result.get('max_distance_from_start_m')} m). "
                f"{self.result.get('fire_outcome', '')}")


def main() -> None:
    rclpy.init()
    node = Px4FireMission()
    code = 0
    try:
        while rclpy.ok() and node.phase not in ("DONE", "ABORTED", "NO_PATH"):
            rclpy.spin_once(node, timeout_sec=0.1)
        for _ in range(5):
            rclpy.spin_once(node, timeout_sec=0.05)
        code = {"DONE": 0, "NO_PATH": 3}.get(node.phase, 1)
    except KeyboardInterrupt:
        code = 130
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()
    raise SystemExit(code)


if __name__ == "__main__":
    main()
