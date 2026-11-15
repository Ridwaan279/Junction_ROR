import type { Map as LibreMap, StyleSpecification } from "maplibre-gl";
import { loadTerrainStyle, addTerrainLayers } from "./terrain";
import { addBuildingLayers } from "./buildings";
import { BuildingMaterialLayer } from "./building-materials";
import { GroundMaterials } from "./ground-materials";

export const DEM_SOURCE = "portugal-terrain";
export const TERRAIN_EXAGGERATION = 1.3;
/** Original Portugal terrain palette and street detail, with source attribution intact. */
export async function loadOpsStyle(signal?: AbortSignal): Promise<StyleSpecification> {
  const style = await loadTerrainStyle(signal);
  return { ...style, sky: {
    "sky-color": "#c7dce7", "horizon-color": "#edf1e9", "fog-color": "#edf1e9",
    "sky-horizon-blend": 0.7, "horizon-fog-blend": 0.7, "fog-ground-blend": 0.1,
  } };
}
export async function restorePortugalLayers(map: LibreMap, signal: AbortSignal, onTextureFailure: () => void) {
  await addTerrainLayers(map);
  if (signal.aborted) return null;
  addBuildingLayers(map);
  const buildings = new BuildingMaterialLayer(onTextureFailure);
  map.addLayer(buildings, map.getStyle().layers.find((layer) => layer.type === "symbol")?.id);
  const ground = new GroundMaterials(map);
  ground.setDisplay(true, true);
  void buildings.load(signal).catch(() => { if (!signal.aborted) onTextureFailure(); });
  void ground.load(signal).catch(() => { if (!signal.aborted) onTextureFailure(); });
  return { buildings, ground };
}
export function setThreeD(map: LibreMap, threeD: boolean, animate = true) {
  if (map.getSource(DEM_SOURCE)) map.setTerrain(threeD ? { source: DEM_SOURCE, exaggeration: TERRAIN_EXAGGERATION } : null);
  const camera = threeD ? { pitch: Math.max(map.getPitch(), 58), bearing: map.getBearing() || -18 } : { pitch: 0, bearing: 0 };
  if (animate) map.easeTo({ ...camera, duration: 700 }); else map.jumpTo(camera);
}
