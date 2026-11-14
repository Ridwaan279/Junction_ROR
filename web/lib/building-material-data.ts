export const MATERIAL_ATLAS_URL = "/textures/portugal/material-atlas.webp";
export const MATERIAL_IMAGE_SIZE = 512;
export const MATERIALS = ["stucco", "azulejo", "terracotta", "granite"] as const;
export type BuildingMaterial = typeof MATERIALS[number];

// Generated atlas boundaries; each region becomes a repeating power-of-two texture.
export const MATERIAL_REGIONS: Record<BuildingMaterial, [number, number, number, number]> = {
  stucco: [0, 0, 627, 655], azulejo: [627, 0, 627, 655],
  terracotta: [0, 655, 627, 599], granite: [627, 655, 627, 599],
};

export function materialRepeatMetres(material: BuildingMaterial) {
  return material === "terracotta" || material === "granite" ? 2 : 6;
}
