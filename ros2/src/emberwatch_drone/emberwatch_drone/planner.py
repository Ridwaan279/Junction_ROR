"""2-D grid A* around a :class:`~emberwatch_drone.hazard.Hazard`, then line-of-sight shortening.

Pure Python, deterministic, no ROS.  Coordinates are local ENU metres.
Returns ``None`` when no path exists (callers must treat that as an explicit NO_PATH,
never fall back to flying straight).
"""

from __future__ import annotations

import heapq
import math
from dataclasses import dataclass

from .hazard import Hazard, Point


class PlanningError(ValueError):
    pass


@dataclass
class PlanResult:
    waypoints: list[Point]
    direct_clear: bool  # would the straight line have been clear?
    length_m: float
    direct_length_m: float
    detour: bool


def _shorten(path: list[Point], hazard: Hazard, margin: float) -> list[Point]:
    out = [path[0]]
    i = 0
    while i < len(path) - 1:
        j = len(path) - 1
        while j > i + 1 and not hazard.segment_clear(path[i], path[j], margin):
            j -= 1
        out.append(path[j])
        i = j
    return out


def plan(start: Point, goal: Point, hazard: Hazard, margin: float, resolution: float = 0.5,
         pad: float = 20.0, max_cells: int = 400_000) -> PlanResult | None:
    if hazard.blocked(start, margin):
        raise PlanningError(f"start {start} is inside the (inflated) hazard")
    if hazard.blocked(goal, margin):
        raise PlanningError(f"goal {goal} is inside the (inflated) hazard")
    direct_len = math.dist(start, goal)
    if hazard.segment_clear(start, goal, margin):
        return PlanResult([start, goal], True, direct_len, direct_len, False)

    # Grid bounds cover start, goal and every hazard shape (+pad).
    xs, ys = [start[0], goal[0]], [start[1], goal[1]]
    for s in hazard.shapes:
        if hasattr(s, "r"):
            xs += [s.cx - s.r, s.cx + s.r]
            ys += [s.cy - s.r, s.cy + s.r]
        else:
            xs += [p[0] for p in s.pts]
            ys += [p[1] for p in s.pts]
    x0, y0 = min(xs) - pad, min(ys) - pad
    nx = int(math.ceil((max(xs) + pad - x0) / resolution)) + 1
    ny = int(math.ceil((max(ys) + pad - y0) / resolution)) + 1
    if nx * ny > max_cells:
        raise PlanningError(f"planning area too large ({nx}x{ny} cells)")

    def cell(p: Point) -> tuple[int, int]:
        return round((p[0] - x0) / resolution), round((p[1] - y0) / resolution)

    def pos(c: tuple[int, int]) -> Point:
        return x0 + c[0] * resolution, y0 + c[1] * resolution

    free_cache: dict[tuple[int, int], bool] = {}

    def free(c: tuple[int, int]) -> bool:
        v = free_cache.get(c)
        if v is None:
            v = not hazard.blocked(pos(c), margin)
            free_cache[c] = v
        return v

    s, g = cell(start), cell(goal)
    moves = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)]
    openq = [(0.0, s)]
    came: dict[tuple[int, int], tuple[int, int]] = {}
    cost = {s: 0.0}
    while openq:
        _, c = heapq.heappop(openq)
        if c == g:
            cells = [c]
            while c in came:
                c = came[c]
                cells.append(c)
            raw = [start] + [pos(k) for k in reversed(cells[1:-1])] + [goal]
            wps = _shorten(raw, hazard, margin)
            length = sum(math.dist(a, b) for a, b in zip(wps, wps[1:]))
            return PlanResult(wps, False, length, direct_len, True)
        for dx, dy in moves:
            n = (c[0] + dx, c[1] + dy)
            if not (0 <= n[0] < nx and 0 <= n[1] < ny) or not free(n):
                continue
            nc = cost[c] + math.hypot(dx, dy) * resolution
            if nc < cost.get(n, math.inf):
                cost[n] = nc
                came[n] = c
                heapq.heappush(openq, (nc + math.dist(pos(n), goal), n))
    return None
