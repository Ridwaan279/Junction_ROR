# Arganil demo world: data sources, licences, approximations

Simulation only. This is a geographically grounded but **simplified** model of the
Arganil (Portugal) area. It is not a survey-grade reconstruction.

## Sources

| Data | Source | Licence / attribution | Use |
|---|---|---|---|
| Terrain | Mapterhorn terrain tiles, Terrarium-encoded WebP, zoom 13 (~7.3 m/px), `https://tiles.mapterhorn.com/{z}/{x}/{y}.webp` | Mainland Portugal: **Direção-Geral do Território, LiDAR Digital Elevation Models, CC BY 4.0**. Fallback: **Copernicus GLO-30** (Copernicus free and open licence). Exact per-source list: `attribution.json` (downloaded with the tiles). | Elevation mesh, 20 m grid |
| Ground texture + forest mask | **Sentinel-2 cloudless 2016** WMTS layer `s2cloudless_3857` (zoom 15), the same layer as the web app | **CC BY 4.0**, "EOxCloudless https://cloudless.eox.at by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2016)". Later years are CC BY-NC-SA and are **not** used. | Terrain texture; tree placement where the image shows dark-green canopy |
| Buildings, mapped trees, land use, roads, rivers | OpenStreetMap via the Overpass API (snapshot `osm_base` recorded in `generated/arganil/features.json`) | **© OpenStreetMap contributors, ODbL 1.0** | Footprints, heights where tagged, terrain texture, tree positions |

No Google 3D tiles or other restricted imagery are used.

Raw data is cached outside the repo (`~/px4/geodata`, from `scripts/fetch_arganil_data.sh`).
The generated world (`ros2/generated/arganil/`, ~120 MB) is derived ODbL/CC BY data. It is
git-ignored; regenerate it with `gen_arganil_world`. If it is ever distributed, keep the
attributions above (ODbL share-alike applies to derived databases).

## Frames

- **Map/world frame:** ENU metres about the map origin, 40.2180 N, 8.0545 W (Arganil centre).
  It uses PX4's azimuthal-equidistant projection (`geo.py`), so the Gazebo, PX4 and planner
  frames agree.
- **z:** DEM elevation minus the elevation at the origin (169.9 m).
- **PX4:** `PX4_HOME_LAT/LON/ALT` are set to the map origin, so Gazebo's spherical
  coordinates match. The x500 spawns at the generated start point (`PX4_GZ_MODEL_POSE`).
- **Mission frame conversion:** NED = (n₀ + y, e₀ + x, −(z − Δz)). Here (n₀, e₀) is the
  origin's offset from the EKF origin (`ref_lat/lon`), and Δz = `ref_alt` − origin elevation.

## Approximations (all estimates are labelled in `features.json`)

- **Building heights:** OSM `height` where present (none in this extract). Otherwise
  `building:levels` × 3 m + 1 m (400 buildings). Otherwise **6 m (estimated default)** (182
  buildings).
- **Building geometry:** flat roofs. Base = lowest terrain point under the footprint − 1 m;
  top = mean terrain under the footprint + height.
- **OSM coverage:** the Arganil building coverage is incomplete. Unmapped buildings do not exist
  in the world or the planner.
- **Trees:** about 18,500 in total.
  - A few are mapped OSM trees.
  - The rest are **representative**, placed (seed 20261115) where the 2016 Sentinel-2 image
    shows dark-green canopy. Placement is dense within 1.9 km of the town and sparser outside.
    Buildings, roads and water are excluded.
  - The species mix (pine / eucalyptus / broadleaf) and the heights (6–17 m) are illustrative,
    not surveyed. The imagery is ~10 m resolution and from 2016; land cover may have changed.
  - Collision shape: a 6-sided prism enclosing each tree.
- **Terrain texture:** the Sentinel-2 image resampled to 1.5 m/px and brightened. The fallback
  (`--no-satellite`) paints it from OSM land use.
- **Launch pad:** a simulated 5 m level box under the start point, so the x500 starts level on
  sloping terrain.
- **Fire spread:** 12 seeded points, downwind-biased, never inside buildings. This is illustrative
  only; the planner's hazard covers the whole possible footprint.
- **Visual vs collision:** the terrain and buildings use the **same mesh** for both. Trees use
  the enclosing prism, so collision ⊇ visual.
- **Planner obstacle raster** (5 m): terrain = maximum of the cell centre and corners;
  buildings = footprint touching a cell; trees = crown radius. It is conservative relative to
  the meshes.
