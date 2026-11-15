import math

import pytest

from emberwatch_drone.geo import (enu_to_ned, geodetic_to_local_ned, local_ned_to_geodetic, ned_to_enu,
                                  yaw_enu_to_ned)


def test_origin_is_zero():
    assert geodetic_to_local_ned(40.735, -8.367, 40.735, -8.367) == pytest.approx((0.0, 0.0), abs=1e-6)


def test_north_and_east_axes():
    n, e = geodetic_to_local_ned(40.745, -8.367, 40.735, -8.367)  # 0.01 deg north
    assert n == pytest.approx(1111.95, rel=1e-3) and abs(e) < 1e-6
    n, e = geodetic_to_local_ned(40.735, -8.357, 40.735, -8.367)  # 0.01 deg east
    assert e == pytest.approx(1111.95 * math.cos(math.radians(40.735)), rel=1e-3) and abs(n) < 0.1


def test_roundtrip():
    lat, lon = local_ned_to_geodetic(-750.0, 1234.0, 40.735, -8.367)
    n, e = geodetic_to_local_ned(lat, lon, 40.735, -8.367)
    assert (n, e) == pytest.approx((-750.0, 1234.0), abs=1e-3)


def test_enu_ned_swap():
    assert enu_to_ned(1.0, 2.0, 3.0) == (2.0, 1.0, -3.0)
    assert ned_to_enu(*enu_to_ned(1.0, 2.0, 3.0)) == (1.0, 2.0, 3.0)


def test_yaw():
    assert yaw_enu_to_ned(0.0) == pytest.approx(math.pi / 2)  # facing east
    assert yaw_enu_to_ned(math.pi / 2) == pytest.approx(0.0)  # facing north
