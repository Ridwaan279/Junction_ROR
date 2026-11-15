"""Arganil-area geodata: Mapterhorn terrain tiles + OpenStreetMap features -> local ENU metres.

Frames
------
* Map frame = local ENU metres about the MAP ORIGIN (``MAP_ORIGIN`` lat/lon, Arganil centre),
  using the PX4 azimuthal-equidistant projection (geo.py), so PX4/Gazebo/planner agree.
* Map z = elevation (m above the DEM datum) minus the elevation at the map origin.
  Gazebo world z == map z; PX4 home/spherical-coordinate elevation = origin elevation.

Sources (see ros2/ARGANIL_DATA.md): terrain = Mapterhorn Terrarium tiles (mainland Portugal:
DGT LiDAR DEM, CC BY 4.0; fallback Copernicus GLO-30); features = OpenStreetMap (ODbL).
"""

from __future__ import annotations

import glob
import json
import math
import os
import re
from dataclasses import dataclass, field

import numpy as np
from PIL import Image

from ..geo import geodetic_to_local_ned

MAP_ORIGIN = (40.2180, -8.0545)  # lat, lon - Arganil town centre (provisional demo area)
MAP_HALF_M = 3000.0              # map is a 6 km x 6 km square about the origin
DEFAULT_LEVEL_H_M = 3.0          # m per building:levels (estimate)
DEFAULT_BUILDING_H_M = 6.0       # buildings without levels/height (estimate: ~2 storeys)
TILE_PX = 512


def to_map(lat: float, lon: float) -> tuple[float, float]:
    n, e = geodetic_to_local_ned(lat, lon, *MAP_ORIGIN)
    return e, n


class Terrain:
    """Bilinear elevation sampler over the downloaded Terrarium tiles (web-mercator pixels)."""

    def __init__(self, tile_dir: str) -> None:
        files = glob.glob(os.path.join(tile_dir, "*_*_*.webp"))
        if not files:
            raise FileNotFoundError(f"no terrain tiles in {tile_dir} (run scripts/fetch_arganil_data.sh)")
        idx = []
        for f in files:
            z, x, y = map(int, re.findall(r"(\d+)_(\d+)_(\d+)\.webp$", f)[0])
            idx.append((z, x, y, f))
        zs = {i[0] for i in idx}
        if len(zs) != 1:
            raise ValueError("mixed zoom levels")
        self.z = zs.pop()
        xs, ys = [i[1] for i in idx], [i[2] for i in idx]
        self.x0, self.y0 = min(xs), min(ys)
        nx, ny = max(xs) - self.x0 + 1, max(ys) - self.y0 + 1
        self.h = np.full((ny * TILE_PX, nx * TILE_PX), np.nan, dtype=np.float32)
        for z, x, y, f in idx:
            rgb = np.asarray(Image.open(f).convert("RGB"), dtype=np.float32)
            if rgb.shape[:2] != (TILE_PX, TILE_PX):
                raise ValueError(f"unexpected tile size {rgb.shape} in {f}")
            elev = rgb[..., 0] * 256.0 + rgb[..., 1] + rgb[..., 2] / 256.0 - 32768.0  # Terrarium
            oy, ox = (y - self.y0) * TILE_PX, (x - self.x0) * TILE_PX
            self.h[oy:oy + TILE_PX, ox:ox + TILE_PX] = elev
        if np.isnan(self.h).any():
            raise ValueError("terrain tile mosaic has gaps")
        self.origin_elev = float(self.elev_ll(*MAP_ORIGIN))

    def _px(self, lat, lon):
        n = 2 ** self.z * TILE_PX
        x = (np.asarray(lon) + 180.0) / 360.0 * n - self.x0 * TILE_PX
        y = (1 - np.arcsinh(np.tan(np.radians(lat))) / math.pi) / 2 * n - self.y0 * TILE_PX
        return x - 0.5, y - 0.5  # pixel centres

    def elev_ll(self, lat, lon):
        x, y = self._px(np.asarray(lat, dtype=np.float64), np.asarray(lon, dtype=np.float64))
        x0 = np.clip(np.floor(x).astype(int), 0, self.h.shape[1] - 2)
        y0 = np.clip(np.floor(y).astype(int), 0, self.h.shape[0] - 2)
        fx, fy = np.clip(x - x0, 0, 1), np.clip(y - y0, 0, 1)
        h = self.h
        return ((h[y0, x0] * (1 - fx) + h[y0, x0 + 1] * fx) * (1 - fy)
                + (h[y0 + 1, x0] * (1 - fx) + h[y0 + 1, x0 + 1] * fx) * fy)

    def z_map(self, e, n):
        """Map-frame z (elevation - origin elevation) at map ENU (e, n) (arrays ok)."""
        lat, lon = map_to_ll(np.asarray(e, dtype=np.float64), np.asarray(n, dtype=np.float64))
        return self.elev_ll(lat, lon) - self.origin_elev


