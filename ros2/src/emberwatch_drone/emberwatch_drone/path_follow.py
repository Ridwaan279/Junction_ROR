"""Smooth route following for the simulated drone: look-ahead ("carrot") setpoints + rate-limited yaw.

Instead of commanding one waypoint at a time (PX4 then brakes at every corner and snaps its
heading), the mission streams a position setpoint that slides along the planned 3-D polyline
``lookahead_m`` ahead of the drone's projection onto it. The drone therefore flies the planned
path continuously and rounds corners. Corner cutting is bounded: the carrot always lies ON the
planned polyline, so the drone stays within roughly ``lookahead_m * (1 - cos(turn/2))`` of it;
the look-ahead is kept below the planner's horizontal obstacle margin for low-level legs.
"""

from __future__ import annotations

import math


def _sub(a, b):
    return tuple(x - y for x, y in zip(a, b))


class PathFollower:
    def __init__(self, points, lookahead_m: float = 8.0, low_lookahead_m: float | None = None,
                 low_z_below: float | None = None) -> None:
        pts = [tuple(float(v) for v in p) for p in points]
        # drop duplicate consecutive points (zero-length segments)
        self.pts = [pts[0]] + [p for i, p in enumerate(pts[1:], 1) if math.dist(p, pts[i - 1]) > 1e-6]
        self.cum = [0.0]
        for a, b in zip(self.pts, self.pts[1:]):
            self.cum.append(self.cum[-1] + math.dist(a, b))
        self.length = self.cum[-1]
        self.L = float(lookahead_m)
        self.L_low = float(low_lookahead_m if low_lookahead_m is not None else lookahead_m)
        self.low_z = low_z_below
        self.s = 0.0  # progress along the path (m), monotonic
        self.seg = 0

    def point_at(self, s: float):
        s = min(max(s, 0.0), self.length)
        i = 0
        while i < len(self.cum) - 2 and self.cum[i + 1] < s:
            i += 1
        a, b = self.pts[i], self.pts[min(i + 1, len(self.pts) - 1)]
        seg = self.cum[min(i + 1, len(self.cum) - 1)] - self.cum[i]
        t = 0.0 if seg <= 0 else (s - self.cum[i]) / seg
        return tuple(pa + (pb - pa) * t for pa, pb in zip(a, b))

    def tangent_at(self, s: float):
        s = min(max(s, 0.0), self.length)
        i = 0
        while i < len(self.cum) - 2 and self.cum[i + 1] <= s:
            i += 1
        a, b = self.pts[i], self.pts[min(i + 1, len(self.pts) - 1)]
        return _sub(b, a)

    def _project(self, pos):
        """Progress of the closest point on segments seg..seg+3 (never goes backwards)."""
        best_s, best_d = self.s, math.inf
        for i in range(self.seg, min(self.seg + 4, len(self.pts) - 1)):
            a, b = self.pts[i], self.pts[i + 1]
            ab = _sub(b, a)
            L2 = sum(v * v for v in ab)
            t = 0.0 if L2 == 0 else max(0.0, min(1.0, sum(p * q for p, q in zip(_sub(pos, a), ab)) / L2))
            q = tuple(pa + v * t for pa, v in zip(a, ab))
            d = math.dist(pos, q)
            if d < best_d - 1e-9:
                best_d, best_s = d, self.cum[i] + t * math.sqrt(L2)
        return best_s, best_d

    def update(self, pos):
        """Return (setpoint_xyz, travel_heading_rad_enu_or_None, remaining_m)."""
        s, _ = self._project(pos)
        self.s = max(self.s, s)
        while self.seg < len(self.pts) - 2 and self.cum[self.seg + 1] <= self.s:
            self.seg += 1
        L = self.L
        if self.low_z is not None and pos[2] < self.low_z:
            L = self.L_low
        target_s = min(self.s + L, self.length)
        sp = self.point_at(target_s)
        tx, ty = sp[0] - pos[0], sp[1] - pos[1]
        heading = math.atan2(ty, tx) if math.hypot(tx, ty) > 1.5 else None
        return sp, heading, self.length - self.s


class YawLimiter:
    """Rate-limits yaw changes so the airframe turns smoothly instead of snapping."""

    def __init__(self, max_rate_rad_s: float = math.radians(35.0)) -> None:
        self.rate = max_rate_rad_s
        self.yaw: float | None = None

    def step(self, target: float | None, dt: float) -> float | None:
        if target is None:
            return self.yaw
        if self.yaw is None:
            self.yaw = target
            return self.yaw
        err = (target - self.yaw + math.pi) % (2 * math.pi) - math.pi
        lim = self.rate * max(dt, 0.0)
        self.yaw = (self.yaw + max(-lim, min(lim, err)) + math.pi) % (2 * math.pi) - math.pi
        return self.yaw
