import copy
import json
from pathlib import Path

import pytest

from emberwatch_drone.cli import main as cli_main
from emberwatch_drone.mission import MissionError, mission_from_snapshot

REPO = Path(__file__).resolve().parents[4]
FIXTURE = REPO / "contracts" / "fixtures" / "snapshot.json"


@pytest.fixture()
def snap():
    # Synthetic, clearly labelled contract fixture (no real fire report).
    return json.loads(FIXTURE.read_text(encoding="utf-8"))


def test_fixture_mission_keeps_provenance_and_unknowns(snap):
    m = mission_from_snapshot(snap)
    assert m.simulation is True and m.drone_count == 1
    assert m.data_mode == "synthetic"
    assert (m.target_lon, m.target_lat) == tuple(snap["incidents"][0]["position"])
    r = m.reports[0]
    assert r.source_url == snap["evidence"][0]["provenance"]["source_url"]
    assert r.captured_at == snap["evidence"][0]["provenance"]["captured_at"]
    assert r.event_at is None  # unknown in source stays unknown
    c = m.conditions
    assert c.wind_speed_mps is None and c.wind_from_deg is None and c.smoke is None


def test_no_incidents_is_error_not_empty_mission(snap):
    snap["incidents"] = []
    with pytest.raises(MissionError, match="not evidence of zero fires"):
        mission_from_snapshot(snap)


def test_swapped_lat_lon_rejected(snap):
    snap["incidents"][0]["position"] = list(reversed(snap["incidents"][0]["position"]))
    with pytest.raises(MissionError, match="outside mainland Portugal"):
        mission_from_snapshot(snap)


def test_uncited_incident_rejected(snap):
    snap["evidence"] = []
    with pytest.raises(MissionError, match="source_url"):
        mission_from_snapshot(snap)


def test_forecast_weather_ignored_observed_used(snap):
    base = {"id": "w", "run_id": "r", "provenance": {"source_url": "https://example.org/w"},
            "station_id": "s1", "position": [-8.367, 40.745], "temperature_c": None,
            "relative_humidity_pct": None, "gust_mps": None, "fire_risk": None, "wind_from_deg": 270.0}
    fc = dict(base, valid_at="2026-11-14T12:30:00Z", forecast=True, wind_speed_mps=9.0)
    obs = dict(base, valid_at="2026-11-14T12:00:00Z", forecast=False, wind_speed_mps=3.0)
    snap["weather"] = [fc]
    assert mission_from_snapshot(snap).conditions.wind_speed_mps is None
    snap["weather"] = [fc, copy.deepcopy(obs)]
    c = mission_from_snapshot(snap).conditions
    assert c.wind_speed_mps == 3.0 and c.wind_from_deg == 270.0
    assert c.weather_station_distance_m == pytest.approx(1112, abs=2)


def test_cli_outputs_json(capsys):
    assert cli_main(["--snapshot", str(FIXTURE)]) == 0
    out = json.loads(capsys.readouterr().out)
    assert out["schema"] == "emberwatch.sim_mission.v1" and out["simulation"] is True


def test_wind_uses_nearest_station_not_latest_anywhere(snap):
    def obs(sid, lon, lat, t, speed):
        return {"id": sid + t, "run_id": "r", "provenance": {"source_url": f"https://example.org/{sid}"},
                "station_id": sid, "position": [lon, lat], "valid_at": t, "forecast": False,
                "wind_speed_mps": speed, "wind_from_deg": 90.0}
    near_old = obs("near", -8.40, 40.74, "2026-11-14T11:00:00Z", 2.0)
    near_new = obs("near", -8.40, 40.74, "2026-11-14T12:00:00Z", 3.0)
    far_newest = obs("far", -9.14, 38.72, "2026-11-14T12:30:00Z", 9.0)  # Lisbon, ~240 km away
    no_pos = dict(obs("nopos", 0, 0, "2026-11-14T12:45:00Z", 7.0), position=None)
    snap["weather"] = [far_newest, near_old, no_pos, near_new]
    c = mission_from_snapshot(snap).conditions
    assert c.weather_station_id == "near" and c.wind_speed_mps == 3.0
    assert c.weather_station_distance_m < 5000
    snap["weather"] = [far_newest]  # only a distant station: wind stays unknown
    assert mission_from_snapshot(snap).conditions.wind_speed_mps is None
