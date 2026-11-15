"""Occupancy-grid A* + line-of-sight shortening (numpy/scipy, deterministic).

A grid cell is BLOCKED at a flight altitude z when (obstacle top + vertical clearance) > z or it
lies in the smoke hazard; blocked cells are dilated by the horizontal margin. The planner only
knows these modelled obstacles - this is planned avoidance, not sensing or physics.
"""

from __future__ import annotations

import heapq
import math

import numpy as np
from scipy import ndimage


class Grid:
    def __init__(self, x0: float, y0: float, res: float, blocked: np.ndarray) -> None:
        self.x0, self.y0, self.res, self.blocked = x0, y0, res, blocked  # blocked[row=y, col=x]

    def cell(self, x: float, y: float) -> tuple[int, int]:
        return int(round((y - self.y0) / self.res)), int(round((x - self.x0) / self.res))

    def pos(self, rc: tuple[int, int]) -> tuple[float, float]:
        return self.x0 + rc[1] * self.res, self.y0 + rc[0] * self.res

    def inside(self, rc) -> bool:
        return 0 <= rc[0] < self.blocked.shape[0] and 0 <= rc[1] < self.blocked.shape[1]

    def free_xy(self, x: float, y: float) -> bool:
        rc = self.cell(x, y)
        return self.inside(rc) and not self.blocked[rc]

    def segment_free(self, a, b) -> bool:
        n = max(2, int(math.ceil(math.dist(a, b) / (self.res * 0.5))))
        return all(self.free_xy(a[0] + (b[0] - a[0]) * i / n, a[1] + (b[1] - a[1]) * i / n) for i in range(n + 1))


def make_grid(x0, y0, res, top: np.ndarray, z: float, v_clear: float, h_margin: float,
              hazard_mask: np.ndarray | None = None) -> Grid:
    blocked = (top + v_clear) > z
    if hazard_mask is not None:
        blocked |= hazard_mask
    r = int(math.ceil(h_margin / res))
    if r > 0:
        yy, xx = np.mgrid[-r:r + 1, -r:r + 1]
        blocked = ndimage.binary_dilation(blocked, structure=(xx * xx + yy * yy) <= r * r)
    return Grid(x0, y0, res, blocked)


def astar(g: Grid, start, goal, max_expand: int = 3_000_000) -> list[tuple[float, float]] | None:
    s, t = g.cell(*start), g.cell(*goal)
    if not (g.inside(s) and g.inside(t)) or g.blocked[s] or g.blocked[t]:
        return None
    if g.segment_free(start, goal):
        return [tuple(start), tuple(goal)]
    B = g.blocked
    moves = [(1, 0, 1.0), (-1, 0, 1.0), (0, 1, 1.0), (0, -1, 1.0),
             (1, 1, 1.4142), (1, -1, 1.4142), (-1, 1, 1.4142), (-1, -1, 1.4142)]
    cost = {s: 0.0}
    came = {}
    q = [(0.0, s)]
    h = lambda c: math.hypot(c[0] - t[0], c[1] - t[1])  # noqa: E731
    n = 0
    while q:
        _, c = heapq.heappop(q)
        if c == t:
            cells = [c]
            while c in came:
                c = came[c]
                cells.append(c)
            raw = [tuple(start)] + [g.pos(k) for k in reversed(cells[1:-1])] + [tuple(goal)]
            return shorten(g, raw)
        n += 1
        if n > max_expand:
            return None
        cc = cost[c]
        for dr, dc, w in moves:
            nb = (c[0] + dr, c[1] + dc)
            if not (0 <= nb[0] < B.shape[0] and 0 <= nb[1] < B.shape[1]) or B[nb]:
                continue
            nc = cc + w
            if nc < cost.get(nb, math.inf):
                cost[nb] = nc
                came[nb] = c
                heapq.heappush(q, (nc + h(nb), nb))
    return None


def shorten(g: Grid, path):
    out = [path[0]]
    i = 0
    while i < len(path) - 1:
        j = len(path) - 1
        while j > i + 1 and not g.segment_free(path[i], path[j]):
            j -= 1
        out.append(path[j])
        i = j
    return out
