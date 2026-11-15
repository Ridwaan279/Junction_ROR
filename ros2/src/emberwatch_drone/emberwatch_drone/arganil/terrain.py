"""Terrain geometry and licensed imagery resampling."""
from __future__ import annotations

import math
import os
import numpy as np
from matplotlib.path import Path as MplPath
from PIL import Image, ImageDraw
from .geodata import (DEFAULT_BUILDING_H_M, DEFAULT_LEVEL_H_M, MAP_HALF_M, MAP_ORIGIN, Building, Satellite, Terrain, load_osm)

from .mesh import Obj
TERRAIN_STEP_M = 20.0
TEX_PX = 2048
CORE_TEX_PX = 1024
OUTER_TEX_PX = 256

def build_terrain(t: Terrain, out: str) -> tuple[np.ndarray, np.ndarray]:
    g = np.arange(-MAP_HALF_M, MAP_HALF_M + 0.1, TERRAIN_STEP_M)
    E, N = np.meshgrid(g, g)  # rows = north
    Z = t.z_map(E, N)
    n = len(g)
    o = Obj()
    for r in range(n):
        for c in range(n):
            o.add_v(E[r, c], N[r, c], Z[r, c])
            o.vt.append(((E[r, c] + MAP_HALF_M) / (2 * MAP_HALF_M), (N[r, c] + MAP_HALF_M) / (2 * MAP_HALF_M)))
    for r in range(n - 1):
        for c in range(n - 1):
            a = r * n + c + 1
            b, cc, d = a + 1, a + n, a + n + 1
            o.f += [(a, b, d), (a, d, cc)]  # CCW seen from +z
    o.write(os.path.join(out, "terrain.obj"), uv=True)
    return g, Z


def satellite_texture(sat, out: str, px: int = 4096) -> None:
    """Terrain texture = Sentinel-2 cloudless 2016 (EOX, CC BY 4.0) resampled into the map frame."""
    g = np.linspace(-MAP_HALF_M, MAP_HALF_M, px)
    rows = []
    for chunk in np.array_split(g[::-1], 16):  # row 0 = north
        E, N = np.meshgrid(g, chunk)
        rows.append(sat.rgb_map(E, N))
    rgb = np.concatenate(rows, 0) / 255.0
    rgb = np.clip(rgb, 0, 1) ** 1.0                     # retain source radiometry: no exposure lift
    grey = rgb.mean(-1, keepdims=True)
    rgb = np.clip(grey + (rgb - grey) * 1.0, 0, 1)              # retain source saturation
    Image.fromarray((rgb * 255).astype(np.uint8)).save(os.path.join(out, "terrain.jpg"), quality=90)


def paint_texture(t: Terrain, feats, out: str) -> None:
    px = TEX_PX
    s = px / (2 * MAP_HALF_M)

    def P(x, y):
        return ((x + MAP_HALF_M) * s, (MAP_HALF_M - y) * s)  # image row 0 = north

    # base colour by elevation + slope (no landcover data => generic dry Mediterranean hillside)
    g = np.linspace(-MAP_HALF_M, MAP_HALF_M, 512)
    E, N = np.meshgrid(g, g[::-1])
    Z = t.z_map(E, N)
    gy, gx = np.gradient(Z, g[1] - g[0])
    slope = np.clip(np.hypot(gx, gy), 0, 1.2) / 1.2
    hn = np.clip((Z - Z.min()) / max(1.0, np.ptp(Z)), 0, 1)
    base = np.stack([105 + 40 * slope + 20 * hn, 112 + 10 * hn - 10 * slope, 70 + 15 * slope], -1)
    img = Image.fromarray(base.clip(0, 255).astype(np.uint8)).resize((px, px), Image.BILINEAR)
    d = ImageDraw.Draw(img, "RGBA")
    colors = {"forest": (46, 78, 38, 235), "scrub": (96, 104, 58, 220), "grass": (120, 150, 70, 200),
              "farmland": (150, 150, 85, 200), "orchard": (95, 130, 60, 210), "residential": (150, 140, 128, 190),
              "water": (60, 100, 140, 255)}
    for cls in ["farmland", "grass", "orchard", "scrub", "forest", "residential", "water"]:
        for c2, poly in feats.areas:
            if c2 == cls and len(poly) >= 3:
                d.polygon([P(*p) for p in poly], fill=colors[cls])
    for kind, line in feats.lines:
        w = 7 if kind == "river" else 3
        d.line([P(*p) for p in line], fill=(70, 110, 150, 255) if kind == "river" else (85, 82, 78, 255), width=w)
    for b in feats.buildings:
        d.polygon([P(*p) for p in b.poly], fill=(120, 110, 100, 255))
    img.save(os.path.join(out, "terrain.jpg"), quality=90)