def map_to_ll(e, n):
    """Vectorised inverse PX4 projection (map ENU -> lat, lon)."""
    R = 6_371_000.0
    rlat, rlon = math.radians(MAP_ORIGIN[0]), math.radians(MAP_ORIGIN[1])
    x, y = np.asarray(n) / R, np.asarray(e) / R
    c = np.hypot(x, y)
    with np.errstate(invalid="ignore", divide="ignore"):
        sin_c, cos_c = np.sin(c), np.cos(c)
        lat = np.where(c < 1e-12, rlat, np.arcsin(cos_c * math.sin(rlat) + np.where(c < 1e-12, 0, x * sin_c * math.cos(rlat) / c)))
        lon = np.where(c < 1e-12, rlon, rlon + np.arctan2(y * sin_c, c * math.cos(rlat) * cos_c - x * math.sin(rlat) * sin_c))
    return np.degrees(lat), np.degrees(lon)


@dataclass
class Building:
    osm_id: int
    poly: list[tuple[float, float]]  # CCW map ENU
    height_m: float
    height_basis: str  # "osm:height" | "osm:building:levels" | "estimated_default"


@dataclass
class OsmFeatures:
    buildings: list[Building] = field(default_factory=list)
    trees: list[tuple[float, float]] = field(default_factory=list)  # mapped natural=tree nodes
    areas: list[tuple[str, list[tuple[float, float]]]] = field(default_factory=list)  # (class, poly)
    lines: list[tuple[str, list[tuple[float, float]]]] = field(default_factory=list)  # roads/rivers
    osm_base: str | None = None


def _ccw(poly):
    a = sum(x0 * y1 - x1 * y0 for (x0, y0), (x1, y1) in zip(poly, poly[1:] + poly[:1]))
    return poly if a > 0 else poly[::-1]


def _height(tags: dict) -> tuple[float, str]:
    for k in ("height", "building:height"):
        v = tags.get(k)
        if v:
            try:
                return float(str(v).replace("m", "").strip()), "osm:height"
            except ValueError:
                pass
    lv = tags.get("building:levels")
    if lv:
        try:
            return max(1.0, float(lv)) * DEFAULT_LEVEL_H_M + 1.0, "osm:building:levels"
        except ValueError:
            pass
    return DEFAULT_BUILDING_H_M, "estimated_default"


AREA_CLASS = {"forest": "forest", "wood": "forest", "scrub": "scrub", "heath": "scrub",
              "residential": "residential", "farmland": "farmland", "meadow": "grass", "grass": "grass",
              "orchard": "orchard", "water": "water"}


def load_osm(path: str) -> OsmFeatures:
    d = json.load(open(path, encoding="utf-8"))
    out = OsmFeatures(osm_base=d.get("osm3s", {}).get("timestamp_osm_base"))
    for el in d["elements"]:
        t = el.get("tags", {})
        if el["type"] == "node" and t.get("natural") == "tree":
            out.trees.append(to_map(el["lat"], el["lon"]))
            continue
        geom = el.get("geometry")
        if el["type"] == "relation" and "building" in t:
            outers = [m for m in el.get("members", []) if m.get("role") == "outer" and m.get("geometry")]
            geom = outers[0]["geometry"] if outers else None
        if not geom:
            continue
        pts = [to_map(p["lat"], p["lon"]) for p in geom]
        closed = len(pts) > 3 and math.dist(pts[0], pts[-1]) < 0.01
        if "building" in t and closed:
            h, basis = _height(t)
            out.buildings.append(Building(el["id"], _ccw(pts[:-1]), h, basis))
        elif "highway" in t:
            out.lines.append(("road", pts))
        elif t.get("waterway") == "river":
            out.lines.append(("river", pts))
        elif closed:
            cls = AREA_CLASS.get(t.get("landuse") or t.get("natural") or "")
            if cls:
                out.areas.append((cls, pts[:-1]))
    return out


