"""``plan_arganil_scenario``: seeded synthetic fire in Arganil -> Snapshot -> mission scenario.

    ros2 run emberwatch_drone plan_arganil_scenario --seed 42 --distance 3000 --demo-wind 270 5 \
        --world ros2/generated/arganil --out-dir RUN_DIR

1. Seeded fire point near a randomly chosen OSM building in the town area (outside any
   building/tree footprint + margin).
2. Contract-valid SYNTHETIC ``Snapshot`` (contracts/snapshot.schema.json) for that fire, written
   to RUN_DIR/synthetic_snapshot.json - the same shape the backend serves. The fire position used
   by the mission is re-derived from that snapshot through the shared adapter (mission.py).
3. Drone start a configurable distance away (seeded bearing; shrinks with a labelled FALLBACK
   if the map cannot support the distance), outside obstacles.
4. Two-level planned route: cruise altitude (terrain/buildings/trees/smoke-hazard grid) to an
   entry point, vertical descent, low-level route among buildings/trees to a safe suppression
   waypoint upwind of the fire. Any failure -> NO_PATH (exit 3), never a straight-line fallback.

Exit: 0 ok, 2 rejected input, 3 no safe path / no valid placement.
"""

from __future__ import annotations

import argparse
import copy
import json
import math
import os
import sys
import uuid
from datetime import datetime, timezone

import numpy as np

from ..fire_model import FireModelConfig
from ..hazard import Circle, Hazard, downwind_unit, plume_polygon
from ..mission import MissionError, mission_from_snapshot
from ..geo import geodetic_to_local_ned
from .geodata import MAP_HALF_M, MAP_ORIGIN, map_to_ll
from .gridplan import Grid, astar, make_grid

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), *[".."] * 5))


def _repo_path(*p):
    for base in (os.environ.get("EMBERWATCH_REPO", ""), REPO, os.getcwd()):
        cand = os.path.join(base, *p)
        if base and os.path.exists(cand):
            return cand
    raise FileNotFoundError(os.path.join(*p))


class Placement(Exception):
    pass


def load_world(world_dir: str):
    meta = json.load(open(os.path.join(world_dir, "features.json")))
    r = np.load(os.path.join(world_dir, "obstacles.npz"))
    return meta, {k: r[k] for k in r.files}


def raster_sample(R, x, y, key="top"):
    res = float(R["res"])
    c = int(round((x - float(R["x0"])) / res))
    rr = int(round((y - float(R["y0"])) / res))
    a = R[key]
    if not (0 <= rr < a.shape[0] and 0 <= c < a.shape[1]):
        return None
    return float(a[rr, c])


def mesh_z(R, x, y):
    """Height of the actual terrain MESH (bilinear on its 20 m grid) - what the drone lands on."""
    g, Z = R["mesh_g"], R["mesh_z"]
    st = float(g[1] - g[0])
    fx, fy = (x - float(g[0])) / st, (y - float(g[0])) / st
    c, r = int(np.clip(math.floor(fx), 0, len(g) - 2)), int(np.clip(math.floor(fy), 0, len(g) - 2))
    u, v = fx - c, fy - r
    return float((Z[r, c] * (1 - u) + Z[r, c + 1] * u) * (1 - v) + (Z[r + 1, c] * (1 - u) + Z[r + 1, c + 1] * u) * v)


def window(R, xmin, ymin, xmax, ymax):
    res, x0, y0 = float(R["res"]), float(R["x0"]), float(R["y0"])
    n = R["top"].shape[0]
    c0, c1 = max(0, int((xmin - x0) / res)), min(n, int((xmax - x0) / res) + 1)
    r0, r1 = max(0, int((ymin - y0) / res)), min(n, int((ymax - y0) / res) + 1)
    return (r0, r1, c0, c1), x0 + c0 * res, y0 + r0 * res


def hazard_mask(hz: Hazard, gx0, gy0, res, shape, extra=0.0):
    yy, xx = np.mgrid[0:shape[0], 0:shape[1]]
    X, Y = gx0 + xx * res, gy0 + yy * res
    m = np.zeros(shape, bool)
    for s in hz.shapes:  # vectorised distance only for circles; polygons via bbox + per-cell test
        if isinstance(s, Circle):
            m |= np.hypot(X - s.cx, Y - s.cy) <= s.r + extra
        else:
            xs, ys = [p[0] for p in s.pts], [p[1] for p in s.pts]
            sel = (X >= min(xs) - extra) & (X <= max(xs) + extra) & (Y >= min(ys) - extra) & (Y <= max(ys) + extra)
            for r, c in zip(*np.nonzero(sel)):
                if s.distance((X[r, c], Y[r, c])) <= extra:
                    m[r, c] = True
    return m


