"""Deliberately simplified 2-D hazard regions in the local ENU frame (metres).

A hazard is a union of circles and convex polygons, treated as a full-height column
(altitude is ignored).  This is a planning abstraction for a demo, NOT a smoke dispersion,
visibility or sensing model, and the drone does not "see" smoke: it only avoids these shapes
because the planner was given them.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field

Point = tuple[float, float]


@dataclass(frozen=True)
class Circle:
    cx: float
    cy: float
    r: float

    def distance(self, p: Point) -> float:
        """Signed distance: negative inside."""
        return math.hypot(p[0] - self.cx, p[1] - self.cy) - self.r


@dataclass(frozen=True)
class ConvexPolygon:
    pts: tuple[Point, ...]  # counter-clockwise

    def distance(self, p: Point) -> float:
        inside = True
        best = math.inf
        n = len(self.pts)
        for i in range(n):
            a, b = self.pts[i], self.pts[(i + 1) % n]
            ex, ey = b[0] - a[0], b[1] - a[1]
            # CCW polygon: point is inside if left of every edge
            if ex * (p[1] - a[1]) - ey * (p[0] - a[0]) < 0:
                inside = False
            L2 = ex * ex + ey * ey
            t = 0.0 if L2 == 0 else max(0.0, min(1.0, ((p[0] - a[0]) * ex + (p[1] - a[1]) * ey) / L2))
            best = min(best, math.hypot(p[0] - (a[0] + t * ex), p[1] - (a[1] + t * ey)))
        return -best if inside else best


@dataclass
class Hazard:
    shapes: list = field(default_factory=list)

    def distance(self, p: Point) -> float:
        """Signed distance to the union (negative inside any shape)."""
        if not self.shapes:
            return math.inf
        return min(s.distance(p) for s in self.shapes)

    def blocked(self, p: Point, margin: float = 0.0) -> bool:
        return self.distance(p) < margin

    def segment_clear(self, a: Point, b: Point, margin: float = 0.0, step: float = 0.25) -> bool:
        n = max(1, int(math.ceil(math.dist(a, b) / step)))
        return all(not self.blocked((a[0] + (b[0] - a[0]) * i / n, a[1] + (b[1] - a[1]) * i / n), margin)
                   for i in range(n + 1))

    def to_dict(self) -> list[dict]:
        out = []
        for s in self.shapes:
            if isinstance(s, Circle):
                out.append({"type": "circle", "center_enu": [s.cx, s.cy], "radius_m": s.r})
            else:
                out.append({"type": "convex_polygon", "points_enu": [list(p) for p in s.pts]})
        return out

    @staticmethod
    def from_dict(items: list[dict]) -> "Hazard":
        shapes = []
        for it in items:
            if it["type"] == "circle":
                shapes.append(Circle(it["center_enu"][0], it["center_enu"][1], it["radius_m"]))
            elif it["type"] == "convex_polygon":
                shapes.append(ConvexPolygon(tuple(tuple(p) for p in it["points_enu"])))
            else:
                raise ValueError(f"unknown hazard shape {it['type']!r}")
        return Hazard(shapes)


def downwind_unit(wind_from_deg: float) -> Point:
    """ENU unit vector the wind blows TOWARD. wind_from_deg: meteorological, degrees
    clockwise from true North that the wind comes FROM (270 = westerly, blows to the east)."""
    to = math.radians(wind_from_deg + 180.0)
    return math.sin(to), math.cos(to)  # (east, north)


def plume_polygon(fire: Point, wind_from_deg: float, length_m: float, base_half_width_m: float,
                  spread_deg: float) -> ConvexPolygon:
    """Trapezoid from the fire extending ``length_m`` downwind, widening by ``spread_deg``."""
    ux, uy = downwind_unit(wind_from_deg)
    px, py = -uy, ux  # left-perpendicular
    w0 = base_half_width_m
    w1 = base_half_width_m + length_m * math.tan(math.radians(spread_deg))
    fx, fy = fire
    ex, ey = fx + ux * length_m, fy + uy * length_m
    # CCW: right-base, right-end, left-end, left-base
    pts = ((fx - px * w0, fy - py * w0), (ex - px * w1, ey - py * w1),
           (ex + px * w1, ey + py * w1), (fx + px * w0, fy + py * w0))
    return ConvexPolygon(pts)
