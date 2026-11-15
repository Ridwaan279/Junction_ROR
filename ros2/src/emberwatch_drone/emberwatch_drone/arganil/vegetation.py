"""Representative satellite-masked vegetation."""
from __future__ import annotations

import math
import os
import numpy as np
from matplotlib.path import Path as MplPath
from PIL import Image, ImageDraw
from .geodata import (DEFAULT_BUILDING_H_M, DEFAULT_LEVEL_H_M, MAP_HALF_M, MAP_ORIGIN, Building, Satellite, Terrain, load_osm)

from .mesh import Obj
TREE_SEED = 20261115
MAX_TREES = 15000

def _exclusion_mask(feats, blds, res=2.0):
    """Raster (res m) of places trees may NOT go: building footprints +3 m, roads +4 m, water."""
    n = int(2 * MAP_HALF_M / res)
    img = Image.new("L", (n, n), 0)
    d = ImageDraw.Draw(img)

    def P(x, y):
        return ((x + MAP_HALF_M) / res, (MAP_HALF_M - y) / res)

    for b in blds:
        d.polygon([P(*p) for p in b["poly"]], fill=255, outline=255, width=int(3 / res) * 2 + 1)
    for k, line in feats.lines:
        d.line([P(*p) for p in line], fill=255, width=int(8 / res) + 1)
    for c, poly in feats.areas:
        if c == "water":
            d.polygon([P(*p) for p in poly], fill=255)
    return np.asarray(img) > 0, res, n


def place_trees(t: Terrain, feats, blds: list[dict], sat=None) -> list[dict]:
    """Trees where the satellite image shows dark vegetation (if available), else in OSM forest
    polygons / scattered. Positions are REPRESENTATIVE (no individual tree inventory exists),
    except the few OSM natural=tree nodes."""
    rng = np.random.default_rng(TREE_SEED)
    excl, eres, en = _exclusion_mask(feats, blds)

    def free(X, Y):
        c = ((X + MAP_HALF_M) / eres).astype(int)
        r = ((MAP_HALF_M - Y) / eres).astype(int)
        ok = (c >= 0) & (c < en) & (r >= 0) & (r < en) & (np.abs(X) < MAP_HALF_M - 20) & (np.abs(Y) < MAP_HALF_M - 20)
        ok[ok] &= ~excl[r[ok], c[ok]]
        return ok

    trees: list[dict] = []
    for x, y in feats.trees:
        if free(np.array([x]), np.array([y]))[0]:
            trees.append({"x": x, "y": y, "h": 10.0, "r": 2.8, "kind": "broadleaf", "basis": "osm:natural=tree (height estimated)"})
    resid = [MplPath(p) for c, p in feats.areas if c == "residential"]
    if sat is not None:
        # dense near town (flight area), sparse farther out (the imagery carries the far forest)
        for radius, spacing, cap in ((1900.0, 8.5, MAX_TREES), (MAP_HALF_M, 34.0, MAX_TREES + 3500)):
            g = np.arange(-radius, radius, spacing)
            X, Y = np.meshgrid(g, g)
            X = (X + rng.uniform(-spacing * 0.45, spacing * 0.45, X.shape)).ravel()
            Y = (Y + rng.uniform(-spacing * 0.45, spacing * 0.45, Y.shape)).ravel()
            ring = (np.hypot(X, Y) <= radius) if radius < MAP_HALF_M else (np.hypot(X, Y) > 1900.0)
            X, Y = X[ring], Y[ring]
            rgb = sat.rgb_map(X, Y)
            v = rgb.mean(-1)
            exg = 2 * rgb[:, 1] - rgb[:, 0] - rgb[:, 2]
            score = np.clip((56 - v) / 16, 0, 1) * np.clip((exg - 5) / 8, 0, 1)  # dark + green = canopy
            for rp in resid:
                score[rp.contains_points(np.stack([X, Y], 1))] *= 0.25
            keep = (rng.uniform(size=score.shape) < score) & free(X, Y)
            X, Y = X[keep], Y[keep]
            order = rng.permutation(len(X))  # unbiased spatially when the cap is hit
            for x, y in zip(X[order], Y[order]):
                if len(trees) >= cap:
                    break
                u = rng.uniform()
                kind = "pine" if u < 0.5 else "eucalyptus" if u < 0.8 else "broadleaf"
                h = float(rng.uniform(9, 17) if kind != "broadleaf" else rng.uniform(6, 11))
                trees.append({"x": float(x), "y": float(y), "h": round(h, 2),
                              "r": round(h * (0.22 if kind == "pine" else 0.27 if kind == "eucalyptus" else 0.32), 2),
                              "kind": kind, "basis": "representative (satellite vegetation mask)"})
    else:
        veg = [MplPath(p) for c, p in feats.areas if c in ("forest", "scrub", "orchard")]
        while len(trees) < 1800:
            X, Y = rng.uniform(-MAP_HALF_M, MAP_HALF_M, (2, 4000))
            ok = free(X, Y)
            for x, y in zip(X[ok], Y[ok]):
                if len(trees) >= 1800:
                    break
                if any(p.contains_point((x, y)) for p in veg) or rng.uniform() < 0.3:
                    h = float(rng.uniform(6, 14))
                    trees.append({"x": float(x), "y": float(y), "h": h, "r": round(h * 0.28, 2), "kind": "broadleaf",
                                  "basis": "representative (scattered; no satellite)"})
    xs, ys = np.array([tr["x"] for tr in trees]), np.array([tr["y"] for tr in trees])
    zs = t.z_map(xs, ys)
    for tr, z in zip(trees, zs):
        tr["z"] = round(float(z), 2)
        tr["x"], tr["y"] = round(tr["x"], 2), round(tr["y"], 2)
    return trees


