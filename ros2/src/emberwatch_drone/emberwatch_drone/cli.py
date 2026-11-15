"""``plan_mission``: print the simulated mission JSON for one incident (no ROS needed)."""

from __future__ import annotations

import argparse
import json
import sys

from .mission import MissionError, fetch_snapshot, load_snapshot_file, mission_from_snapshot


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    src = ap.add_mutually_exclusive_group(required=True)
    src.add_argument("--snapshot", help="path to a Snapshot JSON (e.g. contracts/fixtures/snapshot.json)")
    src.add_argument("--api", help="EmberWatch server base URL, e.g. http://127.0.0.1:8080")
    ap.add_argument("--run-id", default="synthetic-portugal")
    ap.add_argument("--incident-id")
    args = ap.parse_args(argv)
    try:
        snap = load_snapshot_file(args.snapshot) if args.snapshot else fetch_snapshot(args.api, args.run_id)
        mission = mission_from_snapshot(snap, args.incident_id)
    except (MissionError, OSError, ValueError) as exc:
        print(f"plan_mission: {exc}", file=sys.stderr)
        return 2
    json.dump(mission.to_dict(), sys.stdout, indent=2, ensure_ascii=False)
    print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
