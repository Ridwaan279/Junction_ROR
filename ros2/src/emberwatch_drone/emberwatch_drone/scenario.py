"""Build ONE simulated fire scenario (local frame, wind, hazard, safe point) from a Mission.

Frames and origin
-----------------
* Simulator world frame = ENU metres, origin (0, 0, 0) = the **simulated launch site**,
  which is also PX4's home (``PX4_HOME_LAT/LON/ALT``) and the Gazebo world's
  ``spherical_coordinates`` origin.  The launch site is *not* a real base: it is placed at
  ``launch_from_fire_enu`` metres from the reported fire position (documented, configurable).
* The fire's world position is obtained by projecting the reported lat/lon about that origin
  (PX4 ``MapProjection`` maths, see geo.py).  PX4 NED = (north, east, -up).
* Altitude: terrain is NOT reconstructed; ground is a flat plane at elevation 0 (labelled).

Wind
----
``wind_from_deg`` is meteorological: degrees clockwise from true North the wind blows FROM
(270 = westerly).  ``wind_speed_mps`` in m/s.  Source is one of:
* ``feed``: an observed Weather item in the snapshot (station distance recorded),
* ``simulated_demo_value``: an explicit demo value (feed had none) — labelled simulated,
* ``unknown``: no value; the hazard becomes a conservative circle and smoke rises vertically.
The wind only drives the smoke visual and the hazard geometry; it is NOT applied to the
drone's flight dynamics.
"""

from __future__ import annotations

import math
from dataclasses import asdict, dataclass, field

from .fire_model import FireModelConfig
from .geo import geodetic_to_local_ned, local_ned_to_geodetic
from .hazard import Circle, Hazard, downwind_unit, plume_polygon
from .mission import Mission

SCENARIO_SCHEMA = "emberwatch.sim_scenario.v1"


class ScenarioError(ValueError):
    pass


@dataclass
class ScenarioConfig:
    launch_from_fire_enu: tuple[float, float] = (35.0, 12.0)
    max_sim_range_m: float = 150.0
    demo_wind_from_deg: float | None = None  # only used when the feed has no wind
    demo_wind_speed_mps: float | None = None
    plume_length_m: float = 25.0
    plume_spread_deg: float = 15.0
    safety_margin_m: float = 2.5
    standoff_m: float = 2.0
    cruise_alt_m: float = 10.0
    drop_effective_range_m: float = 10.0
    fire: FireModelConfig = field(default_factory=FireModelConfig)


def _wind(mission: Mission, cfg: ScenarioConfig) -> dict:
    c = mission.conditions
    if c.wind_from_deg is not None and c.wind_speed_mps is not None:
        return {"source": "feed", "from_deg": c.wind_from_deg, "speed_mps": c.wind_speed_mps,
                "valid_at": c.weather_valid_at, "station_id": c.weather_station_id,
                "station_distance_m": c.weather_station_distance_m, "source_url": c.weather_source_url}
    if cfg.demo_wind_from_deg is not None and cfg.demo_wind_speed_mps is not None:
        if cfg.demo_wind_speed_mps < 0:
            raise ScenarioError("demo wind speed must be >= 0")
        return {"source": "simulated_demo_value", "from_deg": cfg.demo_wind_from_deg % 360.0,
                "speed_mps": cfg.demo_wind_speed_mps,
                "note": "SIMULATED demo wind; the report feed had no wind value"}
    return {"source": "unknown", "from_deg": None, "speed_mps": None,
            "note": "wind unknown: conservative circular hazard, vertical smoke"}


def build_scenario(mission: Mission, cfg: ScenarioConfig | None = None) -> dict:
    cfg = cfg or ScenarioConfig()
    lx, ly = cfg.launch_from_fire_enu
    # origin = launch site = fire + launch_from_fire (ENU) ; NED offset (north=ly, east=lx)
    o_lat, o_lon = local_ned_to_geodetic(ly, lx, mission.target_lat, mission.target_lon)
    n, e = geodetic_to_local_ned(mission.target_lat, mission.target_lon, o_lat, o_lon)
    fire = (e, n)  # ENU
    rng = math.hypot(*fire)
    if not math.isfinite(rng) or rng > cfg.max_sim_range_m:
        raise ScenarioError(f"fire is {rng:.0f} m from the simulated origin (> {cfg.max_sim_range_m} m bounds)")
    if rng < cfg.fire.r_max_m + cfg.safety_margin_m + 3.0:
        raise ScenarioError("launch site too close to the fire for this scenario")

    wind = _wind(mission, cfg)
    r_core = cfg.fire.r_max_m  # plan against the maximum modelled fire size
    standoff = r_core + cfg.safety_margin_m + cfg.standoff_m
    if wind["source"] == "unknown":
        R = r_core + cfg.plume_length_m
        hazard = Hazard([Circle(fire[0], fire[1], R)])
        hazard_mode = "conservative_circle_wind_unknown"
        ux, uy = -fire[0] / rng, -fire[1] / rng  # towards launch site
        d = R + cfg.safety_margin_m + cfg.standoff_m
        safe = (fire[0] + ux * d, fire[1] + uy * d)
    else:
        poly = plume_polygon(fire, wind["from_deg"], cfg.plume_length_m, r_core, cfg.plume_spread_deg)
        hazard = Hazard([Circle(fire[0], fire[1], r_core), poly])
        hazard_mode = "fire_core_plus_downwind_plume"
        dx, dy = downwind_unit(wind["from_deg"])
        safe = (fire[0] - dx * standoff, fire[1] - dy * standoff)  # upwind of the fire

    safe_dist = math.dist(safe, fire)
    return {
        "schema": SCENARIO_SCHEMA,
        "label": "SIMULATION ONLY - illustrative fire, smoke, wind and hazard; no real aircraft",
        "mission": mission.to_dict(),
        "origin": {"lat": o_lat, "lon": o_lon, "alt_m": 0.0,
                   "definition": "simulated launch site = PX4 home = Gazebo world origin (ENU)",
                   "launch_from_fire_enu_m": [lx, ly]},
        "terrain": "flat plane at elevation 0 (not reconstructed)",
        "fire": {"lat": mission.target_lat, "lon": mission.target_lon, "enu": [round(fire[0], 3), round(fire[1], 3)],
                 "uncertainty_m": mission.target_uncertainty_m},
        "wind": wind,
        "hazard": {"mode": hazard_mode, "shapes": hazard.to_dict(), "altitude": "full-height column",
                   "planning_margin_m": cfg.safety_margin_m,
                   "plume_length_m": cfg.plume_length_m,
                   "note": "simplified planning region, not a dispersion or sensing model"},
        "safe_point_enu": [round(safe[0], 3), round(safe[1], 3)],
        "safe_point_to_fire_m": round(safe_dist, 2),
        "drop_in_effective_range": safe_dist <= cfg.drop_effective_range_m,
        "drop_effective_range_m": cfg.drop_effective_range_m,
        "cruise_alt_m": cfg.cruise_alt_m,
        "fire_model": asdict(cfg.fire),
    }
