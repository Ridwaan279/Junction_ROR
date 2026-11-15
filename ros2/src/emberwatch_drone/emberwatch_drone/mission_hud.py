"""``mission_hud``: small always-on-top overlay window with an HH:MM:SS timer for screen recording.

Shows SIMULATION time (Gazebo ``/clock``, via the fire model's ``elapsed_s``) since the simulated
fire started, plus the mission phase, simulated fire intensity, burning canopy points and water
drops. Everything shown is simulated state access; it is labelled as such on the window.
"""

from __future__ import annotations

import json
import threading

import rclpy
from rclpy.executors import ExternalShutdownException
from rclpy.node import Node
from rclpy.qos import DurabilityPolicy, QoSProfile, ReliabilityPolicy
from std_msgs.msg import String


def hms(seconds: float) -> str:
    s = max(0, int(seconds))
    return f"{s // 3600:02d}:{s % 3600 // 60:02d}:{s % 60:02d}"


class HudNode(Node):
    def __init__(self) -> None:
        super().__init__("emberwatch_mission_hud")
        latched = QoSProfile(depth=1, reliability=ReliabilityPolicy.RELIABLE, durability=DurabilityPolicy.TRANSIENT_LOCAL)
        self.fire: dict = {}
        self.mission: dict = {}
        self.create_subscription(String, "/emberwatch/fire_state", lambda m: self._set("fire", m), latched)
        self.create_subscription(String, "/emberwatch/mission_status", lambda m: self._set("mission", m), latched)

    def _set(self, attr: str, msg: String) -> None:
        try:
            setattr(self, attr, json.loads(msg.data))
        except ValueError:
            pass


def main() -> None:
    import tkinter as tk

    rclpy.init()
    node = HudNode()
    th = threading.Thread(target=lambda: _spin(node), daemon=True)
    th.start()

    root = tk.Tk()
    root.title("EmberWatch SIMULATION timer")
    root.configure(bg="#111")
    root.attributes("-topmost", True)
    root.geometry("+30+30")
    lab = dict(bg="#111", fg="#ddd", font=("DejaVu Sans", 11))
    tk.Label(root, text="SIMULATION · simulated fire & drone", **{**lab, "fg": "#ff9a3c"}).pack(anchor="w", padx=14, pady=(10, 0))
    timer = tk.Label(root, text="00:00:00", bg="#111", fg="#fff", font=("DejaVu Sans Mono", 40, "bold"))
    timer.pack(anchor="w", padx=12)
    tk.Label(root, text="fire time (hh:mm:ss, sim clock)", **{**lab, "fg": "#999", "font": ("DejaVu Sans", 9)}).pack(anchor="w", padx=14)
    info = tk.Label(root, text="waiting for /emberwatch/fire_state …", justify="left", **{**lab, "font": ("DejaVu Sans Mono", 10)})
    info.pack(anchor="w", padx=14, pady=(6, 12))

    def refresh() -> None:
        f, m = node.fire, node.mission
        if f:
            timer.config(text=hms(float(f.get("elapsed_s", 0.0))), fg="#fff" if f.get("active") else "#7fd67f")
            lines = [f"Fire: {'BURNING' if f.get('active') else 'EXTINGUISHED (model)'}   "
                     f"intensity {float(f.get('intensity', 0)):.2f}",
                     f"Burning canopy points: {f.get('spread_points_burning', 0)} / {f.get('spread_points_total', 0)}",
                     f"Water applied: {float(f.get('water_applied_s', 0)):.0f} s"
                     f"{'   (dropping now)' if f.get('water_active') else ''}"]
            c = f.get("containment")
            if c:
                eta = float(c.get("firefighter_eta_s", 0)) - float(c.get("sim_s_since_ignition", 0))
                lines[2:2] = [("Ground crews ON SCENE (simulated handover)" if c.get("handover")
                               else f"Ground crews ETA (assumed): {hms(eta)}"),
                              f"Front held by wet fuel: {c.get('held_now', 0)} now · wet points {c.get('wet_points', 0)}"
                              f" · unburned {c.get('unburned_points', 0)}"]
            if m and m.get("fleet"):
                lines.insert(0, f"Containment fleet ({len(m['fleet'])} drones) - {m.get('phase', '?')}")
                for d in m["fleet"]:
                    lines.append(f"  D{d['id']}: {d['phase']:<10} water {d['water_left_s']:>4.0f} s  "
                                 f"wetted {d['targets_wetted']}  refills {d['refills']}")
            elif m:
                lines.insert(0, f"Drone: {m.get('phase', '?')}   drops {len(m.get('drops', []))}")
            info.config(text="\n".join(lines))
        if rclpy.ok():
            root.after(250, refresh)
        else:
            root.destroy()

    refresh()
    try:
        root.mainloop()
    except KeyboardInterrupt:
        pass
    finally:
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()


def _spin(node: Node) -> None:
    try:
        rclpy.spin(node)
    except (KeyboardInterrupt, ExternalShutdownException):
        pass


if __name__ == "__main__":
    main()
