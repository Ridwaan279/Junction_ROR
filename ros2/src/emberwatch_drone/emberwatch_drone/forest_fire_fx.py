"""``forest_fire_fx``: large SIMULATED crown-fire visuals on the tree canopy (Gazebo particles only).

Visual layer only. The illustrative :class:`FireModel` inside ``fire_sim`` stays authoritative:
this node reads ``/emberwatch/fire_state`` (same ``scenario_id`` only) and the scenario's
pre-generated spread points (tree cells, ``canopy_z``), then:

* spawns a burning "crown" (dense, large flame sprites + flame tongues, wind-bent dark smoke,
  embers, orange glow light on some) on the canopy at each spread point once the model reports
  it burning (ignition order = ``t_ignite_s``);
* when the model intensity falls (simulated water), puts crowns out from the side nearest the
  drone's suppression point, leaving their smoke to drift a while longer;
* removes everything a short time after the model reports EXTINGUISHED.

Nothing here is fire physics; flame sizes, counts and smoke are illustrative art direction.
"""

from __future__ import annotations

import json
import math
import os
import subprocess
import tempfile
import time

import rclpy
from rclpy.executors import ExternalShutdownException
from rclpy.node import Node
from rclpy.qos import DurabilityPolicy, QoSProfile, ReliabilityPolicy
from std_msgs.msg import String

from . import gz_util as gz


def _smoke_ramp() -> str:
    """Original dark wildfire-smoke colour/alpha ramp over particle lifetime (written once, CC0)."""
    from pathlib import Path
    try:
        folder = Path(gz.fire_asset("flame.png")).parent
    except Exception:  # noqa: BLE001
        folder = Path(gz.PUFF_TEXTURE).parent
    out = folder / "crown_smoke_ramp_v1.png"
    if not out.exists():
        from PIL import Image
        keys = [(0.0, (48, 40, 32, 0)), (0.06, (46, 39, 32, 235)), (0.35, (70, 63, 55, 225)),
                (0.7, (112, 106, 98, 160)), (1.0, (150, 146, 140, 0))]
        im = Image.new("RGBA", (256, 8))
        for x in range(256):
            t = x / 255
            for (t0, c0), (t1, c1) in zip(keys, keys[1:]):
                if t0 <= t <= t1:
                    u = (t - t0) / (t1 - t0)
                    col = tuple(round(a + (b - a) * u) for a, b in zip(c0, c1))
                    break
            for y in range(8):
                im.putpixel((x, y), col)
        im.save(out)
    return str(out)


def _asset(name: str) -> str:
    try:
        return gz.fire_asset(name)
    except Exception:  # noqa: BLE001 - sprite generator unavailable: plain puff texture
        return gz.PUFF_TEXTURE


