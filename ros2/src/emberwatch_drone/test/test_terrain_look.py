"""Smoother terrain preserves collider planes and deterministic seam normals."""
import numpy as np
import pytest

from emberwatch_drone.arganil import terrain


def _grid():
    g = np.arange(-500., 501., 20.)
    e, n = np.meshgrid(g, g)
    return g, .0002 * e * e + 8 * np.sin(n / 90) + 3 * np.cos(e / 70) * np.sin(n / 60)


def test_triangle_sampler_preserves_authored_diagonal_not_bilinear_quad():
    g = np.array([0., 20.])
    z = np.array([[0., 1.], [3., 5.]])
    assert terrain.sample_collider_surface(g, z, 15., 5.) == pytest.approx(1.75)
    assert terrain.sample_collider_surface(g, z, 5., 15.) == pytest.approx(2.75)
    assert terrain.sample_collider_surface(g, z, 10., 10.) == pytest.approx(2.5)


def test_global_normals_unit_length_upward_and_shared_across_chunk_border():
    g, z = _grid()
    normals = terrain.global_terrain_normals(g, z)
    assert np.allclose(np.linalg.norm(normals, axis=-1), 1)
    assert np.all(normals[..., 2] > 0)
    left, ln = terrain.terrain_chunk_mesh(g, z, normals, [-500., -500., 0., 0.], 10.)
    right, rn = terrain.terrain_chunk_mesh(g, z, normals, [0., -500., 500., 0.], 10.)
    lv, rv = np.asarray(left.v).reshape(51, 51, 3), np.asarray(right.v).reshape(51, 51, 3)
    assert np.array_equal(lv[:, -1], rv[:, 0])
    assert np.array_equal(ln.reshape(51, 51, 3)[:, -1], rn.reshape(51, 51, 3)[:, 0])
    assert len(left.f) == 4 * 25 * 25 * 2
    assert np.isfinite(ln).all()


def test_fine_coarse_boundary_is_collinear_and_normal_consistent():
    g, z = _grid()
    normals = terrain.global_terrain_normals(g, z)
    fine, fn = terrain.terrain_chunk_mesh(g, z, normals, [-500., -500., 0., 0.], 10.)
    coarse, cn = terrain.terrain_chunk_mesh(g, z, normals, [0., -500., 500., 0.], 20.)
    fv, cv = np.asarray(fine.v).reshape(51, 51, 3), np.asarray(coarse.v).reshape(26, 26, 3)
    assert np.allclose(fv[::2, -1], cv[:, 0], atol=1e-10)
    assert np.allclose(fv[1::2, -1], (cv[:-1, 0] + cv[1:, 0]) / 2, atol=1e-10)
    assert np.allclose(fn.reshape(51, 51, 3)[::2, -1], cn.reshape(26, 26, 3)[:, 0], atol=1e-12)


def test_subdivision_planes_are_exact_at_serialized_precision():
    g, z = _grid()
    normals = terrain.global_terrain_normals(g, z)
    obj, _ = terrain.terrain_chunk_mesh(g, z, normals, [-500., -500., 0., 0.], 10.)
    v = np.round(np.asarray(obj.v), 3)  # exact explicit visual positions emitted by Obj.write
    faces = np.asarray(obj.f) - 1
    # Face centroids and non-central barycentric points independently sample
    # every authored fine triangle, including cells crossing coarse diagonals.
    for weights in ([1 / 3] * 3, [.1, .3, .6], [.7, .2, .1]):
        p = np.sum(v[faces] * np.asarray(weights)[None, :, None], axis=1)
        reference = terrain.sample_collider_surface(g, z, p[:, 0], p[:, 1])
        assert np.max(np.abs(p[:, 2] - reference)) <= 1e-9


def test_core_refinement_manifest_and_serialized_normals(tmp_path, monkeypatch):
    monkeypatch.setattr(terrain, 'MAP_HALF_M', 500.)
    monkeypatch.setattr(terrain, 'CORE_TEX_PX', 8)
    monkeypatch.setattr(terrain, 'OUTER_TEX_PX', 4)
    g, z = _grid()
    class RGB:
        def rgb_map(self, e, n):
            return np.zeros((*e.shape, 3)) + 100.
    chunks = terrain.build_terrain_chunks(RGB(), RGB(), str(tmp_path), g, z, core_step_m=10.)
    assert len(chunks) == 4
    for chunk in chunks:
        lines = (tmp_path / chunk['mesh']).read_text().splitlines()
        positions = [line for line in lines if line.startswith('v ')]
        normals = np.asarray([[float(v) for v in line.split()[1:]] for line in lines if line.startswith('vn ')])
        assert len(positions) == 51 ** 2
        assert np.allclose(np.linalg.norm(normals, axis=1), 1, atol=1e-4)
        assert chunk['geometry_step_m'] == 10.
        assert chunk['max_surface_offset_m'] == 1e-9
        assert chunk['vertex_precision_decimals'] == 3
