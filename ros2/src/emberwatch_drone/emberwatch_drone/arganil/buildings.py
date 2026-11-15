"""OSM footprints with representative Portuguese plaster / tiled-roof appearance.

No individual facade or roof survey is available in the cached extract. All inferred
forms are labelled; the original total building-height envelope is preserved, and
Gazebo uses these same meshes for visuals and collision.
"""
from __future__ import annotations

import math
import os

import numpy as np
from PIL import Image, ImageDraw

from .geodata import Building, Terrain
from .mesh import Obj, ear_clip


def _face(obj: Obj, verts, uvs) -> None:
    """Unshared vertices give crisp roof creases and valid per-vertex OBJ UVs."""
    ids = [obj.add_v(*v) for v in verts]
    obj.vt.extend(uvs)
    for k in range(1, len(ids) - 1):
        obj.f.append((ids[0], ids[k], ids[k + 1]))


# Representative traditional town palette, selected per building, never a survey.
PALETTE = [(239, 237, 226), (226, 220, 201), (235, 228, 204),
           (214, 193, 143), (236, 233, 222), (221, 215, 201),
           (218, 202, 165), (232, 225, 212)]


class _Atlas:
    """Pack one unique 96px facade/roof image per face with a mip-safe gutter.

    No repeated facade grid: windows and doors are drawn in physical coordinates
    for that face and its building's mapped number of storeys.
    """
    def __init__(self):
        self.images, self.refs = [], []

    def face(self, obj, verts, image, uvs=None):
        slot = len(self.images)
        self.images.append(image)
        if uvs is None:
            uvs = [(0, 0), (1, 0), (1, 1), (0, 1)]
        start = len(obj.vt)
        _face(obj, verts, uvs)
        self.refs.extend((obj, start + i, slot, u, v) for i, (u, v) in enumerate(uvs))

    def write(self, path):
        if not self.images:
            return
        size, gutter = 100, 2
        cols = max(1, math.ceil(math.sqrt(len(self.images))))
        rows = math.ceil(len(self.images) / cols)
        if max(cols * size, rows * size) > 8192:
            raise ValueError('building material atlas exceeds the 8192px texture limit')
        atlas = Image.new('RGB', (cols * size, rows * size))
        for i, image in enumerate(self.images):
            x, y = i % cols * size, i // cols * size
            # Extend edge pixels into a two-pixel gutter to avoid neighbouring
            # building colours leaking through mipmaps at oblique angles.
            padded = Image.new('RGB', (size, size))
            padded.paste(image, (gutter, gutter))
            padded.paste(image.crop((0, 0, 1, 96)).resize((2, 96)), (0, 2))
            padded.paste(image.crop((95, 0, 96, 96)).resize((2, 96)), (98, 2))
            padded.paste(image.crop((0, 0, 96, 1)).resize((96, 2)), (2, 0))
            padded.paste(image.crop((0, 95, 96, 96)).resize((96, 2)), (2, 98))
            for px, py, sx, sy in ((0, 0, 0, 0), (98, 0, 95, 0), (0, 98, 0, 95), (98, 98, 95, 95)):
                padded.paste(Image.new('RGB', (2, 2), image.getpixel((sx, sy))), (px, py))
            atlas.paste(padded, (x, y))
        for obj, idx, slot, u, v in self.refs:
            obj.vt[idx] = (((slot % cols) * size + gutter + .5 + u * 95) / atlas.width,
                          1 - ((slot // cols) * size + gutter + .5 + (1 - v) * 95) / atlas.height)
        # Large procedural atlases should not spend minutes in PNG filter optimization.
        # Compression level changes preserve decoded pixels and UV alignment.
        atlas.save(path, optimize=False, compress_level=3)


def _weather(rgb, seed, width_m, height_m, roof=False):
    """Original procedural plaster/clay albedo; no external pixels (project MIT)."""
    rng = np.random.default_rng(seed)
    coarse = Image.fromarray(rng.integers(95, 160, (12, 12), dtype=np.uint8)).resize((96, 96), Image.Resampling.BILINEAR)
    weather = (np.asarray(coarse, dtype=float) - 127) / 255
    yy, xx = np.mgrid[:96, :96]
    if roof:
        # Real metre-based tile repeats, independently toned/weathered per roof.
        u, v = xx / 95 * width_m, (95 - yy) / 95 * height_m
        col = np.mod(u + (np.floor(v / .32) % 2) * .115, .23) / .23
        light = .80 + .20 * np.cos((col - .36) * math.pi)
        joints = (np.mod(v, .32) < .023) | (col < .06)
        light[joints] *= .66
        shade = light + weather * .42
    else:
        # Rain streaks and low-level damp: moderate value change, no yellow wash.
        shade = 1 + weather * .26 - .13 * np.clip((yy / 95 - .72) / .28, 0, 1)
    rgb = np.clip(np.asarray(rgb)[None, None, :] * shade[..., None], 0, 255).astype(np.uint8)
    return Image.fromarray(rgb, 'RGB')


def _storeys(b):
    # geodata uses max(1, building:levels) * 3 + 1 m. Invalid mapped 0 values
    # already resolve to a conservative one-storey estimate, not zero height.
    if b.height_basis == 'osm:building:levels':
        return max(1, round((b.height_m - 1) / 3))
    return max(1, round(b.height_m / 3))


def _street_side(poly, roads):
    """Nearest outward-facing facade to a mapped road, or longest-wall fallback."""
    lengths = [math.dist(a, b) for a, b in zip(poly, poly[1:] + poly[:1])]
    if not roads:
        return int(np.argmax(lengths)), 'representative longest-wall fallback (no roads supplied)'
    segments = [(np.asarray(a, float), np.asarray(b, float)) for line in roads
                for a, b in zip(line, line[1:]) if math.dist(a, b) > .01]
    if not segments:
        return int(np.argmax(lengths)), 'representative longest-wall fallback (no road segments)'
    r0 = np.asarray([a for a, _ in segments])
    delta = np.asarray([b - a for a, b in segments])
    norm2 = np.sum(delta * delta, axis=1)
    best = (math.inf, 0)
    for i, (a, b) in enumerate(zip(poly, poly[1:] + poly[:1])):
        a, b = np.asarray(a, float), np.asarray(b, float)
        midpoint = (a + b) / 2
        outward = np.array([b[1] - a[1], a[0] - b[0]])
        fraction = np.clip(np.sum((midpoint - r0) * delta, axis=1) / norm2, 0, 1)
        difference = r0 + delta * fraction[:, None] - midpoint
        scores = np.linalg.norm(difference, axis=1) + 25 * (difference @ outward < 0)
        best = min(best, (float(scores.min()), i))
    return best[1], 'representative door on nearest mapped-road-facing facade'


def _facade_image(t, b, poly, side, base, eave, mean_ground, palette, door_side):
    p0, p1 = np.asarray(poly[side], float), np.asarray(poly[(side + 1) % len(poly)], float)
    width, height = float(np.linalg.norm(p1 - p0)), eave - base
    seed = (int(b.osm_id) * 16807 + side * 104729) % (2 ** 32)
    rng = np.random.default_rng(seed)
    image = _weather(palette, seed, width, height)
    draw = ImageDraw.Draw(image)

    def P(x, z):
        return round(x / max(width, .1) * 95), round((eave - z) / max(height, .1) * 95)

    def ground(x):
        xy = p0 + (p1 - p0) * (x / max(width, .1))
        return float(t.z_map(xy[0], xy[1]))

    door = None
    if side == door_side and width > 1.9:
        x = width * float(rng.uniform(.40, .60))
        z0 = max(base + .05, ground(x) + .06)
        dh, dw = min(float(rng.uniform(1.85, 2.10)), eave - z0 - .12), float(rng.uniform(.80, 1.05))
        if dh > 1.35:
            door = (x - dw / 2, z0, x + dw / 2, z0 + dh)
            left, top = P(door[0], door[3]); right, bottom = P(door[2], door[1])
            draw.rectangle((left - 1, top - 1, right + 1, bottom + 1), fill=(192, 181, 157))
            draw.rectangle((left, top, right, bottom), fill=(77, 62, 46))
            draw.line((left + 1, top + 2, right - 1, top + 2), fill=(114, 97, 71))
            draw.point((right - 1, (top + bottom) // 2), fill=(211, 190, 131))
    count = 0
    storeys = _storeys(b)
    spacing = float(rng.uniform(3.4, 4.7))
    columns = max(1, int(width / spacing)) if width > 2 else 0
    if width < 7:
        columns = min(columns, 1)
    for level in range(storeys):
        # Fit precisely the mapped/estimated storey count above mean ground,
        # rather than repeat a texture every 3m from a sunken foundation.
        available = max(2.3, eave - mean_ground)
        fh = available / storeys
        for col in range(columns):
            x = width * (col + .5) / columns + float(rng.uniform(-.18, .18))
            ww, wh = float(rng.uniform(.65, 1.03)), float(rng.uniform(1.02, 1.38))
            z = mean_ground + fh * (level + .52) + float(rng.uniform(-.10, .10))
            if z + wh / 2 > eave - .20 or z - wh / 2 < ground(x) + .32:
                continue
            if door and x + ww / 2 > door[0] - .20 and x - ww / 2 < door[2] + .20 and z - wh / 2 < door[3] + .10:
                continue
            # Some blank wall space and differing bays replace the perfect grid.
            if columns > 2 and rng.random() < .13:
                continue
            left, top = P(x - ww / 2, z + wh / 2); right, bottom = P(x + ww / 2, z - wh / 2)
            draw.rectangle((left - 1, top - 1, right + 1, bottom + 1), fill=(209, 201, 178))
            draw.rectangle((left, top, right, bottom), fill=(42, 60, 65))
            draw.line((left, top, right, top), fill=(116, 129, 126))
            draw.line(((left + right) // 2, top, (left + right) // 2, bottom), fill=(198, 195, 174))
            if right - left > 4 and rng.random() < .60:
                shutter = [(58, 76, 62), (71, 85, 91), (89, 72, 55)][int(rng.integers(3))]
                draw.rectangle((left - 3, top, left - 2, bottom), fill=shutter)
                draw.rectangle((right + 2, top, right + 3, bottom), fill=shutter)
            count += 1
    return image, count, door is not None


def _simple_roof(poly, top, height, osm_id):
    """Roof faces and gable faces, without extending outside an OSM footprint.

    Quadrilaterals receive hipped or gabled roofs. Convex other footprints use
    pyramidal roofs; concave / near-degenerate footprints retain a flat roof.
    """
    p = np.asarray(poly, dtype=float)
    edges = np.roll(p, -1, axis=0) - p
    lengths = np.linalg.norm(edges, axis=1)
    cross = np.cross(edges, np.roll(edges, -1, axis=0))
    convex = bool(np.all(cross > 0.01))
    area = abs(float(np.sum(p[:, 0] * np.roll(p[:, 1], -1) - np.roll(p[:, 0], -1) * p[:, 1]))) / 2
    if not convex or min(lengths) < 0.6 or area > 650 or height > 15:
        return 'flat', top, [], []
    rise = min(1.8, height * 0.24, math.sqrt(area) * 0.18)
    eave = top - rise
    ground = [(float(x), float(y), eave) for x, y in p]
    roofs, gables = [], []
    if len(p) == 4:
        # Rotate so p0 -> p1 is the long side; then ridge follows that side.
        j = int(np.argmax(lengths))
        p = np.roll(p, -j, axis=0)
        ground = [(float(x), float(y), eave) for x, y in p]
        left, right = (p[0] + p[3]) / 2, (p[1] + p[2]) / 2
        delta = right - left
        span = float(np.linalg.norm(delta))
        if span < 0.5:
            return 'flat', top, [], []
        # A gable on one quarter of houses adds variety without claiming a survey.
        gabled = int(osm_id) % 4 == 0
        if not gabled:
            inset = min(span * 0.38, (lengths[(j + 1) % 4] + lengths[(j + 3) % 4]) / 4)
            left, right = left + delta / span * inset, right - delta / span * inset
        a, b = (*left, top), (*right, top)
        roofs = [[ground[0], ground[1], b, a], [ground[2], ground[3], a, b]]
        ends = [[ground[1], ground[2], b], [ground[3], ground[0], a]]
        if gabled:
            gables = ends
        else:
            roofs.extend(ends)
        return 'gabled' if gabled else 'hipped', eave, roofs, gables
    peak = (*p.mean(axis=0), top)
    roofs = [[ground[i], ground[(i + 1) % len(p)], peak] for i in range(len(p))]
    return 'pyramidal', eave, roofs, []


def build_buildings(t: Terrain, blds: list[Building], out: str, roads=None) -> list[dict]:
    """Unique facades and roof tones, preserving the existing planner envelope."""
    walls, roofs, recs = Obj(), Obj(), []
    wall_atlas, roof_atlas = _Atlas(), _Atlas()
    for b in blds:
        poly = b.poly
        if len(poly) < 3:
            continue
        gz = t.z_map(np.array([p[0] for p in poly]), np.array([p[1] for p in poly]))
        mean_ground = float(gz.mean())
        base = float(gz.min()) - 1.0
        top = mean_ground + b.height_m
        shape, eave, roof_faces, gables = _simple_roof(poly, top, b.height_m, b.osm_id)
        rng = np.random.default_rng(int(b.osm_id) % (2 ** 32))
        palette_index = int(rng.integers(len(PALETTE)))
        palette = PALETTE[palette_index]
        door_side, door_basis = _street_side(poly, roads)
        windows, doors = 0, 0
        for i, ((x0, y0), (x1, y1)) in enumerate(zip(poly, poly[1:] + poly[:1])):
            image, nw, nd = _facade_image(t, b, poly, i, base, eave, mean_ground, palette, door_side)
            windows += nw
            doors += int(nd)
            wall_atlas.face(walls, [(x0, y0, base), (x1, y1, base), (x1, y1, eave), (x0, y0, eave)], image)
        for face in gables:
            wall_atlas.face(walls, face, _weather(palette, int(b.osm_id), 4, 2), [(0, 0), (1, 0), (.5, 1)])
        if shape == 'flat':
            roof_faces = [[(poly[k][0], poly[k][1], top) for k in (a, bb, c)] for a, bb, c in ear_clip(poly)]
        roof_tone = float(rng.uniform(.84, 1.13))
        clay = tuple(float(v) * roof_tone for v in [(179, 96, 62), (157, 86, 60), (185, 109, 73)][int(rng.integers(3))])
        for face_i, face in enumerate(roof_faces):
            edge = np.asarray(face[1]) - np.asarray(face[0])
            axis = edge / max(np.linalg.norm(edge), 1e-9)
            normal = np.cross(edge, np.asarray(face[2]) - np.asarray(face[0]))
            normal /= max(np.linalg.norm(normal), 1e-9)
            across = np.cross(normal, axis)
            uv = np.asarray([(float(np.dot(np.asarray(v) - face[0], axis)),
                              float(np.dot(np.asarray(v) - face[0], across))) for v in face])
            minimum, extent = uv.min(axis=0), np.maximum(np.ptp(uv, axis=0), .01)
            normalized = (uv - minimum) / extent
            image = _weather(clay, (int(b.osm_id) + face_i) % (2 ** 32), *extent, roof=True)
            roof_atlas.face(roofs, face, image, normalized)
        recs.append({'osm_id': b.osm_id, 'poly': [[round(x, 2), round(y, 2)] for x, y in poly],
                     'base_z': round(base, 2), 'top_z': round(top, 2), 'height_m': b.height_m,
                     'height_basis': b.height_basis, 'eave_z': round(eave, 2), 'roof_shape': shape,
                     'storeys_visual': _storeys(b), 'wall_palette_rgb': palette, 'roof_tone': round(roof_tone, 3),
                     'windows_visual': windows, 'doors_visual': doors, 'door_side': door_side,
                     'door_basis': door_basis,
                     'roof_basis': 'representative procedural form (roof tags absent in cached OSM)',
                     'facade_basis': 'representative varied plaster/windows/door; not surveyed'})
    wall_atlas.write(os.path.join(out, 'buildings_plaster.png'))
    roof_atlas.write(os.path.join(out, 'buildings_tiles.png'))
    walls.write(os.path.join(out, 'buildings_walls.obj'), uv=True)
    roofs.write(os.path.join(out, 'buildings_roofs.obj'), uv=True)
    return recs
