"""2-D grid A* around a :class:`~emberwatch_drone.hazard.Hazard`, then line-of-sight shortening.

Pure Python, deterministic, no ROS.  Coordinates are local ENU metres.
Returns ``None`` when no path exists (callers must treat that as an explicit NO_PATH,
never fall back to flying straight).
"""

from __future__ import annotations

import heapq
import math
from dataclasses import dataclass

from .hazard import Circle, Hazard, Point

Cell = tuple[int, int]
NEIGHBOURS = ((1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1))


class PlanningError(ValueError):
    """A route cannot be planned with the supplied endpoints or grid bounds."""


@dataclass
class PlanResult:
    waypoints: list[Point]
    direct_clear: bool  # would the straight line have been clear?
    length_m: float
    direct_length_m: float
    detour: bool


def _shorten(path: list[Point], hazard: Hazard, margin: float) -> list[Point]:
    """Keep the farthest visible waypoint from each retained point."""
    shortened = [path[0]]
    current = 0
    while current < len(path) - 1:
        candidate = len(path) - 1
        while candidate > current + 1 and not hazard.segment_clear(
            path[current], path[candidate], margin
        ):
            candidate -= 1
        shortened.append(path[candidate])
        current = candidate
    return shortened


def plan(
    start: Point,
    goal: Point,
    hazard: Hazard,
    margin: float,
    resolution: float = 0.5,
    pad: float = 20.0,
    max_cells: int = 400_000,
) -> PlanResult | None:
    """Find a direct route or search a bounded grid, preserving deterministic tie order."""
    if hazard.blocked(start, margin):
        raise PlanningError(f"start {start} is inside the (inflated) hazard")
    if hazard.blocked(goal, margin):
        raise PlanningError(f"goal {goal} is inside the (inflated) hazard")
    direct_length = math.dist(start, goal)
    if hazard.segment_clear(start, goal, margin):
        return PlanResult(
            waypoints=[start, goal],
            direct_clear=True,
            length_m=direct_length,
            direct_length_m=direct_length,
            detour=False,
        )

    # Grid bounds cover start, goal and every hazard shape (+pad).
    xs, ys = [start[0], goal[0]], [start[1], goal[1]]
    for shape in hazard.shapes:
        if isinstance(shape, Circle):
            xs.extend((shape.cx - shape.r, shape.cx + shape.r))
            ys.extend((shape.cy - shape.r, shape.cy + shape.r))
        else:
            xs.extend(point[0] for point in shape.pts)
            ys.extend(point[1] for point in shape.pts)
    x0, y0 = min(xs) - pad, min(ys) - pad
    nx = int(math.ceil((max(xs) + pad - x0) / resolution)) + 1
    ny = int(math.ceil((max(ys) + pad - y0) / resolution)) + 1
    if nx * ny > max_cells:
        raise PlanningError(f"planning area too large ({nx}x{ny} cells)")

    def to_cell(point: Point) -> Cell:
        return round((point[0] - x0) / resolution), round((point[1] - y0) / resolution)

    def to_position(cell: Cell) -> Point:
        return x0 + cell[0] * resolution, y0 + cell[1] * resolution

    free_cache: dict[Cell, bool] = {}

    def is_free(cell: Cell) -> bool:
        if cell not in free_cache:
            free_cache[cell] = not hazard.blocked(to_position(cell), margin)
        return free_cache[cell]

    start_cell, goal_cell = to_cell(start), to_cell(goal)
    frontier = [(0.0, start_cell)]
    came_from: dict[Cell, Cell] = {}
    costs = {start_cell: 0.0}
    while frontier:
        _, current = heapq.heappop(frontier)
        if current == goal_cell:
            cells = [current]
            while current in came_from:
                current = came_from[current]
                cells.append(current)
            raw_path = [start] + [to_position(cell) for cell in reversed(cells[1:-1])] + [goal]
            waypoints = _shorten(raw_path, hazard, margin)
            length = sum(math.dist(a, b) for a, b in zip(waypoints, waypoints[1:]))
            return PlanResult(
                waypoints=waypoints,
                direct_clear=False,
                length_m=length,
                direct_length_m=direct_length,
                detour=True,
            )
        for dx, dy in NEIGHBOURS:
            neighbour = (current[0] + dx, current[1] + dy)
            if not (0 <= neighbour[0] < nx and 0 <= neighbour[1] < ny) or not is_free(neighbour):
                continue
            next_cost = costs[current] + math.hypot(dx, dy) * resolution
            if next_cost < costs.get(neighbour, math.inf):
                costs[neighbour] = next_cost
                came_from[neighbour] = current
                priority = next_cost + math.dist(to_position(neighbour), goal)
                heapq.heappush(frontier, (priority, neighbour))
    return None