_ICO_V = np.array([(-1, 1.618, 0), (1, 1.618, 0), (-1, -1.618, 0), (1, -1.618, 0), (0, -1, 1.618), (0, 1, 1.618),
                   (0, -1, -1.618), (0, 1, -1.618), (1.618, 0, -1), (1.618, 0, 1), (-1.618, 0, -1), (-1.618, 0, 1)], float)
_ICO_V /= np.linalg.norm(_ICO_V, axis=1, keepdims=True)
_ICO_F = [(0, 11, 5), (0, 5, 1), (0, 1, 7), (0, 7, 10), (0, 10, 11), (1, 5, 9), (5, 11, 4), (11, 10, 2), (10, 7, 6),
          (7, 1, 8), (3, 9, 4), (3, 4, 2), (3, 2, 6), (3, 6, 8), (3, 8, 9), (4, 9, 5), (2, 4, 11), (6, 2, 10),
          (8, 6, 7), (9, 8, 1)]


def _tree_textures(out: str) -> None:
    """Small original opaque textures; no downloaded photographs or third-party assets.

    Repeating leaf/needle clusters break the old uniform plastic shading. This is
    a procedural approximation, not a botanical scan or an alpha-card shader.
    """
    rng = np.random.default_rng(TREE_SEED + 9)
    for kind, base in (("pine", (47, 75, 34)), ("eucalyptus", (91, 112, 83)),
                       ("broadleaf", (68, 98, 39))):
        noise = rng.normal(0, 7, (256, 256, 1))
        rgb = np.clip(np.array(base)[None, None, :] + noise, 0, 255).astype(np.uint8)
        img = Image.fromarray(rgb)
        draw = ImageDraw.Draw(img)
        for _ in range(3300):
            x, y = rng.integers(-8, 256, 2)
            shade = float(rng.uniform(0.55, 1.65))
            color = tuple(int(min(255, v * shade)) for v in base)
            if kind == "pine":
                dx, dy = rng.integers(-6, 7, 2)
                draw.line((int(x), int(y), int(x + dx), int(y + dy)), fill=color, width=1)
            elif kind == "eucalyptus":
                draw.ellipse((int(x), int(y), int(x + 3), int(y + 9)), fill=color)
            else:
                draw.ellipse((int(x), int(y), int(x + 6), int(y + 4)), fill=color)
        img.save(os.path.join(out, f"trees_foliage_{kind}.jpg"), quality=92)
    u, v = np.meshgrid(np.arange(256), np.arange(256))
    grooves = 12 * np.sin(u * 0.38 + 1.5 * np.sin(v * 0.03))
    noise = rng.normal(0, 5, (256, 256))
    bark = np.clip(np.array([99, 79, 59])[None, None, :] + (grooves + noise)[..., None], 0, 255)
    Image.fromarray(bark.astype(np.uint8)).save(os.path.join(out, "trees_bark.jpg"), quality=92)
    atlas = Image.new("RGBA", (768, 512), (0, 0, 0, 0))
    d = ImageDraw.Draw(atlas)
    for part, (kind, color) in enumerate((("pine", (48, 83, 34)), ("eucalyptus", (84, 117, 78)),
                                         ("broadleaf", (65, 105, 36)))):
        offset = part * 256
        d.line((offset + 128, 460, offset + 128, 90), fill=(99, 82, 51, 255), width=3)
        for j in range(900 if kind == "pine" else 380):
            x, y = rng.normal(128, 47), rng.normal(245, 105)
            if ((x - 128) / 105) ** 2 + ((y - 245) / 225) ** 2 > 1:
                continue
            x += offset
            c = tuple(int(min(255, v * rng.uniform(0.70, 1.38))) for v in color) + (255,)
            if kind == "pine":
                dx, dy = rng.uniform(-11, 11), rng.uniform(-16, 6)
                d.line((int(x), int(y), int(x + dx), int(y + dy)), fill=c, width=2)
            elif kind == "eucalyptus":
                d.ellipse((int(x - 3), int(y - 9), int(x + 3), int(y + 9)), fill=c)
            else:
                d.ellipse((int(x - 6), int(y - 5), int(x + 6), int(y + 5)), fill=c)
    atlas.save(os.path.join(out, "trees_leaves.png"))


