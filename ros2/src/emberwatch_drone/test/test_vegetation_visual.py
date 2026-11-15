"""Geometry guarantees for the representative procedural forest."""
import numpy as np
import pytest

from emberwatch_drone.arganil.vegetation import build_tree_meshes


def _vertices(path):
    return np.array([list(map(float, line.split()[1:])) for line in path.read_text().splitlines()
                     if line.startswith('v ')])


@pytest.mark.parametrize('kind', ['pine', 'eucalyptus', 'broadleaf'])
@pytest.mark.parametrize('x', [0.0, 1200.0])
def test_crowns_stay_inside_collision_and_planner_envelope(tmp_path, kind, x):
    tree = {'x': x, 'y': 2.0, 'z': 3.0, 'h': 14.0, 'r': 3.2, 'kind': kind}
    # A partial forest also handles absent species without malformed OBJ indices.
    build_tree_meshes([tree], str(tmp_path))
    col = _vertices(tmp_path / 'trees_collision.obj')
    pieces = [_vertices(tmp_path / 'trees_trunks.obj'),
              _vertices(tmp_path / f'trees_crowns_{kind}.obj')]
    card = _vertices(tmp_path / 'trees_cards.obj')
    if len(card):
        pieces.append(card)
    visual = np.concatenate(pieces)
    ring = col[:6, :2]
    for a, b in zip(ring, np.roll(ring, -1, axis=0)):
        d, rel = b - a, visual[:, :2] - a
        assert np.all(d[0] * rel[:, 1] - d[1] * rel[:, 0] >= -0.05)
    assert np.max(np.linalg.norm(visual[:, :2] - [tree['x'], tree['y']], axis=1)) <= tree['r']
    assert visual[:, 2].min() >= col[:, 2].min() - 0.01
    assert visual[:, 2].max() <= col[:, 2].max() + 0.01
    for path in tmp_path.glob('trees_crowns_*.obj'):
        lines = path.read_text().splitlines()
        assert sum(line.startswith('v ') for line in lines) == sum(line.startswith('vt ') for line in lines)
    for image in ['trees_bark.jpg', f'trees_foliage_{kind}.jpg', 'trees_leaves.png']:
        assert (tmp_path / image).is_file()


def test_geometry_and_procedural_textures_are_deterministic(tmp_path):
    a, b = tmp_path / 'a', tmp_path / 'b'
    a.mkdir()
    b.mkdir()
    trees = [{'x': i * 1500.0, 'y': 0.0, 'z': 0.0, 'h': 12.0, 'r': 3.0, 'kind': kind}
             for i, kind in enumerate(['pine', 'eucalyptus', 'broadleaf'])]
    before = [dict(tree) for tree in trees]
    build_tree_meshes(trees, str(a))
    build_tree_meshes(trees, str(b))
    assert trees == before
    assert sorted(p.name for p in a.iterdir()) == sorted(p.name for p in b.iterdir())
    assert all(p.read_bytes() == (b / p.name).read_bytes() for p in a.iterdir())


def test_canopy_excludes_roads_buildings_and_preserves_vertex_maxima(tmp_path):
    from types import SimpleNamespace
    from emberwatch_drone.arganil.vegetation import build_canopy

    class Ground:
        def z_map(self, e, n):
            return 0.12 * e + 0.04 * n

    class Green:
        def rgb_map(self, e, n):
            return np.zeros(np.shape(e) + (3,)) + [30, 46, 20]

    feats = SimpleNamespace(lines=[('road', [(-80, 0), (80, 0)])], areas=[])
    buildings = [{'poly': [(15, 15), (35, 15), (35, 35), (15, 35)]}]
    c = build_canopy(Ground(), feats, buildings, Green(), str(tmp_path), res=10, half_m=80)
    assert c['mask'].shape == (16, 16)
    assert c['mask'].sum() > 0
    # Both rows that touch the road and all building-intersecting cells are empty.
    assert not c['mask'][7:9, :].any()
    assert not c['mask'][9:12, 9:12].any()
    verts = _vertices(tmp_path / 'trees_canopy.obj')
    faces = [list(map(lambda x: int(x.split('/')[0]) - 1, line.split()[1:]))
             for line in (tmp_path / 'trees_canopy.obj').read_text().splitlines() if line.startswith('f ')]
    for face in faces:
        tri = verts[face]
        center = tri[:, :2].mean(0)
        col, row = np.floor((center + 80 - 1e-5) / 10).astype(int)
        candidates = [(rr, cc) for rr in (row, row + 1) for cc in (col, col + 1)
                      if 0 <= rr < 16 and 0 <= cc < 16 and c['mask'][rr, cc]]
        assert candidates
        assert tri[:, 2].max() <= max(c['top'][rr, cc] for rr, cc in candidates) + 0.02
    assert len(faces) >= 4 * c['mask'].sum()


def test_canopy_is_not_invented_without_satellite(tmp_path):
    from types import SimpleNamespace
    from emberwatch_drone.arganil.vegetation import build_canopy
    assert build_canopy(None, SimpleNamespace(), [], None, str(tmp_path)) is None
    assert not any(line.startswith('v ') for line in (tmp_path / 'trees_canopy.obj').read_text().splitlines())


def test_canopy_fine_coarse_seam_has_shared_edges_and_smooth_unit_normals(tmp_path):
    from collections import Counter
    from types import SimpleNamespace
    from emberwatch_drone.arganil.vegetation import build_canopy

    class Ground:
        def z_map(self, e, n):
            return 0.08 * e - 0.05 * n

    class Green:
        def rgb_map(self, e, n):
            return np.zeros(np.shape(e) + (3,)) + [30, 46, 20]

    result = build_canopy(Ground(), SimpleNamespace(lines=[], areas=[]), [], Green(),
                          str(tmp_path), half_m=80, refine_half_m=40)
    lines = (tmp_path / 'trees_canopy.obj').read_text().splitlines()
    vertices = _vertices(tmp_path / 'trees_canopy.obj')
    normals = np.array([list(map(float, line.split()[1:])) for line in lines if line.startswith('vn ')])
    assert len(normals) == len(vertices)
    assert np.isfinite(normals).all()
    assert np.allclose(np.linalg.norm(normals, axis=1), 1, atol=1e-4)
    assert (tmp_path / 'trees_canopy_normal.png').is_file()
    # Surface triangles must use a conforming mesh at the 5 m / 10 m seam.
    # Remove vertical skirts from this edge count; only the exterior perimeter
    # may then have an edge used by a single upper-surface triangle.
    counts = Counter()
    for line in lines:
        if not line.startswith('f '):
            continue
        face = [int(item.split('/')[0]) - 1 for item in line.split()[1:]]
        xy = vertices[face, :2]
        delta1, delta2 = xy[1] - xy[0], xy[2] - xy[0]
        if abs(delta1[0] * delta2[1] - delta1[1] * delta2[0]) < 1e-5:
            continue
        for a, b in zip(face, face[1:] + face[:1]):
            counts[tuple(sorted((a, b)))] += 1
    for (a, b), count in counts.items():
        assert count in (1, 2)
        if count == 1:
            xy = vertices[[a, b], :2]
            assert any(np.allclose(xy[:, axis], boundary) for axis in (0, 1) for boundary in (-80, 80))
    assert result['mask'].sum() == 256
