import type { CameraOptions, StyleSpecification } from "maplibre-gl";

export const PORTUGAL_BOUNDS: [[number, number], [number, number]] = [[-9.65, 36.85], [-6.05, 42.25]];
export const TERRAIN_ATTRIBUTION = 'Terrain © <a href="https://mapterhorn.com/attribution/" target="_blank" rel="noreferrer">Mapterhorn & data providers</a>';
export const BASE_STYLE_URL = "https://tiles.openfreemap.org/styles/liberty";
export const DEM_URL = "https://tiles.mapterhorn.com/{z}/{x}/{y}.webp";

export type TerrainPlace = {
  id: string;
  name: string;
  region: string;
  kind: "mountain" | "river" | "city" | "coast";
  description: string;
  center: [number, number];
  zoom: number;
  bearing: number;
};

// Approximate camera presets, not surveyed coordinates or navigation guidance.
export const PLACES: TerrainPlace[] = [
  { id: "estrela", name: "Serra da Estrela", region: "Centro", kind: "mountain", description: "Granite ridges, glacial valleys and the high country of central Portugal.", center: [-7.612, 40.322], zoom: 11.5, bearing: -28 },
  { id: "geres", name: "Peneda-Gerês", region: "Norte", kind: "mountain", description: "Follow the folds of the northern mountains and their forested valleys.", center: [-8.143, 41.735], zoom: 11.4, bearing: -24 },
  { id: "douro", name: "Vale do Douro", region: "Norte", kind: "river", description: "Trace the Douro through the hills around Pinhão.", center: [-7.545, 41.191], zoom: 12.1, bearing: 35 },
  { id: "sintra", name: "Serra de Sintra", region: "Lisboa", kind: "coast", description: "The wooded ridge between the Atlantic coast and the plains of Lisbon.", center: [-9.39, 38.783], zoom: 12.7, bearing: -35 },
  { id: "marvao", name: "Marvão", region: "Alentejo", kind: "mountain", description: "Explore the granite escarpment above the Alentejo landscape.", center: [-7.376, 39.393], zoom: 12.5, bearing: 25 },
  { id: "monchique", name: "Serra de Monchique", region: "Algarve", kind: "mountain", description: "Discover the Algarve's inland ridges and their Atlantic-facing slopes.", center: [-8.594, 37.317], zoom: 11.7, bearing: -25 },
  { id: "porto", name: "Porto", region: "Norte", kind: "city", description: "Explore the riverfront, streets and contours of the Douro estuary.", center: [-8.611, 41.149], zoom: 13.4, bearing: -15 },
  { id: "lisboa", name: "Lisboa", region: "Lisboa", kind: "city", description: "The hills and neighbourhoods along the Tejo estuary.", center: [-9.139, 38.722], zoom: 13.1, bearing: -20 },
];

export function searchPlaces(query: string): TerrainPlace[] {
  const normalize = (text: string) => text.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase().trim();
  const needle = normalize(query);
  return PLACES.filter((place) => normalize(place.name + " " + place.region).includes(needle));
}

export function parseCoordinates(query: string): [number, number] | null {
  const match = query.trim().match(/^(-?\d+(?:\.\d+)?)\s*,\s*(-?\d+(?:\.\d+)?)$/);
  if (!match) return null;
  const latitude = Number(match[1]), longitude = Number(match[2]);
  const [[west, south], [east, north]] = PORTUGAL_BOUNDS;
  return longitude >= west && longitude <= east && latitude >= south && latitude <= north ? [longitude, latitude] : null;
}

export function placeCamera(place: TerrainPlace, threeD: boolean): CameraOptions {
  return { center: place.center, zoom: place.zoom, pitch: threeD ? 62 : 0, bearing: threeD ? place.bearing : 0 };
}

export function prepareTerrainStyle(original: StyleSpecification): StyleSpecification {
  const style = structuredClone(original);
  for (const layer of style.layers) {
    if (layer.type === "background") layer.paint = { ...layer.paint, "background-color": "#f3f1e8" };
    if (layer.type === "fill" && layer.id === "water") layer.paint = { ...layer.paint, "fill-color": "#b8d5e3" };
    if (layer.type === "fill" && layer.id === "park") layer.paint = { ...layer.paint, "fill-color": "#c5dbb0", "fill-opacity": 0.8 };
    if (layer.type === "fill" && layer.id === "landcover_wood") layer.paint = { ...layer.paint, "fill-color": "#b4cea0", "fill-opacity": 0.62 };
    if (layer.type === "fill" && layer.id === "landcover_grass") layer.paint = { ...layer.paint, "fill-color": "#d7e0b9", "fill-opacity": 0.55 };
    if (layer.type === "fill" && layer.id === "landuse_residential") layer.paint = { ...layer.paint, "fill-color": "#ebe8de", "fill-opacity": 0.6 };
    if (layer.type === "line" && layer.id.startsWith("waterway")) layer.paint = { ...layer.paint, "line-color": "#9ac4d9" };
  }
  return style;
}
