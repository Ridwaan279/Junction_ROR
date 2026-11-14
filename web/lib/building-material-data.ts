import type { ExpressionSpecification } from "maplibre-gl";

export const MATERIAL_ATLAS_URL = "/textures/portugal/material-atlas.webp";
export const MATERIAL_IMAGE_SIZE = 512;
export const MATERIALS = ["stucco", "azulejo", "terracotta", "granite"] as const;
export type BuildingMaterial = typeof MATERIALS[number];
export const BUILDING_ROOF_LAYER = "portugal-building-roofs";
export const MATERIAL_ZOOMS = [16, 17, 18, 19, 20, 21, 22] as const;

// Generated atlas boundaries; each region is resampled for a power-of-two sprite.
export const MATERIAL_REGIONS: Record<BuildingMaterial, [number, number, number, number]> = {
  stucco: [0, 0, 627, 655], azulejo: [627, 0, 627, 655],
  terracotta: [0, 655, 627, 599], granite: [627, 655, 627, 599],
};

export function materialRepeatMetres(material: BuildingMaterial) {
  return material === "terracotta" || material === "granite" ? 2 : 6;
}

// Temporary native-pattern preview. The metre-based mesh renderer will replace it;
// MapLibre's extrusion pattern projection does not preserve physical roof scale.
export function materialImageId(material: BuildingMaterial, zoom: number) { return `portugal-${material}-${zoom}`; }
export function materialPixelRatio(material: BuildingMaterial, zoom: number) {
  return MATERIAL_IMAGE_SIZE / materialRepeatMetres(material) / 2 ** (zoom - 16);
}
export function materialPattern(roof: boolean): ExpressionSpecification {
  const pattern = (zoom: number): ExpressionSpecification => roof ? ["literal", materialImageId("terracotta", zoom)] : ["match", ["%", ["to-number", ["id"], 0], 10], 0, materialImageId("azulejo", zoom), 1, materialImageId("granite", zoom), materialImageId("stucco", zoom)];
  return ["step", ["zoom"], pattern(16), ...MATERIAL_ZOOMS.slice(1).flatMap((zoom) => [zoom, pattern(zoom)])] as ExpressionSpecification;
}
