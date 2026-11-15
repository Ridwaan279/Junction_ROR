"""Generate the Arganil Gazebo world (terrain, OSM buildings, trees) + planner obstacle raster.

    ros2 run emberwatch_drone gen_arganil_world --data ~/px4/geodata --out ros2/generated/arganil

Outputs (generated; derived from ODbL / CC BY data -> kept out of Git, regenerate instead):
  terrain.obj + terrain.png   textured terrain mesh (visual AND collision, same mesh)
  buildings_walls/roofs.obj   extruded OSM footprints, flat roofs (visual AND collision)
  trees_visual.obj            low-poly trunks + crowns
  trees_collision.obj         one 8-sided prism per tree enclosing trunk+crown (simple shape)
  obstacles.npz               5 m raster: ground (max), obstacle top, obstacle kind
  features.json               buildings / trees with provenance + attribution + approximations
  emberwatch_arganil.sdf      PX4-compatible world (systems = PX4 server.config + ParticleEmitter)
"""

from __future__ import annotations

import math
import os
import numpy as np
from matplotlib.path import Path as MplPath
from PIL import Image, ImageDraw
from .geodata import (DEFAULT_BUILDING_H_M, DEFAULT_LEVEL_H_M, MAP_HALF_M, MAP_ORIGIN, Building, Satellite, Terrain, load_osm)

import argparse
import json
from .mesh import Obj, ear_clip
from .terrain import TERRAIN_STEP_M, TEX_PX, build_terrain, satellite_texture, paint_texture, build_terrain_chunks
from .geodata import DgtOrtho
from .buildings import build_buildings
from .vegetation import TREE_SEED, MAX_TREES, place_trees, build_tree_meshes, build_canopy
from .world_sdf import PX4_SYSTEMS, mesh_model, mat, write_world
RASTER_RES_M = 5.0

