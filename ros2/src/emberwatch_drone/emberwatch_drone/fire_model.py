"""Illustrative fire-state model. NOT validated wildfire spread or suppression physics.

intensity I in [0, 1]:
    dI/dt = growth_rate * I * (1 - I)            (logistic growth while active)
            - suppression_rate * [water in range] (simulated water drop)
Extinguished once I < extinguish_threshold; it then stays out (no re-ignition modelled).
Visual radius r = r_min + (r_max - r_min) * I.
Optional spread (fire_sim): pre-generated spread points ignite over time while intensity is high;
suppression is divided by sqrt(1 + ignited points). Illustrative only.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass
class FireModelConfig:
    initial_intensity: float = 0.35
    growth_rate: float = 0.02  # 1/s
    suppression_rate: float = 0.10  # intensity per second of in-range water
    extinguish_threshold: float = 0.05
    r_min_m: float = 1.0
    r_max_m: float = 4.0


class FireModel:
    def __init__(self, cfg: FireModelConfig) -> None:
        if not 0.0 < cfg.initial_intensity <= 1.0:
            raise ValueError("initial_intensity must be in (0, 1]")
        self.cfg = cfg
        self.intensity = cfg.initial_intensity
        self.active = True
        self.elapsed_s = 0.0
        self.water_s = 0.0

    @property
    def radius_m(self) -> float:
        c = self.cfg
        return c.r_min_m + (c.r_max_m - c.r_min_m) * (self.intensity if self.active else 0.0)

    def step(self, dt: float, water_in_range: bool, area_factor: float = 1.0) -> None:
        """area_factor >= 1: illustrative 'more burning area => each second of water does less'."""
        if dt <= 0:
            return
        self.elapsed_s += dt
        if not self.active:
            return
        c = self.cfg
        dI = c.growth_rate * self.intensity * (1.0 - self.intensity)
        if water_in_range:
            dI -= c.suppression_rate / max(1.0, area_factor)
            self.water_s += dt
        self.intensity = min(1.0, max(0.0, self.intensity + dI * dt))
        if self.intensity < c.extinguish_threshold:
            self.active = False
            self.intensity = 0.0

    def state(self) -> dict:
        return {"active": self.active, "intensity": round(self.intensity, 4),
                "radius_m": round(self.radius_m, 3), "elapsed_s": round(self.elapsed_s, 2),
                "water_applied_s": round(self.water_s, 2)}
