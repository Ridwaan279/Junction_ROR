import json
import math
from pathlib import Path

import numpy as np
import pytest

from emberwatch_drone.arganil.gridplan import astar, make_grid
from emberwatch_drone.arganil.worldgen import ear_clip

ROS2 = Path(__file__).resolve().parents[3]
WORLD = ROS2 / "generated" / "arganil"


def test_grid_blocks_tall_obstacle_not_low_one():
    top = np.zeros((40, 40), np.float32)
    top[10:30, 20] = 30.0  # wall 30 m tall
    top[5, 5] = 3.0        # low shrub
    g = make_grid(0.0, 0.0, 1.0, top, z=20.0, v_clear=5.0, h_margin=1.0)
    assert g.blocked[15, 20] and g.blocked[15, 19]  # wall + margin
    assert not g.blocked[5, 5]                       # 3 + 5 < 20
    path = astar(g, (10.0, 20.0), (30.0, 20.0))
    assert path is not None and len(path) > 2        # detour around the wall
    assert all(g.segment_free(a, b) for a, b in zip(path, path[1:]))
    high = make_grid(0.0, 0.0, 1.0, top, z=40.0, v_clear=5.0, h_margin=1.0)
    assert astar(high, (10.0, 20.0), (30.0, 20.0)) == [(10.0, 20.0), (30.0, 20.0)]  # flies over


def test_grid_no_path_when_enclosed():
    top = np.zeros((30, 30), np.float32)
    top[5:25, 5] = top[5:25, 24] = top[5, 5:25] = top[24, 5:25] = 50.0
    g = make_grid(0.0, 0.0, 1.0, top, z=20.0, v_clear=5.0, h_margin=0.0)
    assert astar(g, (15.0, 15.0), (2.0, 2.0)) is None


def test_ear_clip_concave():
    poly = [(0, 0), (4, 0), (4, 4), (2, 1), (0, 4)]  # CCW, concave
    tris = ear_clip(poly)
    assert len(tris) == 3
    area = sum(abs((poly[b][0] - poly[a][0]) * (poly[c][1] - poly[a][1]) -
                   (poly[c][0] - poly[a][0]) * (poly[b][1] - poly[a][1])) / 2 for a, b, c in tris)
    assert area == pytest.approx(10.0)


@pytest.mark.skipif(not (WORLD / "features.json").exists(), reason="Arganil world not generated")
def test_seeded_scenario_reproducible_and_valid(tmp_path, monkeypatch):
    from emberwatch_drone.arganil import scenario as sc
    monkeypatch.setenv("EMBERWATCH_REPO", str(ROS2.parent))
    outs = []
    for i in range(2):
        d = tmp_path / f"r{i}"
        assert sc.main(["--seed", "42", "--demo-wind", "270", "5", "--world", str(WORLD), "--out-dir", str(d)]) == 0
        outs.append(json.loads((d / "scenario.json").read_text()))
        snap = json.loads((d / "synthetic_snapshot.json").read_text())
        assert snap["incidents"][0]["mode"] == "synthetic" and snap["scenario"]["mode"] == "synthetic"
        assert snap["evidence"][0]["provenance"]["event_at"] is None
    a, b = outs
    assert a["fire"] == b["fire"] and a["start_enu"] == b["start_enu"] and a["route3d"] == b["route3d"]
    assert a["start_distance_m"] == pytest.approx(800, abs=1)
    assert len(a["fire"]["spread"]) == 160 and a["fire"]["spread"] == b["fire"]["spread"]
    assert a["fire"]["fuel"] == "tree_canopy"
    feats = json.loads((WORLD / "features.json").read_text())
    fx, fy = a["fire"]["enu"]
    assert min(math.dist((fx, fy), np.mean(bl["poly"], 0)) for bl in feats["buildings"]) < 120  # next to town
    R = np.load(WORLD / "obstacles.npz")
    for q in [{"x": fx, "y": fy}] + a["fire"]["spread"]:  # seed + every spread point is on a tree cell (fuel)
        assert sc.raster_sample(R, q["x"], q["y"], "kind") == 2
    assert a["hover_alt_z"] > max(q["canopy_z"] for q in a["fire"]["spread"][:1]) + 5  # hover above the canopy
    # previous small open-ground fire still available
    d = tmp_path / "clearing"
    assert sc.main(["--seed", "42", "--fire-in", "clearing", "--demo-wind", "270", "5", "--world", str(WORLD),
                    "--out-dir", str(d)]) == 0
    c = json.loads((d / "scenario.json").read_text())
    assert len(c["fire"]["spread"]) == 12 and c["fire"]["fuel"] == "open_ground"
    # different seed -> different fire
    d = tmp_path / "other"
    assert sc.main(["--seed", "7", "--demo-wind", "270", "5", "--world", str(WORLD), "--out-dir", str(d)]) == 0
    assert json.loads((d / "scenario.json").read_text())["fire"] != a["fire"]


@pytest.mark.skipif(not (WORLD / "features.json").exists(), reason="Arganil world not generated")
def test_forced_blocked_is_no_path(tmp_path, monkeypatch):
    from emberwatch_drone.arganil import scenario as sc
    monkeypatch.setenv("EMBERWATCH_REPO", str(ROS2.parent))
    assert sc.main(["--seed", "42", "--demo-wind", "270", "5", "--world", str(WORLD), "--out-dir", str(tmp_path),
                    "--force-blocked"]) == 3
    assert json.loads((tmp_path / "scenario.json").read_text())["no_path"] is True


@pytest.mark.skipif(not (WORLD / "features.json").exists(), reason="Arganil world not generated")
def test_event_outside_map_rejected(tmp_path, monkeypatch):
    from emberwatch_drone.arganil import scenario as sc
    monkeypatch.setenv("EMBERWATCH_REPO", str(ROS2.parent))
    # the repo's synthetic Sever do Vouga fixture is valid but ~45 km outside the Arganil map
    snap = ROS2.parent / "contracts" / "fixtures" / "snapshot.json"
    assert sc.main(["--seed", "42", "--world", str(WORLD), "--out-dir", str(tmp_path), "--snapshot", str(snap)]) == 2


@pytest.mark.skipif(not (WORLD / "features.json").exists(), reason="Arganil world not generated")
def test_fleet_scenario(tmp_path, monkeypatch):
    from emberwatch_drone.arganil import scenario as sc
    monkeypatch.setenv("EMBERWATCH_REPO", str(ROS2.parent))
    assert sc.main(["--seed", "37", "--demo-wind", "270", "5", "--fleet", "4", "--world", str(WORLD),
                    "--out-dir", str(tmp_path)]) == 0
    s = json.loads((tmp_path / "scenario.json").read_text())
    fl = s["fleet"]
    assert fl["n"] == 4 and len(fl["drones"]) == 4
    pads = [d["pad_enu"] for d in fl["drones"]]
    assert min(math.dist(a[:2], b[:2]) for i, a in enumerate(pads) for b in pads[i + 1:]) >= 9.0
    assert len({d["station_z"] for d in fl["drones"]}) == 4  # altitude layers
    sp = s["fire"]["spread"]
    assert all(-1 <= q["parent"] < i for i, q in enumerate(sp))  # spread tree: parent precedes child
    R = np.load(WORLD / "obstacles.npz")
    tx, ty, _ = fl["refill"]["enu"]
    assert sc.raster_sample(R, tx, ty, "kind") == 0  # tank on open ground
    assert "does not extinguish" in fl["label"]
