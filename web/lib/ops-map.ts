import type { Map as LibreMap, StyleSpecification, LayerSpecification } from "maplibre-gl";
import { BASE_STYLE_URL, DEM_URL } from "./terrain-data";

/** Sentinel-2 cloudless 2016 by EOX, CC BY 4.0 (later years are CC BY-NC-SA). */
export const SATELLITE_URL =
  process.env.NEXT_PUBLIC_SATELLITE_TILES ||
  "https://tiles.maps.eox.at/wmts/1.0.0/s2cloudless_3857/default/g/{z}/{y}/{x}.jpg";
export const SATELLITE_ATTRIBUTION =
  '<a href="https://s2maps.eu" target="_blank" rel="noreferrer">Sentinel-2 cloudless 2016 by EOX</a> (contains modified Copernicus Sentinel data 2016, CC BY 4.0)';
const TERRAIN_ATTRIBUTION =
  'Terrain © <a href="https://mapterhorn.com/attribution/" target="_blank" rel="noreferrer">Mapterhorn</a>';
export const DEM_SOURCE = "ops-dem";
export const TERRAIN_EXAGGERATION = 1.45;

const KEEP_LINES = new Set(["road_motorway", "road_trunk_primary", "road_secondary_tertiary", "boundary_2", "boundary_3", "waterway_river"]);
const KEEP_LABELS = /^(label_|water_name|waterway_line_label|highway-shield-non-us)/;

/** Satellite imagery on 3D terrain with thin roads and light labels: a dark operations look. */
export async function loadOpsStyle(signal?: AbortSignal): Promise<StyleSpecification> {
  const response = await fetch(BASE_STYLE_URL, { signal });
  if (!response.ok) throw new Error("basemap style unavailable");
  const base = (await response.json()) as StyleSpecification;
  if (base.version !== 8 || !Array.isArray(base.layers)) throw new Error("invalid basemap style");
  const layers: LayerSpecification[] = [
    { id: "ops-background", type: "background", paint: { "background-color": "#05080b" } },
    {
      id: "ops-satellite",
      type: "raster",
      source: "ops-satellite",
      paint: { "raster-saturation": -0.28, "raster-brightness-max": 0.78, "raster-contrast": 0.12, "raster-fade-duration": 200 },
    },
  ];
  for (const layer of base.layers) {
    if (layer.type === "line" && KEEP_LINES.has(layer.id)) {
      const road = layer.id.startsWith("road");
      layers.push({
        ...layer,
        paint: {
          "line-color": road ? "#f3dcb4" : layer.id.startsWith("water") ? "#6fa7c7" : "#ffffff",
          "line-opacity": road ? ["interpolate", ["linear"], ["zoom"], 6, 0.12, 12, 0.38] : 0.35,
          "line-width": road ? ["interpolate", ["linear"], ["zoom"], 6, 0.4, 14, 2.2] : 1,
          ...(layer.id.startsWith("boundary") ? { "line-dasharray": [3, 2] } : {}),
        },
      } as LayerSpecification);
    } else if (layer.type === "symbol" && KEEP_LABELS.test(layer.id)) {
      layers.push({
        ...layer,
        paint: {
          ...layer.paint,
          "text-color": layer.id.startsWith("water") ? "#a9d4ec" : "#f2f4f5",
          "text-halo-color": "rgba(4, 8, 12, 0.88)",
          "text-halo-width": 1.4,
          "text-halo-blur": 0.4,
        },
      } as LayerSpecification);
    }
  }
  return {
    version: 8,
    glyphs: base.glyphs,
    sprite: base.sprite,
    sources: {
      openmaptiles: base.sources.openmaptiles,
      "ops-satellite": { type: "raster", tiles: [SATELLITE_URL], tileSize: 256, maxzoom: 15, attribution: SATELLITE_ATTRIBUTION },
      [DEM_SOURCE]: { type: "raster-dem", tiles: [DEM_URL], tileSize: 512, maxzoom: 12, encoding: "terrarium", attribution: TERRAIN_ATTRIBUTION },
    },
    layers,
    terrain: { source: DEM_SOURCE, exaggeration: TERRAIN_EXAGGERATION },
    sky: {
      "sky-color": "#0b1620",
      "horizon-color": "#26333d",
      "fog-color": "#0d151c",
      "sky-horizon-blend": 0.6,
      "horizon-fog-blend": 0.55,
      "fog-ground-blend": 0.35,
    },
  };
}

export function setThreeD(map: LibreMap, threeD: boolean, animate = true) {
  map.setTerrain(threeD ? { source: DEM_SOURCE, exaggeration: TERRAIN_EXAGGERATION } : null);
  const camera = threeD
    ? { pitch: Math.max(map.getPitch(), 58), bearing: map.getBearing() || -18 }
    : { pitch: 0, bearing: 0 };
  if (animate) map.easeTo({ ...camera, duration: 900 });
  else map.jumpTo(camera);
}
