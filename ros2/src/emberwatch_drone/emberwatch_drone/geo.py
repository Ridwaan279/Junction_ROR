"""Coordinate helpers.

Frames used in this package
---------------------------
* GeoJSON / EmberWatch contract: positions are ``[longitude, latitude]`` (WGS84 degrees).
* ROS (REP 103) and Gazebo world frame: **ENU** -- x = East, y = North, z = Up.
* PX4 local frame: **NED** -- x = North, y = East, z = Down. PX4 ``TrajectorySetpoint``
  positions are NED metres relative to the EKF local origin (``ref_lat``/``ref_lon``
  published in ``VehicleLocalPosition``).

ENU -> NED is a swap of the horizontal axes and a sign flip of the vertical axis::

    n = y_enu,  e = x_enu,  d = -z_enu

Yaw: ENU yaw is measured from East, counter-clockwise; NED yaw is measured from
North, clockwise.  ``yaw_ned = pi/2 - yaw_enu`` (wrapped to [-pi, pi]).

Global -> local uses the same azimuthal-equidistant projection as PX4's
``MapProjection`` (``src/lib/geo``), so a target computed here agrees with what PX4
would compute from the same reference point.  It is only meant for short ranges
(a few km); it is not a survey-grade transformation.
"""

from __future__ import annotations

import math

EARTH_RADIUS_M = 6_371_000.0  # PX4 CONSTANTS_RADIUS_OF_EARTH


def wrap_pi(angle: float) -> float:
    return (angle + math.pi) % (2.0 * math.pi) - math.pi


def geodetic_to_local_ned(lat: float, lon: float, ref_lat: float, ref_lon: float) -> tuple[float, float]:
    """Return (north_m, east_m) of (lat, lon) relative to (ref_lat, ref_lon)."""
    lat_r, lon_r = math.radians(lat), math.radians(lon)
    rlat, rlon = math.radians(ref_lat), math.radians(ref_lon)
    sin_lat, cos_lat = math.sin(lat_r), math.cos(lat_r)
    cos_d_lon = math.cos(lon_r - rlon)
    arg = max(-1.0, min(1.0, math.sin(rlat) * sin_lat + math.cos(rlat) * cos_lat * cos_d_lon))
    c = math.acos(arg)
    k = 1.0 if abs(c) < 1e-12 else c / math.sin(c)
    north = k * (math.cos(rlat) * sin_lat - math.sin(rlat) * cos_lat * cos_d_lon) * EARTH_RADIUS_M
    east = k * cos_lat * math.sin(lon_r - rlon) * EARTH_RADIUS_M
    return north, east


def local_ned_to_geodetic(north: float, east: float, ref_lat: float, ref_lon: float) -> tuple[float, float]:
    """Inverse of :func:`geodetic_to_local_ned` (PX4 ``MapProjection::reproject``)."""
    x_rad, y_rad = north / EARTH_RADIUS_M, east / EARTH_RADIUS_M
    c = math.hypot(x_rad, y_rad)
    rlat, rlon = math.radians(ref_lat), math.radians(ref_lon)
    if c < 1e-12:
        return ref_lat, ref_lon
    sin_c, cos_c = math.sin(c), math.cos(c)
    lat = math.asin(cos_c * math.sin(rlat) + (x_rad * sin_c * math.cos(rlat)) / c)
    lon = rlon + math.atan2(y_rad * sin_c, c * math.cos(rlat) * cos_c - x_rad * math.sin(rlat) * sin_c)
    return math.degrees(lat), math.degrees(lon)


def enu_to_ned(x: float, y: float, z: float) -> tuple[float, float, float]:
    return y, x, -z


def ned_to_enu(n: float, e: float, d: float) -> tuple[float, float, float]:
    return e, n, -d


def yaw_enu_to_ned(yaw_enu: float) -> float:
    return wrap_pi(math.pi / 2.0 - yaw_enu)
