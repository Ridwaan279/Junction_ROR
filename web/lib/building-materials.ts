import type { Map as LibreMap } from "maplibre-gl";
import { BUILDING_LAYER } from "./building-data";
import { BUILDING_ROOF_LAYER, MATERIAL_ATLAS_URL, MATERIAL_IMAGE_SIZE, MATERIAL_REGIONS, MATERIAL_ZOOMS, MATERIALS, materialImageId, materialPattern, materialPixelRatio } from "./building-material-data";

export async function loadBuildingMaterials(map: LibreMap, signal: AbortSignal) {
  const response = await fetch(MATERIAL_ATLAS_URL, { signal });
  if (!response.ok) throw new Error("Building textures could not load");
  const atlas = await createImageBitmap(await response.blob());
  try {
    if (atlas.width !== 1254 || atlas.height !== 1254) throw new Error("Invalid building material atlas");
    for (const material of MATERIALS) {
      const [x, y, width, height] = MATERIAL_REGIONS[material];
      const image = await createImageBitmap(atlas, x, y, width, height, { resizeWidth: MATERIAL_IMAGE_SIZE, resizeHeight: MATERIAL_IMAGE_SIZE, resizeQuality: "high" });
      try {
        if (signal.aborted) throw new DOMException("Aborted", "AbortError");
        for (const zoom of MATERIAL_ZOOMS) {
          const id = materialImageId(material, zoom);
          if (!map.hasImage(id)) map.addImage(id, image, { pixelRatio: materialPixelRatio(material, zoom) });
        }
      } finally { image.close(); }
    }
  } finally { atlas.close(); }
}

export function setBuildingMaterials(map: LibreMap, enabled: boolean) {
  if (map.getLayer(BUILDING_LAYER)) map.setPaintProperty(BUILDING_LAYER, "fill-extrusion-pattern", enabled ? materialPattern(false) : undefined);
  if (map.getLayer(BUILDING_ROOF_LAYER)) map.setPaintProperty(BUILDING_ROOF_LAYER, "fill-extrusion-pattern", enabled ? materialPattern(true) : undefined);
}
