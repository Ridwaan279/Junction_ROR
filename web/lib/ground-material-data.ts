import type { ExpressionSpecification, LayerSpecification, StyleSpecification } from "maplibre-gl";
import { materialRepeatMetres, type GroundMaterial } from "./building-material-data";

export const GROUND_ZOOMS = [16, 17, 18, 19, 20, 21, 22] as const;
export const GROUND_SPRITE_SIZE = 128;
export const GROUND_PREFIX = "portugal-surface-";
export type SurfaceGroup = "roads" | "parks";
export function groundImageId(material: GroundMaterial, zoom: number) { return `${GROUND_PREFIX}${material}-${zoom}`; }
export function groundPixelRatio(material: GroundMaterial, zoom: number) {
  const metresPerPixel = 2 * Math.PI * 6371008.8 * Math.cos(39.5 * Math.PI / 180) / (512 * 2 ** zoom);
  return GROUND_SPRITE_SIZE * metresPerPixel / materialRepeatMetres(material);
}
export function groundPattern(material: GroundMaterial, alternate?: GroundMaterial): ExpressionSpecification {
  const at = (zoom: number): ExpressionSpecification => alternate ? ["case", ["==", ["%", ["to-number", ["id"], 1], 5], 0], groundImageId(alternate, zoom), groundImageId(material, zoom)] : ["literal", groundImageId(material, zoom)];
  return ["step", ["zoom"], at(16), ...GROUND_ZOOMS.slice(1).flatMap((zoom) => [zoom, at(zoom)])] as ExpressionSpecification;
}

/** Clone only mapped surfaces, preserving their filters and widths. No invented roads/park boundaries. */
export function groundTextureLayers(style: StyleSpecification): { layer: LayerSpecification; before?: string; group: SurfaceGroup }[] {
  const beforeRoads = style.layers.find((layer) => "source-layer" in layer && layer["source-layer"] === "transportation")?.id;
  return style.layers.flatMap<{ layer: LayerSpecification; before?: string; group: SurfaceGroup }>((original, index) => {
    if (original.id.startsWith(GROUND_PREFIX)) return [];
    const layer = structuredClone(original);
    const id = `${GROUND_PREFIX}${layer.id}`;
    if (layer.type === "fill" && ["park", "landcover_grass"].includes(layer.id)) {
      const paint = { ...layer.paint };
      delete paint["fill-outline-color"];
      return [{ group: "parks" as const, before: beforeRoads, layer: { ...layer, id, minzoom: Math.max(16, layer.minzoom ?? 0), layout: { ...layer.layout, visibility: "none" as const },
        paint: { ...paint, "fill-pattern": groundPattern("grass", "dryGrass"), "fill-opacity": ["interpolate", ["linear"], ["zoom"], 16, 0, 17, 0.72] } } as LayerSpecification }];
    }
    if (layer.type === "fill" && layer.id === "road_area_pattern") {
      return [{ group: "roads" as const, before: style.layers[index + 1]?.id, layer: { ...layer, id, minzoom: Math.max(16, layer.minzoom ?? 0), layout: { ...layer.layout, visibility: "none" as const }, paint: { ...layer.paint, "fill-pattern": groundPattern("calcada"), "fill-opacity": ["interpolate", ["linear"], ["zoom"], 16, 0, 17, 0.75] } } as LayerSpecification }];
    }
    if (layer.type !== "line" || layer["source-layer"] !== "transportation" || !layer.id.startsWith("road_") || /casing|rail|hatching|aerial|transit/.test(layer.id)) return [];
    const paint = { ...layer.paint };
    delete paint["line-dasharray"]; delete paint["line-gradient"];
    return [{ group: "roads" as const, before: style.layers[index + 1]?.id, layer: { ...layer, id, minzoom: Math.max(16, layer.minzoom ?? 0), layout: { ...layer.layout, visibility: "none" as const },
      paint: { ...paint, "line-pattern": groundPattern(layer.id.includes("path") ? "calcada" : "asphalt"), "line-opacity": ["interpolate", ["linear"], ["zoom"], 16, 0, 17, 0.66] } } as LayerSpecification }];
  });
}
