import json
import math
from pathlib import Path

import pytest

from emberwatch_drone.fire_model import FireModel, FireModelConfig
from emberwatch_drone.hazard import Circle, Hazard, downwind_unit, plume_polygon
from emberwatch_drone.mission import mission_from_snapshot
from emberwatch_drone.planner import PlanningError, plan
from emberwatch_drone.scenario import ScenarioConfig, ScenarioError, build_scenario

FIXTURE = Path(__file__).resolve().parents[4] / "contracts" / "fixtures" / "snapshot.json"


@pytest.fixture()
def mission():
    return mission_from_snapshot(json.loads(FIXTURE.read_text(encoding="utf-8")))


# ---------------------------------------------------------------- hazard geometry
def test_downwind_unit_meteorological():
    assert downwind_unit(270.0) == pytest.approx((1.0, 0.0), abs=1e-9)  # westerly blows east
    assert downwind_unit(0.0) == pytest.approx((0.0, -1.0), abs=1e-9)  # northerly blows south


def test_plume_contains_downwind_not_upwind():
    poly = plume_polygon((0.0, 0.0), 270.0, 20.0, 3.0, 15.0)
    assert poly.distance((10.0, 0.0)) < 0
    assert poly.distance((-5.0, 0.0)) > 0
    assert poly.distance((10.0, 30.0)) > 0


# ---------------------------------------------------------------- planner
def test_planner_detours_and_stays_clear():
    hz = Hazard([Circle(0.0, 0.0, 5.0)])
    res = plan((-15.0, 0.0), (15.0, 0.0), hz, margin=1.0)
    assert res is not None and res.detour and not res.direct_clear
    assert res.length_m > res.direct_length_m
    for a, b in zip(res.waypoints, res.waypoints[1:]):
        assert hz.segment_clear(a, b, margin=0.99)


def test_planner_direct_when_clear():
    res = plan((0.0, 0.0), (10.0, 0.0), Hazard([Circle(0.0, 20.0, 2.0)]), margin=1.0)
    assert res.direct_clear and res.waypoints == [(0.0, 0.0), (10.0, 0.0)]


def test_planner_no_path_when_enclosed():
    ring = Hazard([Circle(math.cos(a) * 10, math.sin(a) * 10, 2.5) for a in [i * math.pi / 8 for i in range(16)]])
    assert plan((0.0, 0.0), (30.0, 0.0), ring, margin=0.5, pad=5.0) is None


def test_planner_rejects_goal_in_hazard():
    with pytest.raises(PlanningError, match="goal"):
        plan((-20.0, 0.0), (0.0, 0.0), Hazard([Circle(0.0, 0.0, 3.0)]), margin=0.5)


# ---------------------------------------------------------------- fire model
def test_fire_grows_then_is_extinguished_only_by_model():
    m = FireModel(FireModelConfig(initial_intensity=0.3, growth_rate=0.05, suppression_rate=0.1))
    for _ in range(100):
        m.step(0.1, False)
    grown = m.intensity
    assert grown > 0.3 and m.active
    for _ in range(20):  # 2 s of water: not enough
        m.step(0.1, True)
    assert m.active and m.intensity < grown
    for _ in range(200):
        m.step(0.1, True)
        if not m.active:
            break
    assert not m.active and m.radius_m == pytest.approx(m.cfg.r_min_m)
    m.step(10.0, False)  # no re-ignition
    assert not m.active


def test_fire_out_of_range_water_does_nothing():
    m = FireModel(FireModelConfig())
    m.step(5.0, False)
    i = m.intensity
    m.step(5.0, False)
    assert m.intensity >= i and m.water_s == 0


# ---------------------------------------------------------------- scenario
def test_scenario_roundtrip_origin_and_unknown_wind(mission):
    sc = build_scenario(mission)
    assert sc["fire"]["enu"] == pytest.approx([-35.0, -12.0], abs=0.01)  # projected back from lat/lon
    assert sc["wind"]["source"] == "unknown" and sc["wind"]["speed_mps"] is None
    assert sc["hazard"]["mode"] == "conservative_circle_wind_unknown"
    assert sc["drop_in_effective_range"] is False  # standoff outside conservative circle
    assert sc["mission"]["reports"][0]["event_at"] is None


def test_scenario_demo_wind_labelled_and_route_avoids(mission):
    cfg = ScenarioConfig(demo_wind_from_deg=270.0, demo_wind_speed_mps=5.0)
    sc = build_scenario(mission, cfg)
    assert sc["wind"]["source"] == "simulated_demo_value"
    hz = Hazard.from_dict(sc["hazard"]["shapes"])
    res = plan((0.0, 0.0), tuple(sc["safe_point_enu"]), hz, margin=cfg.safety_margin_m)
    assert res is not None and res.detour  # launch site is downwind: straight line crosses plume
    assert sc["drop_in_effective_range"] is True


def test_scenario_rejects_out_of_bounds(mission):
    with pytest.raises(ScenarioError, match="bounds"):
        build_scenario(mission, ScenarioConfig(launch_from_fire_enu=(500.0, 0.0)))


def test_plan_scenario_cli_writes_unique_scenario_id(tmp_path, capsys):
    from emberwatch_drone.scenario_cli import main
    ids = []
    for i in range(2):
        out = tmp_path / f"s{i}.json"
        assert main(["--snapshot", str(FIXTURE), "--demo-wind", "270", "5", "--out", str(out)]) == 0
        ids.append(json.loads(out.read_text())["scenario_id"])
    assert ids[0] != ids[1]
    assert "PX4_HOME_LAT=" in capsys.readouterr().out


def test_plan_scenario_cli_rejects_and_no_path(tmp_path):
    from emberwatch_drone.scenario_cli import main
    bad = json.loads(FIXTURE.read_text())
    bad["incidents"][0]["position"] = list(reversed(bad["incidents"][0]["position"]))
    (tmp_path / "bad.json").write_text(json.dumps(bad))
    assert main(["--snapshot", str(tmp_path / "bad.json"), "--out", str(tmp_path / "o.json")]) == 2
    assert not (tmp_path / "o.json").exists()
    assert main(["--snapshot", str(FIXTURE), "--demo-wind", "270", "5", "--launch-from-fire", "12", "0",
                 "--out", str(tmp_path / "np.json")]) == 3
