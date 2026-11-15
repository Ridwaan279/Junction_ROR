"""Roof safety invariants for representative procedural buildings."""
import math

import numpy as np
import pytest

from emberwatch_drone.arganil.buildings import _simple_roof, build_buildings
from emberwatch_drone.arganil.geodata import Building


class SlopedTerrain:
    def z_map(self, x, y):
        return np.asarray(x) * 0.2 + np.asarray(y) * 0.1


def _vertices(path):
    return np.asarray([[float(v) for v in l.split()[1:]] for l in path.read_text().splitlines() if l.startswith('v ')])


@pytest.mark.parametrize('osm_id,expected', [(4, 'gabled'), (5, 'hipped')])
def test_pitched_roofs_stay_inside_footprint_and_original_height(tmp_path, osm_id, expected):
    poly = [(0., 0.), (12., 0.), (12., 6.), (0., 6.)]
    rec = build_buildings(SlopedTerrain(), [Building(osm_id, poly, 7., 'estimated_default')], str(tmp_path))[0]
    assert rec['roof_shape'] == expected
    assert rec['top_z'] == pytest.approx(8.5)  # mean DEM + original nominal TOTAL height
    for name in ('buildings_walls.obj', 'buildings_roofs.obj'):
        p = tmp_path / name
        v = _vertices(p)
        assert np.isfinite(v).all()
        assert (v[:, 0] >= 0).all() and (v[:, 0] <= 12).all()
        assert (v[:, 1] >= 0).all() and (v[:, 1] <= 6).all()
        assert (v[:, 2] <= rec['top_z'] + 0.005).all()
        # Every face references matching per-vertex UVs and normals.
        for l in p.read_text().splitlines():
            if l.startswith('f '):
                assert all(len(set(map(int, ref.split('/')))) == 1 for ref in l.split()[1:])
    assert _vertices(tmp_path / 'buildings_roofs.obj')[:, 2].min() < rec['top_z']
    assert 'representative' in rec['roof_basis']


def test_concave_roof_retains_safe_exact_footprint():
    poly = [(0, 0), (8, 0), (8, 3), (3, 3), (3, 8), (0, 8)]
    shape, eave, roofs, gables = _simple_roof(poly, 10., 6., 5)
    assert shape == 'flat' and eave == 10. and roofs == [] and gables == []


def test_convex_roof_is_pyramidal_and_deterministic(tmp_path):
    poly = [(0, 0), (6, 0), (8, 3), (5, 7), (0, 5)]
    b = Building(7, poly, 6., 'estimated_default')
    a = build_buildings(SlopedTerrain(), [b], str(tmp_path))
    first = {p.name: p.read_bytes() for p in tmp_path.iterdir()}
    assert a[0]['roof_shape'] == 'pyramidal'
    assert build_buildings(SlopedTerrain(), [b], str(tmp_path)) == a
    assert first == {p.name: p.read_bytes() for p in tmp_path.iterdir()}


def test_mapped_levels_control_windows_not_sunken_foundation(tmp_path):
    from emberwatch_drone.arganil.buildings import _storeys
    one = Building(8, [(0., 0.), (6., 0.), (6., 5.), (0., 5.)], 4., 'osm:building:levels')
    two = Building(9, one.poly, 7., 'osm:building:levels')
    assert _storeys(one) == 1 and _storeys(two) == 2
    r = build_buildings(SlopedTerrain(), [one], str(tmp_path))[0]
    assert r['storeys_visual'] == 1
    # Small houses have at most one window on each of four walls; a door may
    # replace a street-level window. No repeated basement window row is drawn.
    assert r['windows_visual'] <= 4
    assert r['doors_visual'] == 1


def test_door_faces_nearest_outward_road_and_palettes_vary(tmp_path):
    from emberwatch_drone.arganil.buildings import _street_side
    poly = [(0., 0.), (10., 0.), (10., 6.), (0., 6.)]
    roads = [[(-20., -3.), (30., -3.)]]
    side, basis = _street_side(poly, roads)
    assert side == 0 and 'mapped-road' in basis
    blds = [Building(i, poly, 7., 'osm:building:levels') for i in range(20, 28)]
    r = build_buildings(SlopedTerrain(), blds, str(tmp_path), roads=roads)
    assert len({tuple(b['wall_palette_rgb']) for b in r}) >= 3
    assert len({b['roof_tone'] for b in r}) >= 3
    assert all(b['door_side'] == 0 for b in r)
    assert all(b['doors_visual'] == 1 for b in r)
