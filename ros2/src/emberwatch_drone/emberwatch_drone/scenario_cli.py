"""``plan_scenario``: report snapshot -> scenario JSON (+ PX4 home env) for the PX4 fire demo.

Exit codes: 0 ok, 2 invalid/unsupported event (nothing written), 3 no path (scenario written,
mission must not fly).
"""

from __future__ import annotations

import argparse
import json
import sys
import uuid
from datetime import datetime, timezone

from .hazard import Hazard
from .mission import MissionError, fetch_snapshot, load_snapshot_file, mission_from_snapshot
from .planner import PlanningError, plan
from .scenario import ScenarioConfig, ScenarioError, build_scenario


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    src = ap.add_mutually_exclusive_group(required=True)
    src.add_argument("--snapshot", help="Snapshot JSON file (e.g. contracts/fixtures/snapshot.json)")
    src.add_argument("--api", help="EmberWatch server base URL (GET /api/snapshot)")
    ap.add_argument("--run-id", default="synthetic-portugal")
    ap.add_argument("--incident-id")
    ap.add_argument("--out", required=True)
    ap.add_argument("--demo-wind", nargs=2, type=float, metavar=("FROM_DEG", "SPEED_MPS"),
                    help="SIMULATED wind used only if the feed has none (meteorological FROM, clockwise from N)")
    ap.add_argument("--launch-from-fire", nargs=2, type=float, metavar=("EAST_M", "NORTH_M"), default=(35.0, 12.0),
                    help="simulated launch site position relative to the reported fire (ENU metres)")
    ap.add_argument("--plume-length", type=float, default=25.0)
    a = ap.parse_args(argv)
    cfg = ScenarioConfig(launch_from_fire_enu=tuple(a.launch_from_fire), plume_length_m=a.plume_length)
    if a.demo_wind:
        cfg.demo_wind_from_deg, cfg.demo_wind_speed_mps = a.demo_wind
    try:
        snap = load_snapshot_file(a.snapshot) if a.snapshot else fetch_snapshot(a.api, a.run_id)
        sc = build_scenario(mission_from_snapshot(snap, a.incident_id), cfg)
    except (MissionError, ScenarioError, OSError, ValueError) as exc:
        print(f"plan_scenario: REJECTED: {exc}", file=sys.stderr)
        return 2
    # Unique per run: fire_sim and the mission ignore messages carrying any other id, so a stale
    # node from an earlier run can never feed this run's fire state or water drop.
    sc["scenario_id"] = f"{datetime.now(timezone.utc):%Y%m%dT%H%M%SZ}-{uuid.uuid4().hex[:8]}"
    sc["input"] = {"snapshot_file": a.snapshot, "api": a.api, "run_id": a.run_id if a.api else None}
    code = 0
    try:
        res = plan((0.0, 0.0), tuple(sc["safe_point_enu"]), Hazard.from_dict(sc["hazard"]["shapes"]),
                   sc["hazard"]["planning_margin_m"])
        sc["preflight_plan"] = None if res is None else {
            "waypoints_enu": [[round(x, 2), round(y, 2)] for x, y in res.waypoints],
            "direct_line_clear": res.direct_clear, "length_m": round(res.length_m, 2)}
        if res is None:
            code = 3
    except PlanningError as exc:
        sc["preflight_plan"] = {"error": str(exc)}
        code = 3
    with open(a.out, "w", encoding="utf-8") as f:
        json.dump(sc, f, indent=2, ensure_ascii=False)
    o = sc["origin"]
    print(f"PX4_HOME_LAT={o['lat']:.8f}")
    print(f"PX4_HOME_LON={o['lon']:.8f}")
    print(f"PX4_HOME_ALT={o['alt_m']}")
    print(f"# fire ENU {sc['fire']['enu']}, wind {sc['wind']['source']}, hazard {sc['hazard']['mode']}, "
          f"safe point {sc['safe_point_enu']}, plan {sc['preflight_plan']}", file=sys.stderr)
    if code == 3:
        print("plan_scenario: NO PATH to the safe point; the mission must not fly", file=sys.stderr)
    return code


if __name__ == "__main__":
    sys.exit(main())
