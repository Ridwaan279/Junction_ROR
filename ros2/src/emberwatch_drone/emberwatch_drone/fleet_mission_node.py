"""``fleet_mission``: SIMULATED containment fleet of N PX4 SITL drones (first response until ground crews arrive).

Concept (all simulated, illustrative):
* N drones launch (staggered) from pads at the base and fly their planned, altitude-layered routes
  (terrain/buildings/trees/smoke-hazard planner, as for the single drone) to a staging point upwind
  of the fire.
* A coordinator assigns each on-station drone a point on the ACTIVE FRONT: unburned fuel whose parent
  point is burning, most urgent first, not already wetted, at least ``min_station_separation_m`` from
  other drones' targets. The drone hovers above the canopy just OUTSIDE the front (offset away from the
  burning parent) and sprays that point for ``spray_per_target_s``. Wet fuel cannot ignite in the
  fire model for ``wet_duration_s`` -> the fire is HELD, not extinguished.
* Each drone carries ``water_capacity_s`` of spray. When empty it flies (via staging) to a SIMULATED
  forward refill tank, refills for ``refill_s`` and returns. Staggered launches spread refills out.
* At the SIMULATED ground-crew ETA every drone returns along its planned route and lands.

Interfaces: /px4_<k>/fmu/{in,out}/... per drone (PX4 multi-instance, PX4_UXRCE_DDS_NS=px4_<k>),
``/emberwatch/water_drop`` (JSON with drone_id + target_enu), ``/emberwatch/fire_state`` (scenario-scoped),
``/emberwatch/mission_status`` (fleet summary). Nothing here is a validated firefighting tactic.
"""

from __future__ import annotations

import json
import math
import time

import rclpy
from rclpy.executors import ExternalShutdownException
from rclpy.node import Node
from rclpy.qos import DurabilityPolicy, HistoryPolicy, QoSProfile, ReliabilityPolicy
from std_msgs.msg import String

try:
    from px4_msgs.msg import OffboardControlMode, TrajectorySetpoint, VehicleCommand, VehicleLocalPosition, VehicleStatus
except ImportError as exc:  # pragma: no cover
    raise SystemExit(f"px4_msgs not available ({exc}); source the PX4 ROS 2 workspace") from exc

from .geo import geodetic_to_local_ned, yaw_enu_to_ned
from .hazard import Hazard
from .path_follow import PathFollower, YawLimiter

FLYING = ("TAKEOFF", "TRANSIT", "TO_STATION", "SPRAY", "TO_REFILL", "REFILL", "RTB")


