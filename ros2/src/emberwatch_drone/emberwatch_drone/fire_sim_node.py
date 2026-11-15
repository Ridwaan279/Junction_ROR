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
from rclpy.qos import DurabilityPolicy, QoSProfile, ReliabilityPolicy
from std_msgs.msg import String

from . import gz_util as gz
from .fire_model import FireModel, FireModelConfig
from .hazard import downwind_unit

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
        self.t_start: float | None = None
        self.smoke_off = False

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
        if w["source"] == "unknown" or not w["speed_mps"]:
            sdir, sspeed = (0.0, 0.0, 1.0), SMOKE_RISE_MPS
        else:
            ux, uy = downwind_unit(w["from_deg"])
            ws = float(w["speed_mps"])
            sdir, sspeed = (ux * ws, uy * ws, SMOKE_RISE_MPS), math.hypot(ws, SMOKE_RISE_MPS)
        srpy = gz.direction_rpy(*sdir)
        self.smoke_rpy, self.smoke_speed = srpy, sspeed
        # Smoke particles live just long enough to drift the hazard plume length downwind, so
        # the visual and the planning region describe the same (simplified) plume.
        horiz = math.hypot(sdir[0], sdir[1])
        plume_len = float(self.sc["hazard"].get("plume_length_m", 25.0))
        smoke_life = round(min(12.0, plume_len / horiz), 2) if horiz > 0.1 else 9.0
        self.smoke_life = smoke_life
        r = self.model.radius_m
        flames = gz.emitter_sdf("flames", gz.pose(0, 0, 0.2, *gz.direction_rpy(0, 0, 1)), emitting=True, kind="box",
                                size=(2 * r, 2 * r, 0.2), particle_size=0.7, lifetime=1.2, vmin=0.8, vmax=1.8,
                                rate=self._flame_rate(), scale_rate=0.2, rgb=(1.0, 0.45, 0.05))
        smoke = gz.emitter_sdf("smoke", gz.pose(0, 0, 1.5, *srpy), emitting=True, kind="box",
                               size=(2.0, 2.0, 0.5), particle_size=1.4, lifetime=smoke_life,
                               vmin=sspeed * 0.8, vmax=sspeed * 1.2, rate=14, scale_rate=0.7, rgb=(0.33, 0.33, 0.33))
        fire_sdf = f"""<sdf version="1.9"><model name="sim_fire"><static>true</static><pose>{gz.pose(fx, fy, self.fire_z)}</pose>
<link name="fire_link">
<visual name="scorch"><pose>0 0 0.05 0 0 0</pose><geometry><cylinder><radius>{self.model.cfg.r_max_m}</radius><length>0.02</length></cylinder></geometry>
<material><ambient>0.08 0.06 0.05 1</ambient><diffuse>0.08 0.06 0.05 1</diffuse></material></visual>
{flames}
{smoke}
</link></model></sdf>"""
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
        self.get_logger().info(f"SIMULATED spread: point {i + 1}/{len(self.spread)} ignited at ({sp['x']}, {sp['y']})")
        if not self.visuals:
            return
        flames = gz.emitter_sdf("flames", gz.pose(0, 0, 0.2, *gz.direction_rpy(0, 0, 1)), emitting=True, kind="box",
                                size=(3.0, 3.0, 0.2), particle_size=0.7, lifetime=1.1, vmin=0.8, vmax=1.8, rate=45,
                                scale_rate=0.2, rgb=(1.0, 0.42, 0.05))
        smoke = ""
        if i % 3 == 2:
            smoke = gz.emitter_sdf("smoke", gz.pose(0, 0, 1.5, *self.smoke_rpy), emitting=True, kind="box",
                                   size=(2.0, 2.0, 0.5), particle_size=1.4, lifetime=self.smoke_life,
                                   vmin=self.smoke_speed * 0.8, vmax=self.smoke_speed * 1.2, rate=8, scale_rate=0.7,
                                   rgb=(0.33, 0.33, 0.33))
        gz.spawn_sdf(self.world, f"""<sdf version="1.9"><model name="sim_fire_spot_{i}"><static>true</static>
<pose>{gz.pose(sp['x'], sp['y'], sp['z'])}</pose><link name="fire_link">
<visual name="scorch"><pose>0 0 0.05 0 0 0</pose><geometry><cylinder><radius>2.6</radius><length>0.02</length></cylinder></geometry>
<material><ambient>0.07 0.05 0.04 1</ambient><diffuse>0.07 0.05 0.04 1</diffuse></material></visual>
{flames}{smoke}</link></model></sdf>""", f"sim_fire_spot_{i}")

    def _flame_rate(self) -> int:
        return int(10 + 70 * self.model.intensity) if self.model.active else 0

    def _update_visuals(self) -> None:
        if not self.visuals:
            return
        if self.model.active:
            rate = self._flame_rate()
            if self.last_rate is None or abs(rate - self.last_rate) >= 4:
                d = 2 * self.model.radius_m
                gz.emitter_cmd("sim_fire", "fire_link", "flames",
                               f"rate: {{data: {rate}}} size: {{x: {d:.2f}, y: {d:.2f}, z: 0.2}}")
                self.last_rate = rate
        else:
            if self.last_rate != 0:
                gz.emitter_cmd("sim_fire", "fire_link", "flames", "emitting: {data: false}")
                for i in self.ignited:  # the whole (illustrative) fire goes out together
                    gz.emitter_cmd(f"sim_fire_spot_{i}", "fire_link", "flames", "emitting: {data: false}")
                    if i % 3 == 2:
                        gz.emitter_cmd(f"sim_fire_spot_{i}", "fire_link", "smoke", "emitting: {data: false}")
                gz.emitter_cmd("sim_fire", "fire_link", "smoke", "rate: {data: 3}")
                self.last_rate = 0
            if not self.smoke_off and self.out_since and self._now() - self.out_since > 10.0:
                gz.emitter_cmd("sim_fire", "fire_link", "smoke", "emitting: {data: false}")
                gz.remove_model(self.world, "smoke_hazard_zone")  # visual only; planning used it already
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
        active = bool(d.get("active"))
        pos = d.get("pos_enu")
        if active and isinstance(pos, list) and len(pos) == 3:
            self.water_in_range = math.dist(pos[:2], self.fire) <= self.range_m
            if self.water is None and self.visuals:
                x, y, z = pos
                dx, dy, dz = self.fire[0] - x, self.fire[1] - y, -max(z - 0.5 - self.fire_z, 0.5)
                dist = math.sqrt(dx * dx + dy * dy + dz * dz)
                em = gz.emitter_sdf("water", gz.pose(0, 0, 0, *gz.direction_rpy(dx, dy, dz)), emitting=True,
                                    kind="cylinder", size=(0.5, 0.5, 0.2), particle_size=0.35,
                                    lifetime=round(dist / 7.0, 2), vmin=6.5, vmax=7.5, rate=120, scale_rate=0.6,
                                    rgb=(0.3, 0.6, 1.0))
                gz.spawn_sdf(self.world, f'<sdf version="1.9"><model name="sim_water_drop"><static>true</static>'
                                         f'<pose>{gz.pose(x, y, z - 0.5)}</pose><link name="l">{em}</link></model></sdf>',
                             "sim_water_drop")
            self.water = d
            self.get_logger().info(f"SIMULATED water drop ON at {pos} (in effective range: {self.water_in_range})")
        elif not active and self.water is not None:
            self.water = None
            self.water_in_range = False
            if self.visuals:
                gz.remove_model(self.world, "sim_water_drop")
            self.get_logger().info("SIMULATED water drop OFF")

    # ------------------------------------------------------------------ model
    def _now(self) -> float:
        return self.get_clock().now().nanoseconds * 1e-9

    def _step(self) -> None:
        now = self._now()
        was_active = self.model.active
        if self.t_start is None and now > 0:
            self.t_start = now
        if self.t_start is not None and self.model.active and self.model.intensity >= 0.25:
            for i, sp in enumerate(self.spread):
                if i not in self.ignited and now - self.t_start >= sp["t_ignite_s"]:
                    self.ignited.append(i)
                    self._spawn_spot(i, sp)
        dt = now - self.t_prev
        if dt < 0 or dt > 5.0:  # clock jump / first /clock message: do not integrate
            dt = 0.0
        self.model.step(dt, self.water is not None and self.water_in_range, math.sqrt(1 + len(self.ignited)))
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
                   "spread_points_total": len(self.spread)})
        self.pub.publish(String(data=json.dumps(st)))


def main() -> None:
    rclpy.init()
    node = FireSim()
    try:
        rclpy.spin(node)
    except KeyboardInterrupt:
        pass
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()


if __name__ == "__main__":
    main()
