import type { Map as LibreMap } from "maplibre-gl";
import { BUILDING_LAYER, BUILDING_SOURCE_URL, FOOTPRINT_LAYER, buildingBase, buildingHeight } from "./building-data";

export function addBuildingLayers(map: LibreMap) {
  if (map.getLayer(BUILDING_LAYER)) return;
  const style = map.getStyle();
  const source = Object.entries(style.sources).find(([, value]) => value.type === "vector" && value.url === BUILDING_SOURCE_URL)?.[0] ?? "portugal-building-data";
  if (!map.getSource(source)) map.addSource(source, { type: "vector", url: BUILDING_SOURCE_URL, bounds: [-9.65, 36.85, -6.05, 42.25], attribution: '<a href="https://openfreemap.org/">OpenFreeMap</a> © <a href="https://openmaptiles.org/">OpenMapTiles</a> © <a href="https://www.openstreetmap.org/copyright">OpenStreetMap contributors</a>' });
  // Replace the style's building layers so visibility and height handling agree.
  for (const layer of style.layers) {
    if ("source-layer" in layer && layer["source-layer"] === "building") map.setLayoutProperty(layer.id, "visibility", "none");
  }
  const beforeLabels = style.layers.find((layer) => layer.type === "symbol")?.id;
  map.addLayer({ id: FOOTPRINT_LAYER, type: "fill", source, "source-layer": "building", minzoom: 13,
    paint: { "fill-color": "#d4c4b3", "fill-opacity": 0.7, "fill-outline-color": "#a99885" },
  }, beforeLabels);
  map.addLayer({ id: BUILDING_LAYER, type: "fill-extrusion", source, "source-layer": "building", minzoom: 14,
    filter: ["!=", ["get", "hide_3d"], true], layout: { visibility: "none" },
    paint: { "fill-extrusion-height": buildingHeight, "fill-extrusion-base": buildingBase,
      "fill-extrusion-color": ["to-color", ["get", "colour"], "#e3d8c7"],
      "fill-extrusion-opacity": 1, "fill-extrusion-opacity-transition": { duration: 0, delay: 0 }, "fill-extrusion-vertical-gradient": true,
    },
  }, beforeLabels);
  map.setLight({ anchor: "viewport", color: "#fff4df", intensity: 0.45, position: [1.4, 200, 35] });
}

export function setBuildingsVisible(map: LibreMap, visible: boolean) {
  if (map.getLayer(BUILDING_LAYER)) map.setLayoutProperty(BUILDING_LAYER, "visibility", visible ? "visible" : "none");
  if (map.getLayer(FOOTPRINT_LAYER)) map.setPaintProperty(FOOTPRINT_LAYER, "fill-opacity", visible ? 0.25 : 0.7);
}