def build_raster(t: Terrain, blds: list[dict], trees: list[dict], out: str, mesh_g=None, mesh_z=None, canopy=None) -> None:
    res = RASTER_RES_M
    g = np.arange(-MAP_HALF_M + res / 2, MAP_HALF_M, res)
    E, N = np.meshgrid(g, g)
    # conservative ground: max of cell centre and 4 corners
    zs = [t.z_map(E + dx, N + dy) for dx, dy in [(0, 0), (-res / 2, -res / 2), (res / 2, -res / 2),
                                                   (-res / 2, res / 2), (res / 2, res / 2)]]
    ground = np.max(zs, axis=0).astype(np.float32)
    top = ground.copy()
    kind = np.zeros(ground.shape, np.uint8)
    n = len(g)

    def cells(x0, y0, x1, y1):
        c0, c1 = int((x0 + MAP_HALF_M) // res), int((x1 + MAP_HALF_M) // res) + 1
        r0, r1 = int((y0 + MAP_HALF_M) // res), int((y1 + MAP_HALF_M) // res) + 1
        return max(0, r0), min(n, r1), max(0, c0), min(n, c1)

    for b in blds:
        p = np.array(b["poly"])
        r0, r1, c0, c1 = cells(*p.min(0) - res, *p.max(0) + res)
        if r0 >= r1 or c0 >= c1:
            continue
        pts = np.stack([E[r0:r1, c0:c1].ravel(), N[r0:r1, c0:c1].ravel()], 1)
        inside = MplPath(p).contains_points(pts, radius=res * 0.75).reshape(r1 - r0, c1 - c0)  # touch => occupied
        sub = top[r0:r1, c0:c1]
        upd = inside & (b["top_z"] > sub)
        sub[upd] = b["top_z"]
        kind[r0:r1, c0:c1][upd] = 1
    for tr in trees:
        r0, r1, c0, c1 = cells(tr["x"] - tr["r"] - res, tr["y"] - tr["r"] - res, tr["x"] + tr["r"] + res, tr["y"] + tr["r"] + res)
        sl = (slice(r0, r1), slice(c0, c1))
        d = np.hypot(E[sl] - tr["x"], N[sl] - tr["y"]) <= tr["r"] + res * 0.71
        tz = tr["z"] + tr["h"] + 0.5
        upd = d & (tz > top[sl])
        top[sl][upd] = tz
        kind[sl][upd] = 2
    if canopy is not None:
        # Source masks encode full10m cells; include touching5m target cells conservatively.
        for dy in (-res/2,0,res/2):
            for dx in (-res/2,0,res/2):
                cc=np.floor((E+dx-canopy['x0']+canopy['res']/2)/canopy['res']).astype(int)
                rr=np.floor((N+dy-canopy['y0']+canopy['res']/2)/canopy['res']).astype(int)
                valid=(rr>=0)&(cc>=0)&(rr<canopy['mask'].shape[0])&(cc<canopy['mask'].shape[1])
                rr=np.clip(rr,0,canopy['mask'].shape[0]-1);cc=np.clip(cc,0,canopy['mask'].shape[1]-1)
                update=valid & canopy['mask'][rr,cc] & (canopy['top'][rr,cc]>top)
                top[update]=canopy['top'][rr,cc][update];kind[update]=2
    np.savez_compressed(os.path.join(out, "obstacles.npz"), res=res, x0=g[0], y0=g[0], ground=ground, top=top, kind=kind,
                        mesh_g=mesh_g, mesh_z=mesh_z.astype(np.float32))  # exact terrain-mesh grid (spawn heights)


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--data", default=os.path.expanduser("~/px4/geodata"))
    ap.add_argument("--out", required=True)
    ap.add_argument("--no-satellite", action="store_true", help="fallback: OSM-painted texture, scattered trees")
    a = ap.parse_args(argv)
    out = os.path.abspath(a.out)
    os.makedirs(out, exist_ok=True)
    t = Terrain(os.path.join(a.data, "tiles"))
    feats = load_osm(os.path.join(a.data, "osm_arganil.json"))
    sat = None
    if os.path.isdir(os.path.join(a.data, "s2")) and not a.no_satellite:
        sat = Satellite(os.path.join(a.data, "s2"))
    tg, tZ = build_terrain(t, out)
    if sat is not None:
        satellite_texture(sat, out)
    else:  # simpler fallback: painted from OSM landuse
        paint_texture(t, feats, out)
    ortho_dir = os.path.join(a.data, "dgt2018")
    ortho = DgtOrtho(ortho_dir) if os.path.isfile(os.path.join(ortho_dir, "metadata.json")) and not a.no_satellite else None
    if sat is not None:
        build_terrain_chunks(sat, ortho, out, tg, tZ, core_step_m=10.0)
    blds = build_buildings(t, feats.buildings, out, roads=[line for kind, line in feats.lines if kind == "road"])
    trees = place_trees(t, feats, blds, sat)
    build_tree_meshes(trees, out)
    canopy = build_canopy(t, feats, blds, sat, out)
    build_raster(t, blds, trees, out, tg, tZ, canopy=canopy)
    write_world(out, t.origin_elev, chunked=sat is not None)
    meta = {
        "map_origin_lat_lon": MAP_ORIGIN, "map_half_m": MAP_HALF_M, "origin_elevation_m": round(t.origin_elev, 2),
        "terrain": {"source": "Mapterhorn terrain tiles (Terrarium, z%d)" % t.z, "url": "https://tiles.mapterhorn.com/",
                    "underlying": "DGT LiDAR DEM of mainland Portugal (CC BY 4.0) / Copernicus GLO-30 per Mapterhorn attribution.json",
                    "mesh_step_m": TERRAIN_STEP_M},
        "osm": {"license": "ODbL 1.0", "attribution": "(c) OpenStreetMap contributors", "osm_base": feats.osm_base},
        "approximations": {
            "building_height": f"osm height if present; else building:levels*{DEFAULT_LEVEL_H_M}+1 m; else {DEFAULT_BUILDING_H_M} m estimate",
            "building_base": "min terrain under footprint - 1 m; top = mean terrain + height; representative roof forms within height envelope",
            "trees": "OSM natural=tree nodes (height estimated) + REPRESENTATIVE trees placed where the Sentinel-2 image "
                     f"shows dark-green canopy (seed {TREE_SEED}); species mix/heights/positions are not surveyed",
            "terrain_texture": "DGT2018 CC BY4.0 core 0.488m/px if cached; neutral EOX2016 outside; 100m seam blend; fallback OSM",
            "satellite": {"source": "EOxCloudless https://cloudless.eox.at by EOX IT Services GmbH (Contains modified "
                                    "Copernicus Sentinel data 2016)", "license": "CC BY 4.0", "layer": "s2cloudless_3857",
                          "used": sat is not None},
            "osm_completeness": "OSM building coverage around Arganil is incomplete; missing buildings are not modelled"},
        "counts": {"buildings": len(blds), "trees": len(trees),
                   "trees_mapped": sum(1 for tr in trees if tr["basis"].startswith("osm")),
                   "building_height_basis": {k: sum(1 for b in blds if b["height_basis"] == k)
                                             for k in {b["height_basis"] for b in blds}}},
        "buildings": blds, "trees": trees,
    }
    if ortho is not None:
        meta["imagery_core"]={"source":"DGT Ortofotos2018", "license":"CC BY4.0", "endpoint":ortho.meta["endpoint"],
                              "layer":ortho.meta["layer"], "attribution":ortho.meta["attribution"],
                              "extent_enu_m":[-1500,-1500,1500,1500], "chunk_size_m":500, "texture_gsd_m":500/1024,
                              "note":"100m seam blends to EOX2016; no exposure lift; historic2018 imagery"}
    if canopy is not None:
        meta["counts"]["canopy_cells"]=int(canopy["mask"].sum())
        meta["canopy"]={"mask_source":"EOX2016", "height_basis":"representative terrain-following canopy, not surveyed",
                        "planner":"upper vertex maxima, kind2; visual and collider identical"}
    json.dump(meta, open(os.path.join(out, "features.json"), "w"))
    print(json.dumps({k: meta[k] for k in ("origin_elevation_m", "counts")}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
