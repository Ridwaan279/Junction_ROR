import type { Map as LibreMap, StyleSpecification } from "maplibre-gl";
import { BASE_STYLE_URL, DEM_URL, PORTUGAL_BOUNDS, prepareTerrainStyle, TERRAIN_ATTRIBUTION } from "./terrain-data";
export { PLACES, PORTUGAL_BOUNDS, parseCoordinates, placeCamera, searchPlaces } from "./terrain-data";
export type { TerrainPlace } from "./terrain-data";

export async function loadMapLibre() {
  const libre = await import("maplibre-gl");
  // MapLibre 6 needs an explicit worker asset URL when bundled by Next.js.
  libre.setWorkerUrl(new URL("maplibre-gl/dist/maplibre-gl-worker.mjs", import.meta.url).toString());
  return libre;
}

export async function loadTerrainStyle(signal?: AbortSignal): Promise<StyleSpecification> {
  const url = process.env.NEXT_PUBLIC_MAP_STYLE_URL || BASE_STYLE_URL;
  const response = await fetch(url, { signal });
  if (!response.ok) throw new Error("The basemap could not be loaded");
  const style = await response.json() as StyleSpecification;
  if (style.version !== 8 || !Array.isArray(style.layers) || !style.sources) throw new Error("Invalid map style");
  return process.env.NEXT_PUBLIC_MAP_STYLE_URL ? style : prepareTerrainStyle(style);
}

/** Shared protocol instance avoids duplicate worker/caches across React remounts. */
type ContourDem = InstanceType<(typeof import("maplibre-contour"))["default"]["DemSource"]>;
let contourSource: Promise<ContourDem> | null = null;

async function getContourSource() {
  if (!contourSource) {
    contourSource = (async () => {
      const [{ default: contour }, maplibre] = await Promise.all([import("maplibre-contour"), import("maplibre-gl")]);
      const source = new contour.DemSource({ id: "portugal-dem", url: DEM_URL, encoding: "terrarium", maxzoom: 13, worker: true, cacheSize: 64, timeoutMs: 15_000 });
      source.setupMaplibre(maplibre);
      return source;
    })();
  }
  return contourSource;
}

export async function addTerrainLayers(map: LibreMap): Promise<void> {
  const dem = await getContourSource();
  if (!map.getStyle() || map.getSource("portugal-terrain")) return;
  const bounds: [number, number, number, number] = [...PORTUGAL_BOUNDS[0], ...PORTUGAL_BOUNDS[1]];
  for (const id of ["portugal-terrain", "portugal-hillshade"]) {
    map.addSource(id, { type: "raster-dem", tiles: [dem.sharedDemProtocolUrl], bounds, tileSize: 512, encoding: "terrarium", maxzoom: 13, attribution: TERRAIN_ATTRIBUTION });
  }
  const layers = map.getStyle().layers;
  const beforeRoads = layers.find((layer) => layer.id === "water")?.id ?? layers.find((layer) => layer.type === "symbol")?.id;
  const beforeLabels = layers.find((layer) => layer.type === "symbol")?.id;
  map.addLayer({ id: "portugal-relief", type: "hillshade", source: "portugal-hillshade", paint: {
    "hillshade-exaggeration": 0.32, "hillshade-shadow-color": "#596849", "hillshade-highlight-color": "#fffced", "hillshade-accent-color": "#7c8663",
  } }, beforeRoads);
  map.addSource("portugal-contours", { type: "vector", tiles: [dem.contourProtocolUrl({
    thresholds: { 9: [200, 1000], 11: [100, 500], 12: [50, 200], 14: [20, 100] },
    contourLayer: "contours", elevationKey: "ele", levelKey: "level",
  })], bounds, maxzoom: 14 });
  map.addLayer({ id: "portugal-contour-lines", type: "line", source: "portugal-contours", "source-layer": "contours", minzoom: 9, paint: {
    "line-color": "#7d805e", "line-opacity": ["interpolate", ["linear"], ["zoom"], 9, 0.12, 12, 0.35],
    "line-width": ["match", ["get", "level"], 1, 1, 0.5],
  } }, beforeLabels);
  map.addLayer({ id: "portugal-contour-labels", type: "symbol", source: "portugal-contours", "source-layer": "contours", minzoom: 11,
    filter: [">", ["get", "level"], 0], layout: { "symbol-placement": "line", "text-field": ["concat", ["number-format", ["get", "ele"], {}], " m"],
      "text-font": ["Noto Sans Regular"], "text-size": 10, "text-padding": 20 },
    paint: { "text-color": "#727452", "text-halo-color": "#f5f3e8", "text-halo-width": 1.5 },
  }, beforeLabels);
}

export function setTerrainVisibility(map: LibreMap, relief: boolean, contours: boolean) {
  for (const [id, visible] of [["portugal-relief", relief], ["portugal-contour-lines", contours], ["portugal-contour-labels", contours]] as const) {
    if (map.getLayer(id)) map.setLayoutProperty(id, "visibility", visible ? "visible" : "none");
  }
}