def build_tree_meshes(trees: list[dict], out: str) -> None:
    """Batched textured crowns with branched, species-dependent silhouettes.

    Near-town trees use tested alpha-cutout crossed foliage cards and a small solid
    interior; distant trees use opaque clusters (fixed geometry budget, not dynamic LOD).
    Tree positions and planner dimensions are unchanged. All foliage is confined to
    0.83*r, inside even the inscribed six-sided collision prism (inradius 0.866*r).
    Trees remain representative; no surveyed species or individual crowns claimed.
    """
    rng = np.random.default_rng(TREE_SEED + 1)
    trunk, col, cards = Obj(), Obj(), Obj()
    crowns = {"pine": Obj(), "eucalyptus": Obj(), "broadleaf": Obj()}

    def branch(o, a, b, r0, r1, k=5, uv=True):
        a, b = np.array(a, float), np.array(b, float)
        axis = b - a
        axis /= max(np.linalg.norm(axis), 1e-9)
        tangent = np.cross(axis, [0, 1, 0])
        if np.linalg.norm(tangent) < 0.01:
            tangent = np.cross(axis, [1, 0, 0])
        tangent /= np.linalg.norm(tangent)
        bitangent = np.cross(axis, tangent)
        rings = []
        for j, (c, r) in enumerate(((a, r0), (b, r1))):
            ring = []
            for i in range(k):
                angle = 2 * math.pi * i / k
                q = c + r * (math.cos(angle) * tangent + math.sin(angle) * bitangent)
                ring.append(o.add_v(*q))
                if uv:
                    o.vt.append((i / k, j * np.linalg.norm(b - a) / 2))
            rings.append(ring)
        bot, top = rings
        for i in range(k):
            j = (i + 1) % k
            o.f += [(bot[i], bot[j], top[j]), (bot[i], top[j], top[i])]
        return bot, top

    def foliage(o, x, y, z, cx, cy, cz, rx, rz, radius, height):
        # The icosahedron is smoothed by the shared-vertex OBJ normals. The lobe
        # layout, texture and scale differ per tree instead of identical cones.
        jitter = rng.uniform(0.90, 1.06, len(_ICO_V))
        ids = []
        for i, v in enumerate(_ICO_V):
            dx, dy = cx + v[0] * rx * jitter[i], cy + v[1] * rx * jitter[i]
            d = math.hypot(dx, dy)
            if d > radius * 0.83:
                dx, dy = dx / d * radius * 0.83, dy / d * radius * 0.83
            zz = min(height, max(0.3, cz + v[2] * rz * jitter[i]))
            ids.append(o.add_v(x + dx, y + dy, z + zz))
            o.vt.append(((math.atan2(v[1], v[0]) / (2 * math.pi) + 0.5) * 2,
                         (math.asin(v[2]) / math.pi + 0.5) * 2))
        o.f += [(ids[a], ids[b], ids[c]) for a, b, c in _ICO_F]

    def leaf_cards(x, y, z, cx, cy, cz, radius, height, kind, phase):
        part = {"pine": 0, "eucalyptus": 1, "broadleaf": 2}[kind]
        for j in range(3):
            angle = phase + j * math.pi / 3
            along = np.array([math.cos(angle), math.sin(angle)]) * radius * 0.43
            xy0, xy1 = np.array([cx, cy]) - along, np.array([cx, cy]) + along
            verts = [(xy0[0], xy0[1], cz - height * 0.16),
                     (xy1[0], xy1[1], cz - height * 0.16),
                     (xy1[0], xy1[1], cz + height * 0.16),
                     (xy0[0], xy0[1], cz + height * 0.16)]
            # Separate back vertices give correct normals with the frozen OBJ
            # writer, avoiding dependence on custom two-sided shader settings.
            for order in ((0, 1, 2, 3), (3, 2, 1, 0)):
                ids = []
                for k in order:
                    xx, yy, zz = verts[k]
                    rxy = math.hypot(xx, yy)
                    if rxy > radius * 0.83:
                        xx, yy = xx / rxy * radius * 0.83, yy / rxy * radius * 0.83
                    ids.append(cards.add_v(x + xx, y + yy, z + min(height, max(0.3, zz))))
                    uv = ((0, 0), (1, 0), (1, 1), (0, 1))[k]
                    cards.vt.append(((part + 0.01 + uv[0] * 0.98) / 3, uv[1]))
                cards.f += [(ids[0], ids[1], ids[2]), (ids[0], ids[2], ids[3])]

    for tr in trees:
        x, y, z, h, r, kind = tr["x"], tr["y"], tr["z"] - 0.4, tr["h"], tr["r"], tr.get("kind", "broadleaf")
        cr = crowns[kind]
        near = math.hypot(x, y) < 900
        count = 5 if near else 3
        phase = rng.uniform(0, 2 * math.pi)
        trunk_h = h * (0.73 if kind == "eucalyptus" else 0.66 if kind == "pine" else 0.49)
        branch(trunk, (x, y, z), (x, y, z + trunk_h), max(0.10, r * 0.085), 0.055)
        for i in range(count):
            angle = phase + 2 * math.pi * i / count
            # Adult maritime-pine-like crowns are irregular and rounded rather
            # than the old identical stacked Christmas-tree cones.
            if kind == "pine":
                cx, cy = math.cos(angle) * r * 0.32, math.sin(angle) * r * 0.32
                cz = h * (0.70 + 0.12 * (i % 2))
                rx, rz = r * (0.48 if near else 0.57), h * 0.16
            elif kind == "eucalyptus":
                cx, cy = math.cos(angle) * r * 0.34, math.sin(angle) * r * 0.34
                cz = h * (0.56 + 0.31 * i / max(1, count - 1))
                rx, rz = r * 0.36, h * 0.16
            else:
                cx, cy = math.cos(angle) * r * 0.30, math.sin(angle) * r * 0.30
                cz = h * (0.61 + 0.15 * (i % 2))
                rx, rz = r * 0.53, h * 0.22
            if near:
                branch(trunk, (x, y, z + trunk_h * 0.72),
                       (x + cx, y + cy, z + cz), max(0.035, r * 0.04), 0.018, k=4)
            if near:
                leaf_cards(x, y, z, cx, cy, cz, r, h, kind, phase)
            else:
                foliage(cr, x, y, z, cx, cy, cz, rx, rz, r, h)
        if near:  # small opaque interior, with perforated foliage silhouette
            foliage(cr, x, y, z, 0, 0, h * 0.73, r * 0.40, h * 0.16, r, h)
        # Conservative hexagonal collider remains within the planner's radius.
        branch(col, (x, y, z), (x, y, z + h + 0.5), r, r, k=6, uv=False)
    def write(o, name, uv=False):
        path = os.path.join(out, name)
        if o.f:
            o.write(path, uv=uv)
        else:
            with open(path, "w", encoding="utf-8") as fh:
                fh.write("# no trees of this type\n")

    write(trunk, "trees_trunks.obj", uv=True)
    for k, o in crowns.items():
        write(o, f"trees_crowns_{k}.obj", uv=True)
    write(col, "trees_collision.obj")
    write(cards, "trees_cards.obj", uv=True)
    _tree_textures(out)