def sample_collider_surface(mesh_g, mesh_z, east, north):
    """Sample the authored 20m SW-to-NE triangles, including OBJ 2dp positions.

    Bilinear sampling would depart a nonplanar quad's diagonal by metres. This
    piecewise-linear sampler preserves the collider's planes when subdividing.
    """
    g = np.asarray(mesh_g, dtype=float)
    z = np.round(np.asarray(mesh_z, dtype=float), 2)
    e, n = np.broadcast_arrays(np.asarray(east, dtype=float), np.asarray(north, dtype=float))
    c = np.clip(np.searchsorted(g, e, side='right') - 1, 0, len(g) - 2)
    r = np.clip(np.searchsorted(g, n, side='right') - 1, 0, len(g) - 2)
    u = np.clip((e - g[c]) / (g[c + 1] - g[c]), 0, 1)
    v = np.clip((n - g[r]) / (g[r + 1] - g[r]), 0, 1)
    a, b, cc, d = z[r, c], z[r, c + 1], z[r + 1, c], z[r + 1, c + 1]
    return np.where(u >= v, a * (1 - u) + b * (u - v) + d * v,
                    a * (1 - v) + cc * (v - u) + d * u)


def global_terrain_normals(mesh_g, mesh_z):
    """Area-weighted normals from the entire collider, shared by every tile.

    Chunk-local normal accumulation creates discontinuities at tile borders.
    This computes each source normal with all its global neighbouring faces.
    """
    g = np.asarray(mesh_g, dtype=float)
    e, n = np.meshgrid(g, g)
    vertices = np.stack([e, n, np.round(np.asarray(mesh_z, dtype=float), 2)], axis=-1).reshape(-1, 3)
    count = len(g)
    r, c = np.mgrid[:count - 1, :count - 1]
    a = (r * count + c).ravel()
    faces = np.concatenate([np.stack([a, a + 1, a + count + 1], axis=-1),
                            np.stack([a, a + count + 1, a + count], axis=-1)])
    edge1 = vertices[faces[:, 1]] - vertices[faces[:, 0]]
    edge2 = vertices[faces[:, 2]] - vertices[faces[:, 0]]
    face_normals = np.cross(edge1, edge2)
    normals = np.zeros_like(vertices)
    for k in range(3):
        np.add.at(normals, faces[:, k], face_normals)
    normals /= np.maximum(np.linalg.norm(normals, axis=1, keepdims=True), 1e-12)
    return normals.reshape(count, count, 3)


def sample_terrain_normals(mesh_g, normals, east, north):
    """Globally interpolate unit normals at subdivisions, including chunk seams."""
    g = np.asarray(mesh_g, dtype=float)
    e, n = np.broadcast_arrays(np.asarray(east, dtype=float), np.asarray(north, dtype=float))
    c = np.clip(np.searchsorted(g, e, side='right') - 1, 0, len(g) - 2)
    r = np.clip(np.searchsorted(g, n, side='right') - 1, 0, len(g) - 2)
    u = np.clip((e - g[c]) / (g[c + 1] - g[c]), 0, 1)[..., None]
    v = np.clip((n - g[r]) / (g[r + 1] - g[r]), 0, 1)[..., None]
    ns = ((normals[r, c] * (1 - u) + normals[r, c + 1] * u) * (1 - v)
          + (normals[r + 1, c] * (1 - u) + normals[r + 1, c + 1] * u) * v)
    return ns / np.maximum(np.linalg.norm(ns, axis=-1, keepdims=True), 1e-12)