class Drone:
    def __init__(self, node: "FleetMission", spec: dict) -> None:
        self.n, self.spec = node, spec
        self.id = int(spec["id"])
        ns = spec["namespace"]
        self.sys_id = int(spec["mav_sys_id"])
        self.pad = tuple(spec["pad_enu"])
        self.route = [tuple(p) for p in spec["route3d"]]
        self.station_z = float(spec["station_z"])
        self.refill_z = float(spec["refill_z"])
        px4_qos = QoSProfile(reliability=ReliabilityPolicy.BEST_EFFORT, durability=DurabilityPolicy.TRANSIENT_LOCAL,
                             history=HistoryPolicy.KEEP_LAST, depth=1)
        self.ocm = node.create_publisher(OffboardControlMode, f"{ns}/fmu/in/offboard_control_mode", 10)
        self.sp = node.create_publisher(TrajectorySetpoint, f"{ns}/fmu/in/trajectory_setpoint", 10)
        self.cmd = node.create_publisher(VehicleCommand, f"{ns}/fmu/in/vehicle_command", 10)
        node.create_subscription(VehicleLocalPosition, f"{ns}/fmu/out/vehicle_local_position_v1",
                                 lambda m: setattr(self, "lpos", m), px4_qos)
        node.create_subscription(VehicleStatus, f"{ns}/fmu/out/vehicle_status_v1",
                                 lambda m: setattr(self, "vstatus", m), px4_qos)
        self.lpos = self.vstatus = None
        self.off_ned = self.zoff = None
        self.phase, self.phase_t = "WAIT", time.monotonic()
        self.prestream = 0
        self.was_offboard = False
        self.follower: PathFollower | None = None
        self.legs: list = []  # queued straight legs [(x, y, z), ...] for station / refill moves
        self.yaw = YawLimiter(math.radians(40))
        self.water_s = float(node.fl["water_capacity_s"])
        self.target: int | None = None  # spread index being wetted
        self.hold_xy = None
        self.spray_t0 = None
        self.refill_t0 = None
        self.stats = {"targets_wetted": 0, "spray_s": 0.0, "refills": 0, "min_obstacle_vertical_clearance_m": None,
                      "obstacle_entered": False, "transit_hazard_entered": False}

    # ------------------------------------------------------------ helpers
    def px4_s(self) -> float:
        return self.lpos.timestamp * 1e-6 if self.lpos is not None else 0.0

    def enu(self):
        lp = self.lpos
        return (lp.y - self.off_ned[1], lp.x - self.off_ned[0], -lp.z + self.zoff)

    def ned(self, x, y, z):
        return (self.off_ned[0] + y, self.off_ned[1] + x, -(z - self.zoff))

    def stream(self, enu_xyz, heading=None) -> None:
        o = OffboardControlMode()
        o.position = True
        o.timestamp = self.n.now_us()
        self.ocm.publish(o)
        sp = TrajectorySetpoint()
        nan = float("nan")
        sp.position = [float(v) for v in self.ned(*enu_xyz)]
        sp.velocity = [nan] * 3
        sp.acceleration = [nan] * 3
        y = self.yaw.step(heading, 0.1)
        sp.yaw = nan if y is None else float(yaw_enu_to_ned(y))
        sp.timestamp = self.n.now_us()
        self.sp.publish(sp)

    def command(self, command, p1=0.0, p2=0.0) -> None:
        m = VehicleCommand()
        m.command, m.param1, m.param2 = command, float(p1), float(p2)
        m.target_system, m.target_component, m.source_system, m.source_component = self.sys_id, 1, 1, 1
        m.from_external = True
        m.timestamp = self.n.now_us()
        self.cmd.publish(m)

    def set_phase(self, phase: str, note: str = "") -> None:
        self.phase, self.phase_t = phase, time.monotonic()
        self.n.get_logger().info(f"[drone {self.id}] [{phase}] {note}")

    def water(self, active: bool) -> None:
        tgt = None
        if self.target is not None:
            q = self.n.spread[self.target]
            tgt = [q["x"], q["y"], q.get("canopy_z", q["z"])]
        self.n.water_pub.publish(String(data=json.dumps({
            "active": active, "drone_id": self.id, "pos_enu": list(self.enu()) if self.lpos else None,
            "target_enu": tgt, "scenario_id": self.n.scenario_id})))

    def go_legs(self, pts) -> None:
        here = self.enu()
        self.follower = PathFollower([here] + [tuple(p) for p in pts], 10.0)

    def follow(self) -> bool:
        """Advance along the current follower; True when at its end."""
        pos = self.enu()
        sp, heading, rem = self.follower.update(pos)
        self.stream(sp, heading)
        return rem < 0.5 and math.dist(pos, self.follower.pts[-1]) < 2.0

    # ------------------------------------------------------------ state machine
    def tick(self) -> None:
        n = self.n
        st = self.vstatus
        offboard = bool(st and st.nav_state == VehicleStatus.NAVIGATION_STATE_OFFBOARD)
        armed = bool(st and st.arming_state == VehicleStatus.ARMING_STATE_ARMED)
        if self.phase in ("DONE", "ABORTED"):
            return
        if self.was_offboard and not offboard and self.phase in FLYING:
            if self.target is not None:
                self.water(False)
            self.set_phase("ABORTED", "PX4 left Offboard (failsafe/operator); not re-requesting")
            return
        self.was_offboard |= offboard
        lp = self.lpos
        if lp is None or not (lp.xy_valid and lp.z_valid and lp.xy_global):
            return
        if self.off_ned is None:
            o = n.sc["origin"]
            self.off_ned = geodetic_to_local_ned(o["lat"], o["lon"], lp.ref_lat, lp.ref_lon)
            self.zoff = float(lp.ref_alt) - float(o.get("alt_m", 0.0))
            if self.yaw.yaw is None and math.isfinite(lp.heading):
                self.yaw.yaw = math.pi / 2 - float(lp.heading)
        pos = self.enu()
        if self.phase in FLYING:
            n.check_track(self, pos)
        if self.phase == "WAIT":
            if time.monotonic() >= n.t0 + n.stagger_s * self.id:
                self.set_phase("PRESTREAM", "pre-streaming setpoints")
        elif self.phase == "PRESTREAM":
            self.stream((self.pad[0], self.pad[1], self.route[0][2]))
            self.prestream += 1
            if self.prestream == 10:
                self.command(VehicleCommand.VEHICLE_CMD_DO_SET_MODE, 1.0, 6.0)
                self.command(VehicleCommand.VEHICLE_CMD_COMPONENT_ARM_DISARM, 1.0)
                self.set_phase("ARMING", "requested Offboard + arm")
        elif self.phase == "ARMING":
            self.stream((self.pad[0], self.pad[1], self.route[0][2]))
            if offboard and armed:
                self.set_phase("TAKEOFF", f"climbing to z {self.route[0][2]:.1f}")
            elif time.monotonic() - self.phase_t > 15:
                self.set_phase("ABORTED", "PX4 did not enter Offboard+armed within 15 s")
        elif self.phase == "TAKEOFF":
            self.stream((self.pad[0], self.pad[1], self.route[0][2]))
            if math.dist(pos, (self.pad[0], self.pad[1], self.route[0][2])) < 1.5:
                self.follower = PathFollower(self.route, 10.0, 3.5, float(n.sc.get("hover_alt_z", 0)) + 5.0 + 5.0 * self.id)
                self.set_phase("TRANSIT", "following planned route to staging")
        elif self.phase == "TRANSIT":
            if self.follow():
                self.to_station()
        elif self.phase == "TO_STATION":
            if n.handover:
                return self.rtb()
            if self.follow():
                if self.target_ok():
                    self.start_spray()
                elif self.target is not None or time.monotonic() - self.phase_t > 3.0:
                    self.to_station()  # target ignited meanwhile, or re-check while holding
        elif self.phase == "SPRAY":
            q = n.spread[self.target]
            self.stream((self.hold_xy[0], self.hold_xy[1], self.station_z), math.atan2(q["y"] - self.hold_xy[1], q["x"] - self.hold_xy[0]))
            now = self.px4_s()
            dt = now - self.spray_last
            self.spray_last = now
            self.water_s -= dt
            self.stats["spray_s"] += dt
            n.wet_until[self.target] = n.fire_time() + n.fl["wet_duration_s"]
            if int(now * 2) != int((now - dt) * 2):
                self.water(True)  # refresh wetting at ~2 Hz while spraying
            if n.handover or self.water_s <= 0 or now - self.spray_t0 >= n.spray_per_target_s or not self.target_ok():
                self.water(False)
                self.stats["targets_wetted"] += 1
                n.release(self)
                if n.handover:
                    self.rtb()
                elif self.water_s <= 0:
                    self.to_refill()
                else:
                    self.to_station()
        elif self.phase == "TO_REFILL":
            if self.follow():
                self.refill_t0 = self.px4_s()
                self.set_phase("REFILL", f"SIMULATED refill at forward tank ({n.fl['refill']['refill_s']:.0f} s)")
        elif self.phase == "REFILL":
            t = n.fl["refill"]["enu"]
            self.stream((t[0], t[1], self.refill_z))
            if self.px4_s() - self.refill_t0 >= float(n.fl["refill"]["refill_s"]):
                self.water_s = float(n.fl["water_capacity_s"])
                self.stats["refills"] += 1
                if n.handover:
                    return self.rtb()
                s = n.fl["staging_enu"]
                self.go_legs([(t[0], t[1], self.station_z), (s[0], s[1], self.station_z)])
                self.set_phase("TRANSIT", "refilled; back to the front via staging")
        elif self.phase == "RTB":
            if self.follow():
                self.command(VehicleCommand.VEHICLE_CMD_NAV_LAND)
                self.set_phase("LANDING", "over pad; commanded PX4 land")
        elif self.phase == "LANDING":
            if not armed:
                self.set_phase("DONE", "landed and disarmed")

    def target_ok(self) -> bool:
        return self.target is not None and self.target not in self.n.burning

    def to_station(self) -> None:
        n = self.n
        if n.handover:
            return self.rtb()
        if self.water_s <= 1.0:
            return self.to_refill()
        pick = n.assign(self)
        pos = self.enu()
        if pick is None:  # nothing to wet right now: hold over staging
            s = n.fl["staging_enu"]
            self.go_legs([(pos[0], pos[1], self.station_z), (s[0], s[1], self.station_z)])
            self.set_phase("TO_STATION", "no unwetted front point available; holding at staging")
            self.target = None
            return
        self.target, self.hold_xy = pick
        self.go_legs([(pos[0], pos[1], self.station_z), (self.hold_xy[0], self.hold_xy[1], self.station_z)])
        self.set_phase("TO_STATION", f"to front point {self.target} (hold {self.hold_xy[0]:.0f},{self.hold_xy[1]:.0f})")

    def start_spray(self) -> None:
        self.spray_t0 = self.spray_last = self.px4_s()
        self.water(True)
        self.set_phase("SPRAY", f"SIMULATED wetting of front point {self.target}; water left {self.water_s:.0f} s")

    def to_refill(self) -> None:
        n = self.n
        pos, s, t = self.enu(), n.fl["staging_enu"], n.fl["refill"]["enu"]
        self.target = None
        self.go_legs([(pos[0], pos[1], self.station_z), (s[0], s[1], self.station_z), (t[0], t[1], self.station_z),
                      (t[0], t[1], self.refill_z)])
        self.set_phase("TO_REFILL", "water load used; to the SIMULATED forward refill tank")

    def rtb(self) -> None:
        n = self.n
        pos, s = self.enu(), n.fl["staging_enu"]
        self.target = None
        back = list(reversed(self.route)) + [(self.pad[0], self.pad[1], self.route[0][2])]
        self.follower = PathFollower([pos, (pos[0], pos[1], max(pos[2], self.station_z)),
                                      (s[0], s[1], max(pos[2], self.station_z))] + back, 10.0)
        self.set_phase("RTB", "SIMULATED ground crews on scene: handing over, returning along planned route")


