"""``fire_sim``: illustrative fire-state model + Gazebo visuals for ONE simulated fire.

* Spawns the fire (flames + wind-driven smoke particles), and the simplified hazard zone
  (translucent red volume) at the positions from the scenario file.
* Runs :class:`FireModel` (logistic growth, suppression only while an in-range simulated
  water drop is active) and publishes ``/emberwatch/fire_state`` (std_msgs/String JSON,
  transient-local).  Consumers reading this topic are using **simulated state access**,
  not camera-based fire recognition.
* Subscribes ``/emberwatch/water_drop`` (JSON ``{"active": bool, "pos_enu": [x, y, z]}``) and
  shows a labelled water-drop particle effect at that position.

Nothing here is validated wildfire, smoke or suppression physics.
"""

from __future__ import annotations

import json
import math
import time

import rclpy
from rclpy.node import Node
from rclpy.executors import ExternalShutdownException
from rclpy.qos import DurabilityPolicy, QoSProfile, ReliabilityPolicy
from std_msgs.msg import String

from . import gz_util as gz
from .fire_model import FireModel, FireModelConfig

SMOKE_RISE_MPS = 1.2


class FireSim(Node):
    def __init__(self) -> None:
        super().__init__("emberwatch_fire_sim")
        p = self.declare_parameter
        p("scenario_file", "")
        p("world", "emberwatch_px4")
        p("spawn_visuals", True)
        p("visual_update_s", 1.5)
        p("hazard_visual_height_m", 12.0)
        # Fire model time base: ROS clock. Run with use_sim_time:=true and /clock bridged from
        # Gazebo so growth/suppression follow SIMULATION time even when the sim runs slower
        # than real time.
        gp = lambda n: self.get_parameter(n).value  # noqa: E731
        with open(gp("scenario_file"), encoding="utf-8") as f:
            self.sc = json.load(f)
        self.world = gp("world")
        self.visuals = bool(gp("spawn_visuals"))
        self.vis_dt = float(gp("visual_update_s"))
        self.hz_h = float(gp("hazard_visual_height_m"))
        self.scenario_id = self.sc.get("scenario_id")
        self.model = FireModel(FireModelConfig(**self.sc["fire_model"]))
        self.fire = tuple(self.sc["fire"]["enu"][:2])
        self.fire_z = float(self.sc["fire"].get("ground_z", 0.0))  # terrain height at the fire (world z)
        self.range_m = float(self.sc["drop_effective_range_m"])
        self.water: dict | None = None
        self.water_in_range = False
        self.last_vis = 0.0
        self.last_rate = None
        self.out_since: float | None = None
        self.spread = list(self.sc["fire"].get("spread", []))
        self.ignited: list[int] = []
        # SIMULATED containment fleet: drones wet unburned fuel ahead of the front; wet points cannot
        # ignite. Spread only proceeds from a burning parent point. No extinguishing is modelled.
        self.fleet = self.sc.get("fleet")
        self.wet_until = [-1.0] * len(self.spread)
        self.blocked_events = 0
        self.held_now = 0
        self.waters: dict = {}  # drone id -> last active water message
        self.handover = False
        self.t_start: float | None = None
        self.smoke_off = False
        self.last_front = None
        self.burned_front = None
        self.marker_namespace = "emberwatch_fire_" + str(self.scenario_id or "unscoped")
        self.visual_controller = None

        latched = QoSProfile(depth=1, reliability=ReliabilityPolicy.RELIABLE, durability=DurabilityPolicy.TRANSIENT_LOCAL)
        self.pub = self.create_publisher(String, "/emberwatch/fire_state", latched)
        self.create_subscription(String, "/emberwatch/water_drop", self._on_water, 10)
        if self.visuals:
            self._spawn_scene()
        self.t_prev = self._now()
        self.sim_time = bool(self.get_parameter("use_sim_time").value)
        self.create_timer(0.1, self._step)
        self.create_timer(0.5, self._publish)
        self.get_logger().info(f"fire at ENU {self.fire}, wind {self.sc['wind']['source']}, model {self.model.state()}")

    # ------------------------------------------------------------------ visuals
    def _spawn_scene(self) -> None:
        fx, fy = self.fire
        w = self.sc["wind"]
        wind_from = w.get("from_deg") if w["source"] != "unknown" else None
        wind_speed = w.get("speed_mps") if w["source"] != "unknown" else None
        plume_len = float(self.sc["hazard"].get("plume_length_m", 25.0))
        self.smoke_rpy, self.smoke_speed, self.smoke_life = gz.smoke_motion(wind_from, wind_speed, plume_len)
        raster = self.sc.get("obstacle_raster")
        if raster is None and self.sc.get("world_dir"):
            from pathlib import Path
            raster = str(Path(self.sc["world_dir"]) / "obstacles.npz")
        self.visual_controller = gz.FireVisualController((*self.fire,self.fire_z),
            terrain_npz=raster,spread_points=self.spread,max_radius_m=self.model.cfg.r_max_m,
            wind_from_deg=wind_from,wind_speed_mps=wind_speed,plume_length_m=plume_len)
        self.visual_controller.namespace = self.marker_namespace
        # Compile the small native adapter once during startup, never in timer callbacks.
        self.visual_controller.transport = gz.ensure_fire_transport()
        fire_sdf = self.visual_controller.sdf(radius_m=self.model.radius_m,intensity=self.model.intensity)
        gz.spawn_sdf(self.world, fire_sdf, "sim_fire", wait=True)

        vis = []
        for i, s in enumerate(self.sc["hazard"]["shapes"]):
            mat = "<material><ambient>1.0 0.35 0.1 1</ambient><diffuse>1.0 0.35 0.1 1</diffuse><emissive>0.15 0.03 0 1</emissive></material><transparency>0.9</transparency>"
            if s["type"] == "circle":
                cx, cy = s["center_enu"]
                vis.append(f'<visual name="hz{i}"><pose>{gz.pose(cx, cy, self.fire_z - 3 + self.hz_h / 2)}</pose><geometry><cylinder>'
                           f'<radius>{s["radius_m"]}</radius><length>{self.hz_h}</length></cylinder></geometry>{mat}</visual>')
            else:
                pts = "".join(f"<point>{x:.3f} {y:.3f}</point>" for x, y in s["points_enu"])
                vis.append(f'<visual name="hz{i}"><pose>{gz.pose(0, 0, self.fire_z - 3)}</pose><geometry><polyline>{pts}<height>{self.hz_h}</height></polyline>'
                           f'</geometry>{mat}</visual>')
        hz_sdf = (f'<sdf version="1.9"><model name="smoke_hazard_zone"><static>true</static><link name="l">'
                  f'{"".join(vis)}</link></model></sdf>')
        gz.spawn_sdf(self.world, hz_sdf, "smoke_hazard_zone", wait=True)

    def _spawn_spot(self, i: int, sp: dict) -> None:
        # Seed ignition remains a model event, but never creates a new visual emitter.
        self.get_logger().info(f"SIMULATED spread: point {i + 1}/{len(self.spread)} joined the single front at ({sp['x']}, {sp['y']})")

    def _update_visuals(self) -> None:
        if not self.visuals or self.visual_controller is None:
            return
        smoke_expired = self.out_since is not None and self._now()-self.out_since > 10
        self.visual_controller.update(radius_m=self.model.radius_m,intensity=self.model.intensity,
            ignited=[self.spread[i] for i in self.ignited],active=self.model.active,world=self.world,
            elapsed_s=self.model.elapsed_s,smoke_expired=smoke_expired)
        if smoke_expired and not self.smoke_off:
            gz.remove_model(self.world,"smoke_hazard_zone")
            self.smoke_off = True

    def _on_water(self, msg: String) -> None:
        try:
            d = json.loads(msg.data)
        except ValueError:
            self.get_logger().warning("bad water_drop message ignored")
            return
        if d.get("scenario_id") != self.scenario_id:
            self.get_logger().warning(f"ignoring water_drop for other scenario {d.get('scenario_id')!r}")
            return
        did = d.get("drone_id", 0)
        active = bool(d.get("active"))
        pos = d.get("pos_enu")
        if active and isinstance(pos, list) and len(pos) == 3:
            target = d.get("target_enu") or list(self.fire)
            first = did not in self.waters
            self.waters[did] = d
            if self.fleet:  # wet the fuel around the drop target (simulated)
                now, r = self._now(), float(self.fleet["wet_radius_m"])
                for i, sp in enumerate(self.spread):
                    if i not in self.ignited and math.dist((sp["x"], sp["y"]), target[:2]) <= r:
                        self.wet_until[i] = now + float(self.fleet["wet_duration_s"])
            self.water_in_range = any(math.dist(w["pos_enu"][:2], self.fire) <= self.range_m for w in self.waters.values())
            if first and self.visuals:
                x, y, z = pos
                tz = self._ground_at(target)
                dx, dy, dz = target[0] - x, target[1] - y, -max(z - 0.5 - tz, 0.5)
                dist = math.sqrt(dx * dx + dy * dy + dz * dz)
                # Visual-only styling; per-drone state, wetting and spawn lifecycle stay unchanged.
                import runpy
                from pathlib import Path
                source = Path(gz.PUFF_TEXTURE).parent / "fire/crown_visuals.py"
                if not source.is_file():
                    source = Path(__file__).resolve().parent.parent / "worlds/media/fire/crown_visuals.py"
                em = runpy.run_path(str(source))["water_emitter_sdf"]((dx, dy, dz), dist)
                gz.spawn_sdf(self.world, f'<sdf version="1.9"><model name="sim_water_drop_{did}"><static>true</static>'
                                         f'<pose>{gz.pose(x, y, z - 0.5)}</pose><link name="l">{em}</link></model></sdf>',
                             f"sim_water_drop_{did}")
                self.get_logger().info(f"SIMULATED water drop ON (drone {did}) at {pos} -> target {target[:2]}")
        elif not active and did in self.waters:
            del self.waters[did]
            self.water_in_range = any(math.dist(w["pos_enu"][:2], self.fire) <= self.range_m for w in self.waters.values())
            if self.visuals:
                gz.remove_model(self.world, f"sim_water_drop_{did}")
            self.get_logger().info(f"SIMULATED water drop OFF (drone {did})")
        self.water = next(iter(self.waters.values()), None)

    def _ground_at(self, target) -> float:
        best = min([(math.dist((sp["x"], sp["y"]), target[:2]), sp.get("canopy_z", sp["z"])) for sp in self.spread]
                   or [(0.0, self.fire_z)])
        return float(best[1]) if best[0] < 15 else self.fire_z

    # ------------------------------------------------------------------ model
    def _now(self) -> float:
        return self.get_clock().now().nanoseconds * 1e-9

    def _step(self) -> None:
        now = self._now()
        was_active = self.model.active
        if self.t_start is None and now > 0:
            self.t_start = now
        if self.fleet and not self.handover and self.t_start is not None and \
                now - self.t_start >= float(self.fleet["firefighter_eta_s"]):
            self.handover = True
            self.get_logger().info("SIMULATED ground crews arrived (assumed ETA): spread model stopped, handover")
        if self.t_start is not None and self.model.active and self.model.intensity >= 0.25 and not self.handover:
            burning = set(self.ignited)
            held = 0
            for i, sp in enumerate(self.spread):
                if i in burning or now - self.t_start < sp["t_ignite_s"]:
                    continue
                par = sp.get("parent")
                if par is not None and par >= 0 and par not in burning:
                    continue  # fire only spreads from a burning neighbour
                if self.wet_until[i] > now:
                    held += 1
                    continue  # SIMULATED wetted fuel: held for now
                self.ignited.append(i)
                burning.add(i)
                self._spawn_spot(i, sp)
            if held > self.held_now:
                self.blocked_events += held - self.held_now
            self.held_now = held
        dt = now - self.t_prev
        if dt < 0 or dt > 5.0:  # clock jump / first /clock message: do not integrate
            dt = 0.0
        # Fleet mode never claims extinguishing: water only holds the spread (wet fuel ahead of the front).
        water_on_fire = (self.water is not None and self.water_in_range) and not self.fleet
        self.model.step(dt, water_on_fire, math.sqrt(1 + len(self.ignited)))
        self.t_prev = now
        if was_active and not self.model.active:
            self.out_since = now
            self.get_logger().info("SIMULATED fire marked EXTINGUISHED by the illustrative model")
        wall = time.monotonic()
        if wall - self.last_vis > self.vis_dt:
            self.last_vis = wall
            self._update_visuals()

    def _publish(self) -> None:
        st = self.model.state()
        st.update({"label": "SIMULATED fire state (illustrative model; simulated state access, not sensing)",
                   "fire_enu": list(self.fire), "time_base": "sim" if self.sim_time else "wall", "water_active": self.water is not None,
                   "water_in_effective_range": self.water_in_range, "wind": self.sc["wind"],
                   "incident_id": self.sc["mission"]["incident_id"], "scenario_id": self.scenario_id,
                   "spread_points_burning": len(self.ignited) if self.model.active else 0,
                   "spread_points_total": len(self.spread), "ignited_idx": list(self.ignited),
                   "drones_spraying": sorted(self.waters)})
        if self.fleet:
            now = self._now()
            pending = [i for i in range(len(self.spread)) if i not in set(self.ignited)]
            st.update({"containment": {"label": "SIMULATED containment (wet fuel holds the front); not extinguishing",
                                       "wet_points": sum(1 for i in pending if self.wet_until[i] > now),
                                       "held_now": self.held_now, "hold_events": self.blocked_events,
                                       "unburned_points": len(pending), "handover": self.handover,
                                       "firefighter_eta_s": self.fleet["firefighter_eta_s"],
                                       "sim_s_since_ignition": round(now - self.t_start, 1) if self.t_start else 0.0}})
        self.pub.publish(String(data=json.dumps(st)))


def main() -> None:
    rclpy.init()
    node = FireSim()
    try:
        rclpy.spin(node)
    except (KeyboardInterrupt, ExternalShutdownException):
        pass
    finally:
        gz.close_fire_transport()
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()


if __name__ == "__main__":
    main()
