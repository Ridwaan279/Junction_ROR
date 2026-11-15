export const MATERIAL_ATLAS_URL = "/textures/portugal/material-atlas.webp";
export const MATERIAL_IMAGE_SIZE = 512;
export const FACADE_MATERIALS = ["stucco", "azulejo", "granite", "ochre", "greenTile", "rose", "whiteBlue"] as const;
export const ROOF_MATERIALS = ["terracotta", "clayOrange", "clayRed", "clayPale", "slate", "gravel"] as const;
export const MATERIALS = [...FACADE_MATERIALS, ...ROOF_MATERIALS] as const;
export const GROUND_MATERIALS = ["asphalt", "calcada", "grass", "dryGrass"] as const;
export type BuildingMaterial = typeof MATERIALS[number];
export type FacadeMaterial = typeof FACADE_MATERIALS[number];
export type RoofMaterial = typeof ROOF_MATERIALS[number];
export type GroundMaterial = typeof GROUND_MATERIALS[number];
export type TextureMaterial = BuildingMaterial | GroundMaterial;
export type Tint = [number, number, number];
export type BuildingAppearance = { facade: FacadeMaterial; roof: RoofMaterial; profile: "flat" | "gabled" | "hipped"; tint: Tint; roofTint: Tint; rotation: number; phase: number };

// Generated atlas boundaries; each region becomes a repeating power-of-two texture.
export const MATERIAL_REGIONS: Record<TextureMaterial, [number, number, number, number]> = {
  stucco: [0, 0, 627, 655], azulejo: [627, 0, 627, 655],
  terracotta: [0, 655, 627, 599], granite: [627, 655, 627, 599],
  ochre: [0, 0, 627, 627], greenTile: [627, 0, 627, 627], rose: [0, 627, 627, 627], whiteBlue: [627, 627, 627, 627],
  clayOrange: [0, 0, 418, 418], clayRed: [418, 0, 418, 418], clayPale: [836, 0, 418, 418],
  slate: [0, 418, 418, 418], gravel: [418, 418, 418, 418], asphalt: [836, 418, 418, 418],
  calcada: [0, 836, 418, 418], grass: [418, 836, 418, 418], dryGrass: [836, 836, 418, 418],
};

export function materialAtlasUrl(material: TextureMaterial) {
  if (["stucco", "azulejo", "terracotta", "granite"].includes(material)) return MATERIAL_ATLAS_URL;
  return `/textures/portugal/${["ochre", "greenTile", "rose", "whiteBlue"].includes(material) ? "facade" : "roof-ground"}-variants.webp`;
}
export function materialRepeatMetres(material: TextureMaterial) {
  if (material === "grass") return 8;
  if (material === "dryGrass") return 12;
  return material === "granite" || !(FACADE_MATERIALS as readonly string[]).includes(material) ? 2 : 6;
}

/** Stable decorative assignments, independent of camera, tile IDs and load order. */
export function buildingAppearance([longitude, latitude]: [number, number]): BuildingAppearance {
  let seed = (Math.imul(Math.round(longitude * 1e6), 73856093) ^ Math.imul(Math.round(latitude * 1e6), 19349663)) >>> 0;
  const random = () => { seed = (Math.imul(seed ^ (seed >>> 16), 2246822507) + 3266489909) >>> 0; return seed / 4294967296; };
  const facade = FACADE_MATERIALS[Math.floor(random() * FACADE_MATERIALS.length)];
  const roof = ROOF_MATERIALS[Math.floor(random() * ROOF_MATERIALS.length)];
  const profile = roof === "gravel" ? "flat" : (["gabled", "hipped", "hipped", "flat"] as const)[Math.floor(random() * 4)];
  const warmth = random(), brightness = 0.87 + random() * 0.2;
  const roofLight = 0.76 + random() * 0.3;
  return { facade, roof, profile, tint: [brightness, brightness * (0.95 + warmth * 0.05), brightness * (0.91 + warmth * 0.09)],
    roofTint: [roofLight, roofLight * (0.92 + random() * 0.1), roofLight * (0.9 + random() * 0.1)], rotation: random() < 0.5 ? 0 : Math.PI / 2, phase: random() * 4 };
}