class Satellite:
    """Sentinel-2 cloudless 2016 (EOX, CC BY 4.0) web-mercator mosaic, sampled in the map frame."""

    def __init__(self, tile_dir: str) -> None:
        files = glob.glob(os.path.join(tile_dir, "*_*_*.jpg"))
        if not files:
            raise FileNotFoundError(f"no satellite tiles in {tile_dir}")
        idx = [tuple(map(int, re.findall(r"(\d+)_(\d+)_(\d+)\.jpg$", f)[0])) + (f,) for f in files]
        self.z = idx[0][0]
        self.x0, self.y0 = min(i[1] for i in idx), min(i[2] for i in idx)
        nx, ny = max(i[1] for i in idx) - self.x0 + 1, max(i[2] for i in idx) - self.y0 + 1
        self.img = np.zeros((ny * 256, nx * 256, 3), np.float32)
        for z, x, y, f in idx:
            self.img[(y - self.y0) * 256:(y - self.y0 + 1) * 256, (x - self.x0) * 256:(x - self.x0 + 1) * 256] = \
                np.asarray(Image.open(f).convert("RGB"), np.float32)

    def rgb_map(self, e, n):
        lat, lon = map_to_ll(np.asarray(e, np.float64), np.asarray(n, np.float64))
        N = 2 ** self.z * 256
        x = (lon + 180.0) / 360.0 * N - self.x0 * 256 - 0.5
        y = (1 - np.arcsinh(np.tan(np.radians(lat))) / math.pi) / 2 * N - self.y0 * 256 - 0.5
        h, w = self.img.shape[:2]
        x0 = np.clip(np.floor(x).astype(int), 0, w - 2)
        y0 = np.clip(np.floor(y).astype(int), 0, h - 2)
        fx, fy = np.clip(x - x0, 0, 1)[..., None], np.clip(y - y0, 0, 1)[..., None]
        I = self.img
        return ((I[y0, x0] * (1 - fx) + I[y0, x0 + 1] * fx) * (1 - fy)
                + (I[y0 + 1, x0] * (1 - fx) + I[y0 + 1, x0 + 1] * fx) * fy)


class DgtOrtho:
    """Bounded licensed 2018 WMS sample; geographic pixel centres -> map ENU."""
    def __init__(self, tile_dir: str):
        from pathlib import Path
        folder=Path(tile_dir)
        self.meta=json.loads((folder/'metadata.json').read_text())
        self.bounds=self.meta['bounds_wsen']
        self.img=np.asarray(Image.fromarray(np.concatenate([
            np.concatenate([np.asarray(Image.open(folder/f'{r}_{c}.jpg').convert('RGB')) for c in range(2)],axis=1)
            for r in range(2)],axis=0)))

    def rgb_map(self,e,n):
        lat,lon=map_to_ll(np.asarray(e,np.float64),np.asarray(n,np.float64))
        w,s,ee,nn=self.bounds;h,ww=self.img.shape[:2]
        x=(lon-w)/(ee-w)*ww-.5;y=(nn-lat)/(nn-s)*h-.5
        if np.any((x<-.5)|(x>ww-.5)|(y<-.5)|(y>h-.5)):
            raise ValueError('DGT sample outside cached geographic extent')
        x0=np.clip(np.floor(x).astype(int),0,ww-2);y0=np.clip(np.floor(y).astype(int),0,h-2)
        fx=np.clip(x-x0,0,1)[...,None];fy=np.clip(y-y0,0,1)[...,None]
        return (self.img[y0,x0]*(1-fx)+self.img[y0,x0+1]*fx)*(1-fy)+(self.img[y0+1,x0]*(1-fx)+self.img[y0+1,x0+1]*fx)*fy