def build_canopy(t: Terrain, feats, blds: list[dict], sat, out: str,
                 *, res: float = 10.0, half_m: float = MAP_HALF_M,
                 refine_half_m: float = 1500.0) -> dict | None:
    """Representative terrain-following closed canopy over the satellite forest mask.

    Return conservative per-cell top heights for the coordinator's obstacle raster:
    {res, x0, y0, mask, top}. This surface is a visual stand-in for dense, unresolved
    stems, not an assertion about observed tree heights. Cells touching buildings,
    roads or water are omitted. Its upper mesh must also be bound as a collider.
    No shell is invented without the approved satellite source.
    """
    from scipy import ndimage
    import json

    if res <= 0 or half_m <= 0 or not math.isclose(2 * half_m / res, round(2 * half_m / res)):
        raise ValueError("canopy extent must contain an integer number of positive cells")
    path = os.path.join(out, "trees_canopy.obj")
    if sat is None:
        with open(path, "w", encoding="utf-8") as fh:
            fh.write("# canopy unavailable without satellite imagery\n")
        with open(os.path.join(out, "trees_canopy.json"), "w", encoding="utf-8") as fh:
            json.dump({"cells": 0, "label": "No satellite canopy"}, fh)
        return None
    n = int(round(2 * half_m / res))
    centers = np.arange(n) * res - half_m + res / 2
    E, N = np.meshgrid(centers, centers)
    rgb = sat.rgb_map(E, N)
    value = rgb.mean(-1)
    exg = 2 * rgb[..., 1] - rgb[..., 0] - rgb[..., 2]
    score = np.clip((56 - value) / 16, 0, 1) * np.clip((exg - 5) / 8, 0, 1)
    mask = score > 0.38

    # Use sub-cell exclusions at 2 m so even a road crossing only a cell corner
    # cuts a gap through the canopy. Explicit margin also protects mission starts.
    px = 2.0
    nr = int(math.ceil(2 * half_m / px))
    ex = Image.new("L", (nr, nr), 0)
    draw = ImageDraw.Draw(ex)
    def P(x, y):
        return ((x + half_m) / px, (half_m - y) / px)
    for b in blds:
        poly = [P(*p) for p in b["poly"]]
        draw.polygon(poly, fill=255)
        draw.line(poly + poly[:1], fill=255, width=6)
    for kind, line in feats.lines:
        draw.line([P(*p) for p in line], fill=255, width=7 if kind == "road" else 9)
    for kind, poly in feats.areas:
        if kind == "water":
            draw.polygon([P(*p) for p in poly], fill=255)
    ex = np.asarray(ex)[::-1] > 0
    if res % px == 0 and nr == n * int(res / px):
        k = int(res / px)
        excluded = ex.reshape(n, k, n, k).any(axis=(1, 3))
    else:
        # Custom small proof grids can use a different resolution; nearest-cell
        # sampling plus dilation is intentionally conservative at that boundary.
        idx = np.clip(((centers + half_m) / px).astype(int), 0, nr - 1)
        excluded = ndimage.maximum_filter(ex.astype(np.uint8), size=max(3, int(res / px) + 2))[idx[:, None], idx]
    mask &= ~excluded
    # Isolated one-cell specks are still represented by individual trees; retain
    # connected forest patches and their actual irregular mask boundaries.
    labels, _ = ndimage.label(mask)
    sizes = np.bincount(labels.ravel())
    mask &= sizes[labels] >= 4
    # Resolve the active 3 km core at 5 m while retaining a 10 m outer shell.
    # One global half-cell grid makes normals and coordinates identical at joins.
    step = res / 2
    fine = np.repeat(np.repeat(mask, 2, axis=0), 2, axis=1)
    distance = ndimage.distance_transform_edt(fine) * step
    depth = np.zeros((2 * n + 1, 2 * n + 1), float)
    occupancy = np.zeros_like(depth)
    for dy, dx in ((0, 0), (0, 1), (1, 0), (1, 1)):
        depth[dy:dy + 2 * n, dx:dx + 2 * n] = np.maximum(depth[dy:dy + 2 * n, dx:dx + 2 * n], distance * fine)
        occupancy[dy:dy + 2 * n, dx:dx + 2 * n] += fine
    depth[occupancy < 4] = 0
    g = np.arange(2 * n + 1) * step - half_m
    VE, VN = np.meshgrid(g, g)
    ground = t.z_map(VE, VN)

    def crown_field(e, north):
        # Rotated, incommensurate wavelengths avoid repeated cell-centred peaks.
        # Smooth forms are representative aggregate crowns, not measured heights.
        return (10.5 + 1.65 * np.sin(e * 0.119 + north * 0.043 + 1.9)
                + 1.35 * np.sin(-e * 0.073 + north * 0.183 + 4.1)
                + 0.65 * np.sin(e * 0.271 - north * 0.157 + 2.7))

    blend = np.clip(depth / 12.0, 0, 1)
    blend = blend * blend * (3 - 2 * blend)
    h = 1.2 + blend * (crown_field(VE, VN) - 1.2)
    Z = ground + h
    dn, de = np.gradient(Z, step)
    normals = np.stack((-de, -dn, np.ones_like(de)), axis=-1)
    normals /= np.linalg.norm(normals, axis=-1, keepdims=True)
    refine = mask & (np.abs(E) <= refine_half_m) & (np.abs(N) <= refine_half_m)
    mesh = Obj()
    smooth_normals = []
    shared = {}

    def vertex(row, col):
        key = (row, col)
        if key not in shared:
            shared[key] = mesh.add_v(VE[row, col], VN[row, col], Z[row, col])
            mesh.vt.append((col / (2 * n), row / (2 * n)))
            smooth_normals.append(tuple(normals[row, col]))
        return shared[key]

    def triangles(row, col, stride):
        a, b = vertex(row, col), vertex(row, col + stride)
        c, d = vertex(row + stride, col), vertex(row + stride, col + stride)
        mesh.f.extend(((a, b, d), (a, d, c)))

    def fine_neighbor(row, col):
        return 0 <= row < n and 0 <= col < n and refine[row, col]

    for row, col in zip(*np.nonzero(mask)):
        rr, cc = 2 * row, 2 * col
        if refine[row, col]:
            for dy, dx in ((0, 0), (0, 1), (1, 0), (1, 1)):
                triangles(rr + dy, cc + dx, 1)
        elif any(fine_neighbor(row + dy, col + dx) for dy, dx in ((-1, 0), (0, 1), (1, 0), (0, -1))):
            # Coarse cells bordering fine cells share the exact edge midpoint;
            # the transition fan avoids cracks and interpolated-height seams.
            ring = [(rr, cc)]
            if fine_neighbor(row - 1, col): ring.append((rr, cc + 1))
            ring.append((rr, cc + 2))
            if fine_neighbor(row, col + 1): ring.append((rr + 1, cc + 2))
            ring.append((rr + 2, cc + 2))
            if fine_neighbor(row + 1, col): ring.append((rr + 2, cc + 1))
            ring.append((rr + 2, cc))
            if fine_neighbor(row, col - 1): ring.append((rr + 1, cc))
            center = vertex(rr + 1, cc + 1)
            ids = [vertex(*q) for q in ring]
            mesh.f.extend((ids[i], ids[(i + 1) % len(ids)], center) for i in range(len(ids)))
        else:
            triangles(rr, cc, 2)
        for adjacent, one, two in (((row - 1, col), (rr, cc + 2), (rr, cc)),
                                   ((row, col + 1), (rr + 2, cc + 2), (rr, cc + 2)),
                                   ((row + 1, col), (rr + 2, cc), (rr + 2, cc + 2)),
                                   ((row, col - 1), (rr, cc), (rr + 2, cc))):
            ar, ac = adjacent
            if 0 <= ar < n and 0 <= ac < n and mask[ar, ac]:
                continue
            segments = [(one, two)]
            if refine[row, col]:
                middle = ((one[0] + two[0]) // 2, (one[1] + two[1]) // 2)
                segments = [(one, middle), (middle, two)]
            for begin, end in segments:
                wall = []
                delta = np.array([VE[end] - VE[begin], VN[end] - VN[begin]])
                outward = np.array([-delta[1], delta[0], 0.0])
                outward /= max(np.linalg.norm(outward), 1e-9)
                for pos, lower in ((begin, False), (end, False), (end, True), (begin, True)):
                    vr, vc = pos
                    wall.append(mesh.add_v(VE[pos], VN[pos], ground[pos] if lower else Z[pos]))
                    mesh.vt.append((vc / (2 * n), vr / (2 * n)))
                    smooth_normals.append(tuple(outward))
                mesh.f.extend(((wall[0], wall[1], wall[2]), (wall[0], wall[2], wall[3])))
    # Use the coordinator's explicit-normal extension of the shared writer.
    # Fine/coarse joins therefore retain the identical derivative normal field.
    if mesh.f:
        mesh.write(path, uv=True, normals=np.asarray(smooth_normals))
    else:
        with open(path, "w", encoding="utf-8") as fh:
            fh.write("# no dense satellite canopy cells\n")

    # Same approved satellite colour, now broken up without cell-aligned mounds.
    tex = np.clip(rgb[::-1] * 1.35 + [10, 17, 6], 0, 255).astype(np.uint8)
    tex_px = 4096 if half_m > 500 else 1024
    image = np.asarray(Image.fromarray(tex).resize((tex_px, tex_px), Image.Resampling.BILINEAR)).copy()
    rng = np.random.default_rng(TREE_SEED + 21)
    xx = np.linspace(-half_m, half_m, tex_px)[None, :]
    for rows in np.array_split(np.arange(tex_px), 16):
        yy = half_m - rows[:, None] / (tex_px - 1) * 2 * half_m
        variation = (0.96 + 0.07 * np.sin(xx * 0.43 + yy * 0.19)
                     + 0.05 * np.sin(-xx * 0.23 + yy * 0.58))
        grain = rng.normal(0, 3, (len(rows), tex_px, 1))
        image[rows] = np.clip(image[rows] * variation[..., None] + grain, 0, 255).astype(np.uint8)
    Image.fromarray(image).save(os.path.join(out, "trees_canopy.jpg"), quality=92)
    normal_px = 2048 if half_m > 500 else 512
    nx = np.linspace(-half_m, half_m, normal_px)[None, :]
    ny = np.linspace(half_m, -half_m, normal_px)[:, None]
    # Tangent-space micro-normal detail only; does not change physical heights.
    du = 0.12 * np.cos(nx * 0.71 + ny * 0.23) + 0.07 * np.cos(nx * 0.29 - ny * 0.89)
    dv = 0.10 * np.cos(nx * 0.37 - ny * 0.67) - 0.08 * np.cos(nx * 0.83 + ny * 0.41)
    nz = np.ones_like(du)
    micro = np.stack((-du, -dv, nz), axis=-1)
    micro /= np.linalg.norm(micro, axis=-1, keepdims=True)
    Image.fromarray(np.round((micro + 1) * 127.5).astype(np.uint8)).save(
        os.path.join(out, "trees_canopy_normal.png"), compress_level=3)
    # All nine fine-grid vertices belong to the source 10 m cell envelope. This
    # conservative bound includes every actually emitted top and skirt vertex.
    top = np.maximum.reduce([Z[dy:dy + 2 * n:2, dx:dx + 2 * n:2]
                             for dy in (0, 1, 2) for dx in (0, 1, 2)])
    # OBJ coordinates round to centimetres. Round the bound upward as well and
    # retain a centimetre guard against the final float32 representation.
    top = (np.ceil(top * 100) / 100 + 0.01).astype(np.float32)
    with open(os.path.join(out, "trees_canopy.json"), "w", encoding="utf-8") as fh:
        json.dump({"cells": int(mask.sum()), "cell_m": res, "fine_step_m": step,
                   "fine_cells": int(refine.sum()), "vertices": len(mesh.v), "triangles": len(mesh.f),
                   "normals": "continuous height derivative field, shared fine/coarse positions",
                   "label": "REPRESENTATIVE continuous canopy from 2016 satellite mask; estimated heights",
                   "height_m_range": [float(h[occupancy > 0].min()) if mask.any() else None,
                                      float(h[occupancy > 0].max()) if mask.any() else None]}, fh)
    return {"res": res, "x0": float(centers[0]), "y0": float(centers[0]), "mask": mask, "top": top}
