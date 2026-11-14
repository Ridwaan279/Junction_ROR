import type { ExpressionSpecification } from "maplibre-gl";

export const BUILDING_SOURCE_URL = "https://tiles.openfreemap.org/planet";
export const BUILDING_FALLBACK_HEIGHT_M = 5;
export const BUILDING_LAYER = "portugal-buildings-3d";
export const FOOTPRINT_LAYER = "portugal-building-footprints";

// OpenMapTiles render_height is derived, not a guarantee of measured height.
export const buildingHeight: ExpressionSpecification = ["case", [">", ["to-number", ["get", "render_height"], 0], 0], ["to-number", ["get", "render_height"]], BUILDING_FALLBACK_HEIGHT_M];
export const buildingBase: ExpressionSpecification = ["min", buildingHeight, ["max", 0, ["to-number", ["get", "render_min_height"], 0]]];

export type BuildingInfo = {
  name: string | null;
  renderHeightM: number;
  providerHeightM: number | null;
  baseHeightM: number;
  measuredHeightM: null;
  heightBasis: "provider-derived" | "display-default";
  sourceUrl: string;
};

export function buildingInfo(properties: Record<string, unknown>, longitude: number, latitude: number): BuildingInfo {
  const number = (value: unknown) => typeof value === "number" && Number.isFinite(value) ? value : typeof value === "string" && value.trim() && Number.isFinite(Number(value)) ? Number(value) : null;
  const suppliedHeight = number(properties.render_height);
  const providerHeightM = suppliedHeight !== null && suppliedHeight > 0 ? suppliedHeight : null;
  const renderHeightM = providerHeightM ?? BUILDING_FALLBACK_HEIGHT_M;
  const suppliedBase = number(properties.render_min_height);
  return {
    name: typeof properties.name === "string" && properties.name.trim() ? properties.name : null,
    renderHeightM, providerHeightM, baseHeightM: Math.min(renderHeightM, Math.max(0, suppliedBase ?? 0)),
    measuredHeightM: null, heightBasis: providerHeightM === null ? "display-default" : "provider-derived",
    sourceUrl: `https://www.openstreetmap.org/#map=19/${latitude.toFixed(6)}/${longitude.toFixed(6)}`,
  };
}