def terrain_chunk_mesh(mesh_g, mesh_z, global_normals, bounds, step_m=20.):
    """Return a UV mesh and explicit global normals, without changing the surface."""
    x0, y0, x1, y1 = bounds
    intervals = int(round((x1 - x0) / step_m))
    if step_m not in (TERRAIN_STEP_M, TERRAIN_STEP_M / 2) or abs(intervals * step_m - (x1 - x0)) > 1e-6:
        raise ValueError('terrain subdivision must be 20m or 10m on the original grid')
    e = np.linspace(x0, x1, intervals + 1)
    n = np.linspace(y0, y1, intervals + 1)
    E, N = np.meshgrid(e, n)
    Z = sample_collider_surface(mesh_g, mesh_z, E, N)
    normals = sample_terrain_normals(mesh_g, global_normals, E, N).reshape(-1, 3)
    o = Obj()
    for x, y, z in zip(E.ravel(), N.ravel(), Z.ravel()):
        o.add_v(x, y, z)
        o.vt.append(((x - x0) / (x1 - x0), (y - y0) / (y1 - y0)))
    side = len(e)
    for r in range(intervals):
        for c in range(intervals):
            a = r * side + c + 1
            o.f.extend([(a, a + 1, a + side + 1), (a, a + side + 1, a + side)])
    return o, normals


def build_terrain_chunks(sat, ortho, out: str, mesh_g, mesh_z, core_step_m=20.):
    """500m textured visual tiles with globally consistent smooth vertex normals.

    Optional 10m core subdivision adds polygons on the existing 20m triangle
    planes, without inventing finer DEM information. 3dp OBJ positions preserve subdivided half-centimetre heights exactly.
    Collider/raster/fire surface stay unchanged; visual offset is numerical only.
    Core textures <=0.5m/px. The 100m seam blends DGT2018 to EOX2016 outside.
    """
    import json
    chunks = []
    normals = global_terrain_normals(mesh_g, mesh_z)
    for iy, y0 in enumerate(np.arange(-MAP_HALF_M, MAP_HALF_M, 500.)):
        for ix, x0 in enumerate(np.arange(-MAP_HALF_M, MAP_HALF_M, 500.)):
            core = ortho is not None and abs(x0 + 250) <= 1250 and abs(y0 + 250) <= 1250
            px = CORE_TEX_PX if core else OUTER_TEX_PX
            eg = x0 + (np.arange(px) + .5) * 500 / px
            ng = y0 + 500 - (np.arange(px) + .5) * 500 / px
            E, N = np.meshgrid(eg, ng)
            rgb = sat.rgb_map(E, N) if sat is not None else np.full((*E.shape, 3), 100.)
            if core:
                fine = ortho.rgb_map(E, N)
                alpha = np.clip((1500 - np.maximum(np.abs(E), np.abs(N))) / 100, 0, 1)[..., None]
                rgb = fine * alpha + rgb * (1 - alpha)
            basename = f'terrain_{iy:02d}_{ix:02d}'
            Image.fromarray(np.clip(rgb, 0, 255).astype(np.uint8)).save(os.path.join(out, basename + '.jpg'), quality=94 if core else 88)
            bounds = [x0, y0, x0 + 500, y0 + 500]
            step = core_step_m if core else TERRAIN_STEP_M
            o, explicit_normals = terrain_chunk_mesh(mesh_g, mesh_z, normals, bounds, step)
            o.write(os.path.join(out, basename + '.obj'), uv=True, normals=explicit_normals, vertex_precision=3)
            chunks.append({'mesh': basename + '.obj', 'texture': basename + '.jpg', 'bounds': bounds,
                           'gsd_m': 500 / px, 'source': 'DGT2018 CC BY4.0 / 100m seam blend' if core else 'EOX2016 CC BY4.0',
                           'geometry_step_m': step, 'normal_basis': 'global area-weighted collider normals; interpolated at subdivisions',
                           'max_surface_offset_m': 1e-9, 'vertex_precision_decimals': 3})
    with open(os.path.join(out, 'terrain_chunks.json'), 'w') as f:
        json.dump(chunks, f, indent=2)
    return chunks