def window_kind(R, x, y, rad):
    (r0, r1, c0, c1), _, _ = window(R, x - rad, y - rad, x + rad, y + rad)
    return R["kind"][r0:r1, c0:c1]


def plan_fleet(a, R, sc, hz, start, route, fire, spread, wind, cruise_z, hover_z, mesh_z, occupied, log):
    """SIMULATED containment fleet: pads, layered routes, station altitude, forward refill tank."""
    res = float(R["res"])
    # launch pads in a row across the start->fire direction (each level-checked like the single pad)
    ux, uy = fire[0] - start[0], fire[1] - start[1]
    L = math.hypot(ux, uy)
    px_, py_ = -uy / L, ux / L
    pads = []
    for k in range(60):
        if len(pads) == a.fleet:
            break
        off = (k // 2 + (k % 2)) * (1 if k % 2 else -1) * a.fleet_spacing
        back = 0.0 if k < 20 else -a.fleet_spacing * (1 + (k - 20) // 20)
        x, y = start[0] + px_ * off + ux / L * back, start[1] + py_ * off + uy / L * back
        if occupied(x, y, 5.0) or any(math.dist((x, y), q[:2]) < a.fleet_spacing * 0.9 for q in pads):
            continue
        mz = [mesh_z(R, x + o[0], y + o[1]) for o in [(0, 0), (6, 0), (-6, 0), (0, 6), (0, -6)]]
        if max(mz) - min(mz) > 2.0:
            continue
        top = max(mesh_z(R, x + dx, y + dy) for dx in (-2.5, 0, 2.5) for dy in (-2.5, 0, 2.5)) + 0.15
        pads.append((x, y, top))
    if len(pads) < a.fleet:
        return f"only {len(pads)} level launch pads found near the start for a fleet of {a.fleet}"
    pads.sort(key=lambda q: math.dist(q[:2], start[:2]))
    # station altitude: above the highest canopy over the whole potential fire area
    xs = [fire[0]] + [p["x"] for p in spread]
    ys = [fire[1]] + [p["y"] for p in spread]
    (r0, r1, c0, c1), _, _ = window(R, min(xs) - 30, min(ys) - 30, max(xs) + 30, max(ys) + 30)
    station_z = float(R["top"][r0:r1, c0:c1].max()) + a.station_above_canopy
    staging = tuple(sc["safe_point_enu"])
    # forward refill tank: open, level ground upwind of the fire, outside the hazard, straight clear hop
    if wind["source"] != "unknown":
        dx, dy = downwind_unit(wind["from_deg"])
        up = math.atan2(-dy, -dx)
    else:
        up = math.atan2(start[1] - fire[1], start[0] - fire[0])
    span = 400
    (gr0, gr1, gc0, gc1), gx0, gy0 = window(R, staging[0] - span, staging[1] - span, staging[0] + span, staging[1] + span)
    top_w = R["top"][gr0:gr1, gc0:gc1]
    hz_m = hazard_mask(hz, gx0, gy0, res, top_w.shape)
    g = make_grid(gx0, gy0, res, top_w, station_z, a.v_clear, a.h_margin, hz_m)
    tank = None
    for dist in range(140, 360, 20):
        for k in range(0, 19):
            ang = up + (k // 2 + k % 2) * (1 if k % 2 else -1) * math.radians(10)
            x, y = fire[0] + dist * math.cos(ang), fire[1] + dist * math.sin(ang)
            kw = window_kind(R, x, y, 10.0)
            if kw.size == 0 or np.any(kw > 0) or hz.blocked((x, y), 10.0):
                continue
            mz = [mesh_z(R, x + o[0], y + o[1]) for o in [(0, 0), (4, 0), (-4, 0), (0, 4), (0, -4)]]
            if max(mz) - min(mz) > 2.5 or not g.segment_free((x, y), staging):
                continue
            tank = (x, y, min(mz))
            break
        if tank:
            break
    if tank is None:
        return "no open level ground with a clear hop to the staging point for the forward refill tank"
    drones = []
    for k, pad in enumerate(pads):
        dz = 5.0 * k  # altitude layering for deconfliction (all layers are above the planned clearance)
        r = [[pad[0], pad[1], route[0][2] + dz]] + [[q[0], q[1], q[2] + dz] for q in route[1:]]
        drones.append({"id": k, "px4_instance": k, "namespace": f"/px4_{k}", "mav_sys_id": k + 1,
                       "pad_enu": [round(v, 2) for v in pad], "route3d": [[round(v, 2) for v in q] for q in r],
                       "station_z": round(station_z + dz, 2), "refill_z": round(tank[2] + 8.0 + 3.0 * k, 2)})
    log.append(f"fleet of {a.fleet}: station z {station_z:.1f}, forward tank at {tank[0]:.0f},{tank[1]:.0f} "
               f"({math.dist(tank[:2], fire):.0f} m from the seed)")
    return {"label": "SIMULATED containment fleet - holds the spread until ground crews arrive; does not "
                     "extinguish; water loads, refill tank and crew ETA are assumptions",
            "n": a.fleet, "drones": drones, "staging_enu": [round(v, 2) for v in staging],
            "station_base_z": round(station_z, 2),
            "refill": {"enu": [round(v, 2) for v in tank], "refill_s": a.refill_s,
                       "note": "SIMULATED forward water tank (e.g. one carried in by the first responders)"},
            "water_capacity_s": a.water_capacity_s, "wet_radius_m": a.wet_radius, "wet_duration_s": a.wet_duration,
            "firefighter_eta_s": a.firefighter_eta, "min_station_separation_m": 2 * a.wet_radius * 0.8,
            "min_station_distance_to_burning_m": 12.0}


def synthetic_snapshot(lat, lon, seed, now: datetime) -> dict:
    tmpl = json.load(open(_repo_path("contracts", "fixtures", "snapshot.json"), encoding="utf-8"))
    ts = now.strftime("%Y-%m-%dT%H:%M:%SZ")
    run_id = f"synthetic-arganil-seed-{seed}"
    url = f"https://example.org/emberwatch/synthetic/arganil/seed-{seed}"
    snap = copy.deepcopy(tmpl)
    snap["scenario"].update({"id": f"arganil-synthetic-seed-{seed}", "run_id": run_id, "mode": "synthetic",
                             "name": f"Arganil · generated synthetic fire (seed {seed})", "seed": seed,
                             "start_at": ts, "current_at": ts, "end_at": ts, "epoch": 0, "playing": False,
                             "description": "SIMULATED fire generated by the EmberWatch drone demo; not a real "
                                            "incident, not a reconstruction of any past fire."})
    ev_id, inc_id = f"arganil-generated-{seed}", f"incident-arganil-sim-{seed}"
    snap["incidents"] = [{"id": inc_id, "run_id": run_id, "mode": "synthetic", "title": "Simulated fire (generated)",
                          "location_name": "Arganil (simulated location)", "position": [lon, lat], "uncertainty_m": 0.0,
                          "status": "reported", "evidence_level": "reported", "evidence_ids": [ev_id],
                          "origin_groups": ["emberwatch-sim-generator"], "first_event_at": None, "updated_at": ts}]
    snap["evidence"] = [{"id": ev_id, "run_id": run_id, "mode": "synthetic",
                         "provenance": {"source_id": "emberwatch-sim-generator", "source_url": url,
                                        "origin_group": "emberwatch-sim-generator", "external_id": f"seed-{seed}",
                                        "event_at": None, "published_at": None, "captured_at": ts, "available_at": ts,
                                        "availability_basis": "synthetic_scenario_schedule",
                                        "evidence_hash": f"arganil-seed-{seed}-v1",
                                        "quality_flags": ["synthetic", "generated", "event_time_unknown"]},
                         "position": [lon, lat], "uncertainty_m": 0.0, "location_name": "Arganil (simulated location)",
                         "municipality": "Arganil", "kind": "public_report", "raw_status": None,
                         "title": "Simulated fire (generated)",
                         "body": f"Generated by plan_arganil_scenario with seed {seed}. No real incident is asserted.",
                         "attributes": {"synthetic": True, "generator": "emberwatch_drone.arganil.scenario", "seed": seed}}]
    snap["sources"] = [{"id": "emberwatch-sim-generator", "name": "EmberWatch drone-demo fire generator", "source_url": url,
                        "via": "local generator (no external source)", "origin_group": "emberwatch-sim-generator",
                        "enabled": True, "health": "healthy",
                        "coverage": {"complete": False, "explicit_empty": False, "scope": "one generated synthetic fire",
                                     "notes": ["Not coverage of anything real"]},
                        "last_success_at": ts, "captured_at": ts, "data_at": None,
                        "attribution": "EmberWatch synthetic data; map (c) OpenStreetMap contributors",
                        "notes": ["Invented fire for the simulation demo"]}]
    for k in ("weather", "forecasts", "drone_plans", "briefings"):
        snap[k] = []
    return snap


def validate_snapshot(snap: dict) -> None:
    import jsonschema
    schema = json.load(open(_repo_path("contracts", "snapshot.schema.json")))
    jsonschema.validate(snap, schema)


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--distance", type=float, default=800.0, help="drone start distance from fire (m)")
    ap.add_argument("--min-distance", type=float, default=300.0, help="smallest labelled fallback distance")
    ap.add_argument("--fire-in", choices=("vegetation", "clearing"), default="vegetation",
                    help="vegetation: SIMULATED crown fire seeded in tree cover next to town, spreading through "
                         "tree cells; clearing: small fire on open ground (previous demo)")
    ap.add_argument("--spread-points", type=int, default=None, help="illustrative spread points (0 = no spread)")
    ap.add_argument("--spread-step", type=float, default=None)
    ap.add_argument("--spread-interval", type=float, default=None, help="sim-s between ignitions")
    ap.add_argument("--hover-above-canopy", type=float, default=14.0,
                    help="vegetation mode: hover z = highest canopy within 40 m of the fire + this")
    ap.add_argument("--drop-range", type=float, default=None, help="simulated drop effective range (m)")
    ap.add_argument("--fleet", type=int, default=0,
                    help="N>0: SIMULATED containment fleet of N drones (wet the fuel ahead of the front, rotate to a "
                         "forward refill tank, hand over to ground crews at --firefighter-eta)")
    ap.add_argument("--fleet-spacing", type=float, default=10.0, help="launch pad spacing (m)")
    ap.add_argument("--firefighter-eta", type=float, default=420.0,
                    help="SIMULATED ground-crew arrival, sim-s after ignition (assumption, not a dispatch time)")
    ap.add_argument("--water-capacity-s", type=float, default=24.0, help="simulated spray time per tank load (s)")
    ap.add_argument("--refill-s", type=float, default=15.0, help="simulated refill time at the forward tank (s)")
    ap.add_argument("--wet-radius", type=float, default=18.0, help="simulated wetted radius around a drop target (m)")
    ap.add_argument("--wet-duration", type=float, default=90.0, help="fuel stays wet this long after a drop (s)")
    ap.add_argument("--station-above-canopy", type=float, default=20.0,
                    help="fleet station altitude above the highest canopy in the fire area (m)")
    ap.add_argument("--town-radius", type=float, default=700.0, help="fire chosen near buildings within this radius")
    ap.add_argument("--world", default=None, help="generated world dir (default ros2/generated/arganil)")
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--snapshot", help="use this Snapshot instead of generating one (event-to-mission test)")
    ap.add_argument("--demo-wind", nargs=2, type=float, metavar=("FROM_DEG", "SPEED_MPS"))
    ap.add_argument("--cruise-agl", type=float, default=45.0, help="cruise z = max(start, fire ground) + this")
    ap.add_argument("--hover-agl", type=float, default=16.0, help="suppression hover height above fire ground")
    ap.add_argument("--v-clear", type=float, default=6.0, help="vertical clearance over obstacle tops (m)")
    ap.add_argument("--h-margin", type=float, default=4.0, help="horizontal margin around obstacles (m)")
    ap.add_argument("--entry-radius", type=float, default=140.0, help="low-level approach length (m)")
    ap.add_argument("--plume-length", type=float, default=40.0)
    ap.add_argument("--force-blocked", action="store_true",
                    help="TEST: surround the fire with a ring hazard so no safe path exists")
    a = ap.parse_args(argv)
    veg = a.fire_in == "vegetation"
    dflt = {"spread_points": (160, 12), "spread_step": (10.0, 6.0), "spread_interval": (1.15, 8.0), "drop_range": (25.0, 10.0)}
    if a.fleet > 0 and a.spread_interval is None:
        a.spread_interval = 3.0  # slower spread so the fleet's containment has a window to act
    for k, (v_veg, v_clr) in dflt.items():
        if getattr(a, k) is None:
            setattr(a, k, v_veg if veg else v_clr)
    world = os.path.abspath(a.world or _repo_path("ros2", "generated", "arganil"))
    os.makedirs(a.out_dir, exist_ok=True)
    meta, R = load_world(world)
    rng = np.random.default_rng(a.seed)
    log: list[str] = []
    res = float(R["res"])
    # vegetation: faster-growing, larger illustrative fire; more suppression per water-second so a
    # long hover can still bring the (much larger) burning area down. Illustrative values only.
    fcfg = FireModelConfig(initial_intensity=0.45, growth_rate=0.03, suppression_rate=0.40, r_max_m=6.0) if veg \
        else FireModelConfig()

    def ground(x, y):
        return raster_sample(R, x, y, "ground")

    def occupied(x, y, rad):
        (r0, r1, c0, c1), _, _ = window(R, x - rad, y - rad, x + rad, y + rad)
        return bool(np.any(R["kind"][r0:r1, c0:c1] > 0))

    # ---------------------------------------------------------------- 1. fire point
    blds = [b for b in meta["buildings"] if math.hypot(*np.mean(b["poly"], 0)) <= a.town_radius]
    if not blds:
        print("REJECTED: no buildings in town radius", file=sys.stderr)
        return 2
    fire = None
    for _ in range(400 if not veg else 1500):
        b = blds[int(rng.integers(len(blds)))]
        cx, cy = np.mean(b["poly"], 0)
        ang, d = rng.uniform(0, 2 * math.pi), (rng.uniform(10, 30) if not veg else rng.uniform(20, 90))
        x, y = cx + d * math.cos(ang), cy + d * math.sin(ang)
        if veg:
            # inside tree cover (not a building), with a substantial forest patch around it
            (r0, r1, c0, c1), _, _ = window(R, x - 30, y - 30, x + 30, y + 30)
            k = R["kind"][r0:r1, c0:c1]
            here = raster_sample(R, x, y, "kind")
            if here != 2 or k.size == 0 or np.mean(k == 2) < 0.45 or np.any(window_kind(R, x, y, 6.0) == 1):
                continue
        elif occupied(x, y, fcfg.r_max_m + 2.0):
            continue
        fire = (float(x), float(y))
        near_building = b["osm_id"]
        break
    if fire is None:
        print("NO VALID FIRE POINT (all candidates inside obstacles)", file=sys.stderr)
        return 3

    # ---------------------------------------------------------------- 2. event via shared contract
    flat, flon = (float(v) for v in map_to_ll(np.array(fire[0]), np.array(fire[1])))
    now = datetime.now(timezone.utc)
    if a.snapshot:
        snap = json.load(open(a.snapshot, encoding="utf-8"))
        log.append(f"event from provided snapshot {a.snapshot}")
    else:
        snap = synthetic_snapshot(round(flat, 7), round(flon, 7), a.seed, now)
    try:
        validate_snapshot(snap)
        mission = mission_from_snapshot(snap)
    except (MissionError, Exception) as exc:  # jsonschema.ValidationError or contract rejection
        print(f"REJECTED: {type(exc).__name__}: {str(exc)[:300]}", file=sys.stderr)
        return 2
    snap_path = os.path.join(a.out_dir, "synthetic_snapshot.json")
    json.dump(snap, open(snap_path, "w"), indent=1, ensure_ascii=False)
    n, e = geodetic_to_local_ned(mission.target_lat, mission.target_lon, *MAP_ORIGIN)
    fire = (e, n)  # re-derived from the event through the shared adapter
    if max(abs(e), abs(n)) > MAP_HALF_M - 200:
        print(f"REJECTED: event at map ({e:.0f},{n:.0f}) outside the modelled Arganil area", file=sys.stderr)
        return 2
    fz = ground(*fire)

    # ---------------------------------------------------------------- 3. wind + hazard
    if mission.conditions.wind_from_deg is not None and mission.conditions.wind_speed_mps is not None:
        wind = {"source": "feed", "from_deg": mission.conditions.wind_from_deg, "speed_mps": mission.conditions.wind_speed_mps}
    elif a.demo_wind:
        wind = {"source": "simulated_demo_value", "from_deg": a.demo_wind[0] % 360, "speed_mps": a.demo_wind[1],
                "note": "SIMULATED demo wind; the event had no wind value"}
    else:
        wind = {"source": "unknown", "from_deg": None, "speed_mps": None,
                "note": "wind unknown: conservative circular hazard, vertical smoke"}
    r_core = fcfg.r_max_m if not veg else 10.0
    # ---- illustrative spread footprint (seeded): downwind-biased random walk, never into buildings
    spread = []
    pts = [fire]
    srng = np.random.default_rng(a.seed + 1000)
    dw = downwind_unit(wind["from_deg"]) if wind["source"] != "unknown" else None
    tries = 0
    while len(spread) < a.spread_points and tries < 2000:
        tries += 1
        pi = int(srng.integers(max(0, len(pts) - 4), len(pts)))
        par = pts[pi]
        base = math.atan2(dw[1], dw[0]) if dw else srng.uniform(0, 2 * math.pi)
        if veg:  # parent: any recent front point (wide, ragged front); flanks + some backing spread
            pi = int(srng.integers(max(0, len(pts) - 24), len(pts)))
            par = pts[pi]
            ang = base + srng.normal(0.0, math.radians(70)) if srng.uniform() < 0.85 else srng.uniform(0, 2 * math.pi)
        else:
            ang = base + srng.uniform(-math.radians(60), math.radians(60))
        q = (par[0] + a.spread_step * math.cos(ang), par[1] + a.spread_step * math.sin(ang))
        if any(math.dist(q, o) < a.spread_step * 0.6 for o in pts):
            continue
        (r0, r1, c0, c1), _, _ = window(R, q[0] - 2, q[1] - 2, q[0] + 2, q[1] + 2)
        if np.any(R["kind"][r0:r1, c0:c1] == 1):
            continue  # do not spread into building footprints
        if veg and raster_sample(R, q[0], q[1], "kind") != 2:
            continue  # SIMULATED crown fire only spreads through tree cells (fuel)
        pts.append(q)
        pt = {"x": round(q[0], 2), "y": round(q[1], 2), "z": round(ground(*q), 2),
              "t_ignite_s": round(a.spread_interval * len(pts) * srng.uniform(0.8, 1.2), 1),
              "parent": pi - 1}  # index into spread (-1 = the seed); fire only spreads from a burning parent
        if veg:
            pt["canopy_z"] = round(raster_sample(R, q[0], q[1], "top"), 2)
        spread.append(pt)
    if wind["source"] == "unknown":
        reach = max([math.dist(fire, (p["x"], p["y"])) for p in spread] or [0.0])
        hz = Hazard([Circle(fire[0], fire[1], r_core + a.plume_length + reach)])
        mode = "conservative_circle_wind_unknown"
    else:
        ext = [((p["x"] - fire[0]) * dw[0] + (p["y"] - fire[1]) * dw[1],
                abs(-(p["x"] - fire[0]) * dw[1] + (p["y"] - fire[1]) * dw[0])) for p in spread] or [(0.0, 0.0)]
        hz = Hazard([Circle(fire[0], fire[1], r_core)] + [Circle(p["x"], p["y"], 3.5 if not veg else 9.0) for p in spread] +
                    [plume_polygon(fire, wind["from_deg"], a.plume_length + max(e[0] for e in ext),
                                   r_core + max(e[1] for e in ext), 15.0)])
        mode = "fire_core_plus_downwind_plume"
    if a.force_blocked:
        ring = [Circle(fire[0] + 60 * math.cos(t), fire[1] + 60 * math.sin(t), 14.0)
                for t in np.linspace(0, 2 * math.pi, 28, endpoint=False)]
        hz = Hazard(hz.shapes + ring)
        mode += "+TEST_forced_blocking_ring"
        log.append("TEST: forced ring hazard around fire (no-path test)")

    if veg:
        (r0, r1, c0, c1), _, _ = window(R, fire[0] - 40, fire[1] - 40, fire[0] + 40, fire[1] + 40)
        hover_z = float(R["top"][r0:r1, c0:c1].max()) + a.hover_above_canopy
    else:
        hover_z = fz + a.hover_agl
    result = {"stage": None}

    def fail(stage, why):
        sc = {"schema": "emberwatch.sim_scenario.v1", "label": "SIMULATION ONLY", "no_path": True, "stage": stage,
              "reason": why, "seed": a.seed, "fire": {"enu": list(fire), "ground_z": fz}, "log": log}
        json.dump(sc, open(os.path.join(a.out_dir, "scenario.json"), "w"), indent=1)
        print(f"NO_PATH at {stage}: {why}", file=sys.stderr)
        return 3

    # ---------------------------------------------------------------- 4. safe suppression waypoint (low level)
    half_low = a.entry_radius + 80
    (lr0, lr1, lc0, lc1), lx0, ly0 = window(R, fire[0] - half_low, fire[1] - half_low, fire[0] + half_low, fire[1] + half_low)
    top_low = R["top"][lr0:lr1, lc0:lc1]
    hm_low = hazard_mask(hz, lx0, ly0, res, top_low.shape)
    low = make_grid(lx0, ly0, res, top_low, hover_z, a.v_clear, a.h_margin, hm_low)
    standoff0 = r_core + 2.5 + 2.0
    if wind["source"] != "unknown":
        dx, dy = downwind_unit(wind["from_deg"])
        base_ang = math.atan2(-dy, -dx)  # upwind
    else:
        base_ang = math.atan2(-fire[1], -fire[0])
    safe = None
    drop_range = a.drop_range if wind["source"] != "unknown" else 1e9
    for dist in np.arange(standoff0, a.drop_range + 0.01, 0.5) if wind["source"] != "unknown" else [r_core + a.plume_length + 6.5]:
        for k in range(0, 37):
            ang = base_ang + (k // 2 + k % 2) * (1 if k % 2 else -1) * math.radians(10)
            p = (fire[0] + dist * math.cos(ang), fire[1] + dist * math.sin(ang))
            if low.free_xy(*p) and not hz.blocked(p, 2.5):
                safe = p
                break
        if safe:
            break
    if safe is None:
        return fail("safe_point", "no free hover waypoint within drop range outside hazard/obstacles")

    # ---------------------------------------------------------------- 5. start point (seeded) with fallback
    dist_used = None
    bearings = rng.permutation(np.arange(0, 360, 5))
    d = a.distance
    start = None
    while d >= a.min_distance and start is None:
        for brg in bearings:
            p = (fire[0] + d * math.sin(math.radians(brg)), fire[1] + d * math.cos(math.radians(brg)))
            if max(abs(p[0]), abs(p[1])) > MAP_HALF_M - 150 or occupied(p[0], p[1], 8.0):
                continue
            g = ground(*p)
            if g is None:
                continue
            mz = [mesh_z(R, p[0] + o[0], p[1] + o[1]) for o in [(0, 0), (6, 0), (-6, 0), (0, 6), (0, -6)]]
            if max(mz) - min(mz) > 2.0:  # gentle ground only (~10 deg); a level pad is placed on top
                continue
            pad_top = max(mesh_z(R, p[0] + dx_, p[1] + dy_) for dx_ in (-2.5, 0, 2.5) for dy_ in (-2.5, 0, 2.5)) + 0.15
            start = (p[0], p[1], pad_top)
            dist_used = d
            break
        if start is None:
            log.append(f"no valid start at {d:.0f} m; shrinking")
            d *= 0.85
    if start is None:
        return fail("start_point", f"no valid start between {a.min_distance} and {a.distance} m")
    fallback = dist_used < a.distance - 1

    # ---------------------------------------------------------------- 6. cruise-level plan to entry point
    cruise_z = max(start[2], fz) + a.cruise_agl
    pad = 300
    xs, ys = [start[0], fire[0]], [start[1], fire[1]]
    (cr0, cr1, cc0, cc1), cx0, cy0 = window(R, min(xs) - pad, min(ys) - pad, max(xs) + pad, max(ys) + pad)
    step = 2  # cruise grid at 10 m (max-pool of the 5 m raster)
    top_c = R["top"][cr0:cr1, cc0:cc1]
    h2, w2 = top_c.shape[0] // step * step, top_c.shape[1] // step * step
    top_c = top_c[:h2, :w2].reshape(h2 // step, step, w2 // step, step).max(axis=(1, 3))
    cres = res * step
    cx0c, cy0c = cx0 + res * (step - 1) / 2, cy0 + res * (step - 1) / 2
    hm_c = hazard_mask(hz, cx0c, cy0c, cres, top_c.shape, extra=cres / 2)
    cruise = make_grid(cx0c, cy0c, cres, top_c, cruise_z, a.v_clear, a.h_margin, hm_c)
    if not cruise.free_xy(start[0], start[1]):
        return fail("cruise", "start column blocked at cruise altitude (raise --cruise-agl)")

    entry, low_path, cruise_path = None, None, None
    to_start = math.atan2(start[1] - safe[1], start[0] - safe[0])
    for k in range(0, 36):
        ang = to_start + (k // 2 + k % 2) * (1 if k % 2 else -1) * math.radians(10)
        for rr in (a.entry_radius, a.entry_radius * 0.7, a.entry_radius * 0.45):
            E = (safe[0] + rr * math.cos(ang), safe[1] + rr * math.sin(ang))
            if not (low.free_xy(*E) and cruise.free_xy(*E)):
                continue
            lp = astar(low, E, safe)
            if lp is None:
                continue
            cp = astar(cruise, (start[0], start[1]), E)
            if cp is None:
                continue
            entry, low_path, cruise_path = E, lp, cp
            break
        if entry:
            break
    if entry is None:
        return fail("route", "no entry point with both a cruise-level and a low-level path (obstacles/smoke)")

    # ---------------------------------------------------------------- 7. route + checks
    route = [[start[0], start[1], cruise_z]] + [[x, y, cruise_z] for x, y in cruise_path[1:]] + \
            [[entry[0], entry[1], hover_z]] + [[x, y, hover_z] for x, y in low_path[1:]]
    direct_c = cruise.segment_free((start[0], start[1]), entry)
    direct_low = low.segment_free(entry, safe)

    def blocked_kinds(a_, b_, z):
        """Samples along the straight line that the planner treats as blocked (incl. margins),
        attributed to the tallest raw obstacle within the margin window, or the smoke hazard."""
        kinds = {"terrain": 0, "building": 0, "tree": 0, "smoke_hazard": 0}
        n_ = max(2, int(math.dist(a_, b_) / res))
        m = a.h_margin + res
        for i in range(n_ + 1):
            x, y = a_[0] + (b_[0] - a_[0]) * i / n_, a_[1] + (b_[1] - a_[1]) * i / n_
            if hz.blocked((x, y), a.h_margin):
                kinds["smoke_hazard"] += 1
                continue
            (r0, r1, c0, c1), _, _ = window(R, x - m, y - m, x + m, y + m)
            win = R["top"][r0:r1, c0:c1]
            if win.size and float(win.max()) + a.v_clear > z:
                rr, cc = np.unravel_index(int(np.argmax(win)), win.shape)
                kinds[["terrain", "building", "tree"][int(R["kind"][r0 + rr, c0 + cc])]] += 1
        return kinds

    length = sum(math.dist(p, q) for p, q in zip(route, route[1:]))
    o = meta
    sc = {
        "schema": "emberwatch.sim_scenario.v1", "world": "emberwatch_arganil", "world_dir": world,
        "label": "SIMULATION ONLY - generated synthetic fire in a representative Arganil world; no real aircraft",
        "scenario_id": f"{now:%Y%m%dT%H%M%SZ}-{uuid.uuid4().hex[:8]}", "seed": a.seed,
        "mission": mission.to_dict(), "snapshot_file": snap_path,
        "origin": {"lat": MAP_ORIGIN[0], "lon": MAP_ORIGIN[1], "alt_m": o["origin_elevation_m"],
                   "definition": "Arganil map origin = Gazebo world origin (ENU); z = elevation - origin elevation"},
        "terrain": "Mapterhorn DEM mesh (see features.json)",
        "fire": {"lat": mission.target_lat, "lon": mission.target_lon, "enu": [round(fire[0], 2), round(fire[1], 2)],
                 "ground_z": round(fz, 2), "near_osm_building": near_building,
                 "spread": spread, "spread_note": "ILLUSTRATIVE seeded spread points, not a fire-spread prediction",
                 "fuel": "tree_canopy" if veg else "open_ground",
                 "canopy_z": round(raster_sample(R, fire[0], fire[1], "top"), 2) if veg else None},
        "start_enu": [round(start[0], 2), round(start[1], 2), round(start[2], 2)],
        "launch_pad": {"enu_top": [round(start[0], 2), round(start[1], 2), round(start[2], 2)], "size_m": 5.0,
                       "note": "simulated level launch pad (box collider) so the x500 starts level on sloped terrain"},
        "start_distance_m": round(math.dist(start[:2], fire), 1), "requested_distance_m": a.distance,
        "distance_fallback": fallback,
        "wind": wind,
        "hazard": {"mode": mode, "shapes": hz.to_dict(), "altitude": "full-height column",
                   "planning_margin_m": 2.5, "plume_length_m": a.plume_length,
                   "note": "simplified planning region, not a dispersion or sensing model"},
        "safe_point_enu": [round(safe[0], 2), round(safe[1], 2)], "safe_point_to_fire_m": round(math.dist(safe, fire), 2),
        "drop_in_effective_range": math.dist(safe, fire) <= drop_range, "drop_effective_range_m": a.drop_range,
        "cruise_alt_m": round(cruise_z, 2), "hover_alt_z": round(hover_z, 2),
        "route3d": [[round(v, 2) for v in p] for p in route],
        "route_info": {"length_m": round(length, 1), "cruise_waypoints": len(cruise_path), "low_waypoints": len(low_path),
                       "entry_enu": [round(entry[0], 2), round(entry[1], 2)],
                       "direct_cruise_line_clear": direct_c, "direct_low_line_clear": direct_low,
                       "direct_cruise_line_blocked_samples": blocked_kinds((start[0], start[1]), entry, cruise_z),
                       "direct_low_line_blocked_samples": blocked_kinds(entry, safe, hover_z),
                       "v_clear_m": a.v_clear, "h_margin_m": a.h_margin, "grid_res_m": {"cruise": cres, "low": res}},
        "obstacle_raster": os.path.join(world, "obstacles.npz"),
        "fire_model": vars(fcfg), "log": log,
    }
    if a.fleet > 0:
        fl = plan_fleet(a, R, sc, hz, start, route, fire, spread, wind, cruise_z, hover_z, mesh_z, occupied, log)
        if isinstance(fl, str):
            return fail("fleet", fl)
        sc["fleet"] = fl
    json.dump(sc, open(os.path.join(a.out_dir, "scenario.json"), "w"), indent=1)
    if a.fleet > 0:
        print('EMBER_FLEET_POSES="' + ";".join(f"{d['pad_enu'][0]:.2f},{d['pad_enu'][1]:.2f},{d['pad_enu'][2]:.2f}"
                                               for d in sc["fleet"]["drones"]) + '"')
        t = sc["fleet"]["refill"]["enu"]
        print(f"EMBER_TANK_POSE={t[0]:.2f},{t[1]:.2f},{t[2]:.2f}")
    print(f"PX4_HOME_LAT={MAP_ORIGIN[0]}\nPX4_HOME_LON={MAP_ORIGIN[1]}\nPX4_HOME_ALT={o['origin_elevation_m']}")
    print(f"PX4_GZ_MODEL_POSE={start[0]:.2f},{start[1]:.2f},{start[2] + 0.3:.2f},0,0,0")
    print(f"EMBER_PAD_POSE={start[0]:.2f},{start[1]:.2f},{start[2]:.2f}")
    print(f"# seed {a.seed}: fire map ENU ({fire[0]:.1f},{fire[1]:.1f}) z {fz:.1f}; start {start[0]:.0f},{start[1]:.0f} "
          f"dist {sc['start_distance_m']} m{' (FALLBACK)' if fallback else ''}; cruise z {cruise_z:.1f}, hover z {hover_z:.1f}; "
          f"route {length:.0f} m, {len(route)} wps; direct cruise clear={direct_c}, low clear={direct_low}; wind {wind['source']}",
          file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
