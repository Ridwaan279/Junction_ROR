from pathlib import Path
import xml.etree.ElementTree as ET

from emberwatch_drone.recon import ReconFlight, make_world, route_error


def task():
    return {"simulation": True, "event_stale": False,
            "incident": {"mode": "synthetic", "position": [-8.367, 40.735]}}


def test_camera_is_mounted_on_existing_drone_and_source_preserved(tmp_path):
    source = Path(__file__).parents[1] / "worlds/emberwatch_scene.sdf"
    before = source.read_bytes()
    output = tmp_path / "recon.sdf"
    make_world(source, output)
    assert source.read_bytes() == before
    root = ET.parse(output).getroot()
    sensor = root.find("world/model[@name='ember_drone']/link[@name='base_link']/sensor")
    assert sensor.get("type") == "camera"
    assert sensor.findtext("topic") == "/emberwatch/recon/camera"
    assert sensor.findtext("camera/image/width") == "640"


def test_recon_returns_to_measured_start_without_water():
    flight = ReconFlight(observe_s=2)
    home = (-12., -8., .3)
    flight.assign(task(), home, 0)
    assert flight.state == "travelling"
    # Reach each commanded goal; assertions cover the complete state path.
    flight.step((-12., -8., 12.), 1, True)
    flight.step((0., -4., 18.), 2, True)
    assert flight.state == "observing"
    flight.step((0., -4., 18.), 5, True)
    assert flight.state == "returning"
    flight.step((-12., -8., 12.), 6, True)
    flight.step(home, 7, True)
    assert flight.state == "complete"
    assert flight.step(home, 8, True) == (0., 0., 0.)


def test_early_operator_recall_and_stale_odometry_stop():
    flight = ReconFlight()
    flight.assign(task(), (-10., -8., .3), 0)
    flight.step((-10., -8., 4.), 1, True, recall=True)
    assert flight.state == "returning"
    assert flight.step(None, 2, False) == (0., 0., 0.)
    assert flight.state == "failed"
    assert "stale_telemetry" in flight.note


def test_stale_missing_unreachable_and_blocked_fail_closed():
    stale = task() | {"event_stale": True}
    assert "stale_event" in route_error(stale)
    assert "unreachable" in route_error(task() | {"incident": {"position": None}})
    assert "unreachable" in route_error(task() | {"incident": {"position": [0, 0]}})
    assert "no_route" in route_error(task() | {"simulation": False})
    flight = ReconFlight()
    flight.assign(task(), (0, 0, 0), 0, blocked=True)
    assert flight.state == "failed"
    assert "no_route" in flight.note
    assert flight.step((0, 0, 0), 1, True) == (0., 0., 0.)