class ForestFireFx(Node):
    def __init__(self) -> None:
        super().__init__("emberwatch_forest_fire_fx")
        p = self.declare_parameter
        p("scenario_file", "")
        p("world", "emberwatch_arganil")
        p("density", 1.0)  # scales particle rates (lower it if the GUI frame rate drops)
        p("flame_scale", 1.0)  # scales flame sprite size
        p("smoke_linger_s", 25.0)
        p("max_spawn_per_tick", 6)
        gp = lambda n: self.get_parameter(n).value  # noqa: E731
        with open(gp("scenario_file"), encoding="utf-8") as f:
            self.sc = json.load(f)
        self.world = gp("world")
        self.density = float(gp("density"))
        self.fscale = float(gp("flame_scale"))
        self.linger = float(gp("smoke_linger_s"))
        self.max_spawn = int(gp("max_spawn_per_tick"))
        self.scenario_id = self.sc.get("scenario_id")
        fire = self.sc["fire"]
        fx, fy = fire["enu"][:2]
        seed = {"x": fx, "y": fy, "z": fire.get("ground_z", 0.0), "canopy_z": fire.get("canopy_z"), "t_ignite_s": 0.0}
        self.spread_by_idx = dict(enumerate(fire.get("spread", [])))
        pts = [seed] + sorted(fire.get("spread", []), key=lambda q: q["t_ignite_s"])
        self.pts = pts
        sx, sy = self.sc.get("safe_point_enu", (fx, fy))
        w = self.sc.get("wind", {})
        self.wind_from = w.get("from_deg") if w.get("source") != "unknown" else None
        self.wind_speed = float(w.get("speed_mps") or 0.0) if self.wind_from is not None else 0.0
        # suppression order: crowns nearest the drone's suppression point go out first
        self.out_order = sorted(range(len(pts)), key=lambda i: math.dist((pts[i]["x"], pts[i]["y"]), (sx, sy)))
        self.tex = {k: _asset(k) for k in ("flame.png", "flame_lifetime.png", "smoke_soft.png", "smoke_lifetime.png",
                                           "smoke.png", "ember.png", "ember_lifetime.png")}
        self.tex["crown_smoke_ramp"] = _smoke_ramp()
        self.flames_on: set[int] = set()
        self.smoke_on: dict[int, float | None] = {}  # idx -> time flames went out (None = still burning)
        self.queue: list[tuple[str, int]] = []  # ("spawn"|"remove"|"spawn_smoke"|"remove_smoke", idx)
        self.state: dict | None = None
        self.peak = 0.0
        self.out_t: float | None = None
        self.tmp = tempfile.mkdtemp(prefix="ember_crowns_")
        self.procs: list[subprocess.Popen] = []
        self.pending: dict[str, subprocess.Popen] = {}
        self.spawned = 0
        self.retries = 0
        self.create_timer(10.0, lambda: self.get_logger().info(
            f"crowns: flames {len(self.flames_on)}, smoke {len(self.smoke_on)}, spawned {self.spawned}, "
            f"spawn retries {self.retries}"))
        latched = QoSProfile(depth=1, reliability=ReliabilityPolicy.RELIABLE, durability=DurabilityPolicy.TRANSIENT_LOCAL)
        self.create_subscription(String, "/emberwatch/fire_state", self._on_state, latched)
        self.create_timer(0.5, self._tick)
        self.get_logger().info(f"SIMULATED crown-fire visuals: {len(pts)} canopy points, fuel={fire.get('fuel')}")

    # ---------------------------------------------------------------- SDF
    def _z(self, i: int) -> float:
        q = self.pts[i]
        cz = q.get("canopy_z")
        return float(cz) - 3.0 if cz is not None else float(q.get("z", 0.0)) + 0.5

    def _flame_sdf(self, i: int) -> str:
        import runpy
        from pathlib import Path
        source = Path(gz.PUFF_TEXTURE).parent / "fire/crown_visuals.py"
        if not source.is_file():
            source = Path(__file__).resolve().parent.parent / "worlds/media/fire/crown_visuals.py"
        return runpy.run_path(str(source))["crown_flame_sdf"](i, self.pts[i],
            density=self.density, flame_scale=self.fscale,
            wind_from_deg=self.wind_from, wind_speed_mps=self.wind_speed)

    def _lean_rpy(self, up_ratio: float):
        """Direction mostly up, leaning downwind (smoke/flame tongues bend with the simulated wind)."""
        if self.wind_from is None:
            return gz.direction_rpy(0, 0, 1)
        a = math.radians(self.wind_from)
        w = min(self.wind_speed, 12.0) / 12.0
        return gz.direction_rpy(-math.sin(a) * w, -math.cos(a) * w, max(up_ratio, 0.15))

    def _smoke_sdf(self, i: int) -> str:
        import runpy
        from pathlib import Path
        source = Path(gz.PUFF_TEXTURE).parent / "fire/crown_visuals.py"
        if not source.is_file():
            source = Path(__file__).resolve().parent.parent / "worlds/media/fire/crown_visuals.py"
        return runpy.run_path(str(source))["crown_smoke_sdf"](i, self.pts[i],
            density=self.density, flame_scale=self.fscale,
            wind_from_deg=self.wind_from, wind_speed_mps=self.wind_speed)

    # ---------------------------------------------------------------- gz service (batched, async)
    def _gz(self, args: list[str]) -> None:
        self.procs = [p for p in self.procs if p.poll() is None]
        self.procs.append(subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))

    def _spawn_many(self, items: list[tuple[str, str]]) -> None:
        """One checked create call per model; failures (server busy, timeout) are retried next tick."""
        for name, sdf in items:
            path = os.path.join(self.tmp, name + ".sdf")
            with open(path, "w", encoding="utf-8") as f:
                f.write(sdf)
            proc = subprocess.Popen(["gz", "service", "-s", f"/world/{self.world}/create", "--reqtype",
                                     "gz.msgs.EntityFactory", "--reptype", "gz.msgs.Boolean", "--timeout", "8000",
                                     "--req", f'sdf_filename: "{path}" name: "{name}" allow_renaming: false'],
                                    stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
            self.pending[name] = proc

    def _check_spawns(self) -> None:
        for name, proc in list(self.pending.items()):
            if proc.poll() is None:
                continue
            del self.pending[name]
            out = proc.stdout.read() if proc.stdout else ""
            if "true" in out:
                self.spawned += 1
                continue
            i = int(name.rsplit("_", 1)[1])
            self.retries += 1
            if name.startswith("crown_f_"):
                self.flames_on.discard(i)  # re-spawned on the next tick if still visible
            else:
                self.smoke_on.pop(i, None)

    def _remove(self, name: str) -> None:
        self._gz(["gz", "service", "-s", f"/world/{self.world}/remove", "--reqtype", "gz.msgs.Entity",
                  "--reptype", "gz.msgs.Boolean", "--timeout", "3000", "--req", f'name: "{name}" type: MODEL'])

    # ---------------------------------------------------------------- logic
    def _on_state(self, msg: String) -> None:
        try:
            d = json.loads(msg.data)
        except ValueError:
            return
        if d.get("scenario_id") != self.scenario_id:
            return
        self.state = d

    def _tick(self) -> None:
        st = self.state
        now = time.monotonic()
        self._check_spawns()
        if st is None or len(self.pending) > 2 * self.max_spawn:
            return
        active = bool(st.get("active"))
        intensity = float(st.get("intensity", 0.0))
        burning = int(st.get("spread_points_burning", 0)) + 1 if active else 0  # + seed crown
        self.peak = max(self.peak, intensity)
        if active:
            # Visible crowns: all ignited ones while the model is strong; fewer as water brings it down.
            frac = max(0.0, min(1.0, (intensity - 0.05) / 0.5))
            if self.peak > 0.55 or intensity < self.peak - 0.02:  # only thin out once it is being suppressed
                n_vis = math.ceil(burning * frac)
            else:
                n_vis = burning
            if "ignited_idx" in st:  # exact model state (pts[0] is the seed, pts[j + 1] is spread[j])
                order = {id(q): k for k, q in enumerate(self.pts)}
                ignited = [0] + [order[id(self.spread_by_idx[j])] for j in st["ignited_idx"] if j in self.spread_by_idx]
                burning = len(ignited)
                n_vis = n_vis if n_vis < burning else burning
            else:
                ignited = list(range(min(burning, len(self.pts))))
            out_first = [i for i in self.out_order if i in set(ignited)]
            vis = set(out_first[len(out_first) - n_vis:]) if n_vis > 0 else set()
        else:
            vis = set()
            if self.out_t is None:
                self.out_t = now
                self.get_logger().info("model EXTINGUISHED: removing SIMULATED crown flames; smoke lingers")
        spawns: list[tuple[str, str]] = []
        for i in sorted(vis - self.flames_on, key=lambda k: self.pts[k]["t_ignite_s"]):
            if len(spawns) >= self.max_spawn:
                break
            spawns.append((f"crown_f_{i}", self._flame_sdf(i)))
            self.flames_on.add(i)
            if i % 3 == 0 and i not in self.smoke_on:
                spawns.append((f"crown_s_{i}", self._smoke_sdf(i)))
                self.smoke_on[i] = None
            elif i in self.smoke_on:
                self.smoke_on[i] = None
        if spawns:
            self._spawn_many(spawns)
        for i in list(self.flames_on - vis):
            self._remove(f"crown_f_{i}")
            self.flames_on.discard(i)
            if i in self.smoke_on:
                self.smoke_on[i] = now
        for i, t_out in list(self.smoke_on.items()):
            if t_out is not None and now - t_out > self.linger:
                self._remove(f"crown_s_{i}")
                del self.smoke_on[i]

    def shutdown(self) -> None:
        for i in list(self.flames_on):
            self._remove(f"crown_f_{i}")
        for i in list(self.smoke_on):
            self._remove(f"crown_s_{i}")


def main() -> None:
    rclpy.init()
    node = ForestFireFx()
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
