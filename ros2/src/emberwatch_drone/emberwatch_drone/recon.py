"""Pure helpers for the representative, simulation-only camera demo."""
from __future__ import annotations

import math
import xml.etree.ElementTree as ET
from pathlib import Path


def make_world(source: Path, output: Path) -> None:
    """Extend the existing kinematic drone with a real rendered oblique camera.

    The scene is representative, not georeferenced. No existing asset is changed.
    """
    tree = ET.parse(source)
    world = tree.getroot().find("world")
    ET.SubElement(world, "plugin", filename="gz-sim-sensors-system",
                  name="gz::sim::systems::Sensors").append(ET.fromstring("<render_engine>ogre2</render_engine>"))
    link = world.find("model[@name='ember_drone']/link[@name='base_link']")
    link.append(ET.fromstring('''<sensor name="recon_camera" type="camera">
      <pose>0 0 -0.25 0 0.807 0.78539816339</pose><always_on>true</always_on>
      <update_rate>2</update_rate><topic>/emberwatch/recon/camera</topic>
      <camera><horizontal_fov>1.57079632679</horizontal_fov>
      <image><width>640</width><height>480</height><format>R8G8B8</format></image>
      <clip><near>0.1</near><far>500</far></clip></camera></sensor>'''))
    # Resolve original local texture paths after moving the generated SDF.
    for node in world.iter("albedo_map"):
        node.text = str((source.parent / node.text).resolve())
    output.parent.mkdir(parents=True, exist_ok=True)
    tree.write(output, encoding="utf-8", xml_declaration=True)


def route_error(task: dict) -> str | None:
    """A bounded scene mapping, never an arbitrary geographic flight route."""
    if task.get("event_stale"):
        return "stale_event: event update is over 15 scenario-minutes old; operator must review"
    pos = task.get("incident", {}).get("position")
    if not isinstance(pos, list) or len(pos) != 2 or not all(isinstance(x, (int, float)) and math.isfinite(x) for x in pos):
        return "unreachable_target: coordinates unknown"
    if not (-9.6 <= pos[0] <= -6.1 and 36.9 <= pos[1] <= 42.2):
        return "unreachable_target: outside mainland Portugal"
    if task.get("simulation") is not True or task.get("incident", {}).get("mode") != "synthetic":
        return "no_route: this demo accepts only explicitly synthetic events"
    return None


class ReconFlight:
    """Kinematic recon from current odometry, no water, no inferred detection.

    All distances in local scene metres. Cruise clears the existing visual trees.
    This bounded empty-air corridor is not an obstacle-avoidance planner.
    """
    def __init__(self, observe_s: float = 15.0):
        self.state = "assigned"
        self.phase = "idle"
        self.note = "Awaiting operator assignment / A aguardar missão"
        self.home = None
        self.goal = None
        self.observe_s = observe_s
        self.phase_at = 0.0

    def assign(self, task: dict, pos: tuple, now: float, blocked: bool = False):
        self.home = tuple(pos)
        error = route_error(task) or ("no_route: demonstration corridor blocked" if blocked else None)
        if error:
            self.state, self.note = "failed", error
            return
        self.state, self.phase = "travelling", "climb"
        self.goal = (pos[0], pos[1], 12.0)
        self.phase_at = now
        self.note = "Kinematic flight to representative scene fire; no geographic route"

    def step(self, pos: tuple | None, now: float, odom_fresh: bool, recall: bool = False) -> tuple:
        if self.state in ("assigned", "complete", "failed"):
            return (0., 0., 0.)
        if pos is None or not odom_fresh:
            self.state, self.note = "failed", "stale_telemetry: zero velocity; odometry unavailable"
            return (0., 0., 0.)
        if now - self.phase_at > 90:
            self.state, self.note = "failed", "unreachable_target: phase timed out; holding zero velocity"
            return (0., 0., 0.)
        if recall and self.state != "returning":
            self._return(now)
        if self.state == "observing":
            if now - self.phase_at >= self.observe_s:
                self._return(now)
            else:
                return self._velocity(pos)
        if math.dist(pos, self.goal) < .35:
            if self.phase == "climb":
                self.goal, self.phase = (0., -4., 18.), "transit"
            elif self.phase == "transit":
                self.state, self.phase = "observing", "observe"
                self.note = "Observing rendered scene; simulator state only, no image-based detector"
            elif self.phase == "return":
                self.goal, self.phase = self.home, "land"
            elif self.phase == "land":
                self.state, self.note = "complete", "Returned to captured start position (simulation)"
                return (0., 0., 0.)
            self.phase_at = now
        return self._velocity(pos)

    def _return(self, now: float):
        self.state, self.phase, self.phase_at = "returning", "return", now
        self.goal = (self.home[0], self.home[1], 12.)
        self.note = "Returning to captured start position / Regresso à posição inicial"

    def _velocity(self, pos: tuple) -> tuple:
        delta = tuple(.8 * (b - a) for a, b in zip(pos, self.goal))
        scale = max(1., math.sqrt(sum(x*x for x in delta)) / 4.)
        return tuple(x / scale for x in delta)