class FleetMission(Node):
    def __init__(self) -> None:
        super().__init__("emberwatch_fleet_mission")
        p = self.declare_parameter
        p("scenario_file", "")
        p("summary_file", "")
        p("stagger_s", 8.0)
        p("spray_per_target_s", 6.0)
        gp = lambda k: self.get_parameter(k).value  # noqa: E731
        with open(gp("scenario_file"), encoding="utf-8") as f:
            self.sc = json.load(f)
        self.fl = self.sc["fleet"]
        self.summary_file = gp("summary_file")
        self.stagger_s = float(gp("stagger_s"))
        self.spray_per_target_s = float(gp("spray_per_target_s"))
        self.scenario_id = self.sc.get("scenario_id")
        self.spread = self.sc["fire"]["spread"]
        self.hazard = Hazard.from_dict(self.sc["hazard"]["shapes"])
        self.wet_until = [-1.0] * len(self.spread)
        self.burning: set[int] = set()
        self.fire_state: dict | None = None
        self.handover = False
        self.claims: dict[int, int] = {}  # drone id -> spread idx
        self.min_dist_burning = math.inf
        import numpy as np
        r = np.load(self.sc["obstacle_raster"])
        self.R = {k: r[k] for k in r.files}
        latched = QoSProfile(depth=1, reliability=ReliabilityPolicy.RELIABLE, durability=DurabilityPolicy.TRANSIENT_LOCAL)
        self.status_pub = self.create_publisher(String, "/emberwatch/mission_status", latched)
        self.water_pub = self.create_publisher(String, "/emberwatch/water_drop", 10)
        self.create_subscription(String, "/emberwatch/fire_state", self._on_fire, latched)
        self.drones = [Drone(self, d) for d in self.fl["drones"]]
        self.t0 = time.monotonic()
        self.done = False
        self.create_timer(0.1, self._tick)
        self.create_timer(1.0, self._status)
        self.get_logger().info(f"SIMULATED containment fleet of {len(self.drones)} drones; ground-crew ETA "
                               f"{self.fl['firefighter_eta_s']:.0f} sim-s after ignition (assumption)")

    def now_us(self) -> int:
        return self.get_clock().now().nanoseconds // 1000

    def fire_time(self) -> float:
        c = (self.fire_state or {}).get("containment") or {}
        return float(c.get("sim_s_since_ignition", 0.0))

    def _on_fire(self, m: String) -> None:
        try:
            d = json.loads(m.data)
        except ValueError:
            return
        if d.get("scenario_id") != self.scenario_id:
            return
        self.fire_state = d
        self.burning = set(d.get("ignited_idx", []))
        c = d.get("containment") or {}
        if c.get("handover") and not self.handover:
            self.handover = True
            self.get_logger().info("fire model reports SIMULATED ground-crew handover: fleet returning")

    # ------------------------------------------------------------ coordination
    def assign(self, drone: Drone):
        """Most urgent unwetted front point (parent burning), separated from other drones' targets."""
        t = self.fire_time()
        others = [self.spread[j] for k, j in self.claims.items() if k != drone.id]
        sep = float(self.fl["min_station_separation_m"])
        best = None
        for i, q in enumerate(self.spread):
            if i in self.burning or i in self.claims.values():
                continue
            par = q.get("parent", -1)
            if par >= 0 and par not in self.burning:
                continue  # not on the active front
            if self.wet_until[i] - t > 0.5 * float(self.fl["wet_duration_s"]):
                continue  # already well wetted
            if any(math.dist((q["x"], q["y"]), (o["x"], o["y"])) < sep for o in others):
                continue
            urgency = q["t_ignite_s"] - t + (0 if self.wet_until[i] < t else 20)
            if best is None or urgency < best[0]:
                best = (urgency, i)
        if best is None:
            return None
        i = best[1]
        q = self.spread[i]
        pq = self.spread[q["parent"]] if q.get("parent", -1) >= 0 else {"x": self.sc["fire"]["enu"][0],
                                                                         "y": self.sc["fire"]["enu"][1]}
        dx, dy = q["x"] - pq["x"], q["y"] - pq["y"]
        L = math.hypot(dx, dy) or 1.0
        off = float(self.fl["min_station_distance_to_burning_m"])
        hold = (q["x"] + dx / L * off, q["y"] + dy / L * off)  # just outside the front, spraying back onto it
        self.claims[drone.id] = i
        return i, hold

    def release(self, drone: Drone) -> None:
        self.claims.pop(drone.id, None)

    def check_track(self, d: Drone, pos) -> None:
        R = self.R
        res, x0, y0 = float(R["res"]), float(R["x0"]), float(R["y0"])
        c, r = int(round((pos[0] - x0) / res)), int(round((pos[1] - y0) / res))
        top = R["top"]
        if not (1 <= r < top.shape[0] - 1 and 1 <= c < top.shape[1] - 1):
            return
        if math.dist(pos[:2], d.pad[:2]) < 8 and pos[2] < d.pad[2] + 8:
            return  # take-off / landing column
        if d.phase == "REFILL" or (d.phase == "TO_REFILL" and math.dist(pos[:2], self.fl["refill"]["enu"][:2]) < 8):
            return  # SIMULATED tank hover column over open ground (checked level/clear by the planner)
        vc = pos[2] - float(top[r - 1:r + 2, c - 1:c + 2].max())
        cur = d.stats["min_obstacle_vertical_clearance_m"]
        d.stats["min_obstacle_vertical_clearance_m"] = round(vc if cur is None else min(cur, vc), 2)
        if vc < 0 and not d.stats["obstacle_entered"]:
            d.stats["obstacle_entered"] = True
            self.get_logger().warning(f"drone {d.id} BELOW a modelled obstacle/terrain top at {pos}")
        if d.phase == "TRANSIT" and self.hazard.distance(pos[:2]) < 0 and d.follower is not None and \
                d.follower.pts == [tuple(p) for p in d.route]:
            d.stats["transit_hazard_entered"] = True
        for i in self.burning:
            q = self.spread[i]
            self.min_dist_burning = min(self.min_dist_burning, math.dist(pos[:2], (q["x"], q["y"])))

    # ------------------------------------------------------------ loop / status
    def _tick(self) -> None:
        for d in self.drones:
            d.tick()
        if not self.done and all(d.phase in ("DONE", "ABORTED") for d in self.drones):
            self.done = True
            self._status()
            self.get_logger().info("fleet mission finished")
            rclpy.shutdown()

    def _status(self) -> None:
        c = (self.fire_state or {}).get("containment") or {}
        body = {"label": "SIMULATED PX4 SITL containment fleet - no real aircraft or dispatch",
                "phase": "DONE" if self.done else ("HANDOVER" if self.handover else "CONTAINMENT"),
                "fleet": [{"id": d.id, "phase": d.phase, "water_left_s": round(max(d.water_s, 0), 1),
                           "target": d.target, **d.stats} for d in self.drones],
                "drops": [], "containment": c,
                "min_horizontal_distance_to_burning_point_m": None if math.isinf(self.min_dist_burning)
                else round(self.min_dist_burning, 1)}
        self.status_pub.publish(String(data=json.dumps(body)))
        if self.done and self.summary_file:
            fs = self.fire_state or {}
            with open(self.summary_file, "w", encoding="utf-8") as f:
                json.dump({"final_phase": "DONE" if all(d.phase == "DONE" for d in self.drones) else "PARTIAL",
                           "fire_final": {k: fs.get(k) for k in ("active", "intensity", "spread_points_burning",
                                                                  "spread_points_total", "containment")},
                           **body}, f, indent=2)


def main() -> None:
    rclpy.init()
    node = FleetMission()
    try:
        rclpy.spin(node)
    except (KeyboardInterrupt, ExternalShutdownException):
        pass
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()


if __name__ == "__main__":
    main()
