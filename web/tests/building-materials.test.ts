import { describe, expect, it } from "vitest";
import { MATERIAL_IMAGE_SIZE, MATERIAL_REGIONS, MATERIAL_ZOOMS, MATERIALS, materialImageId, materialPixelRatio } from "../lib/building-material-data";
import { CLOSE_BUILDING_ZOOM, MAX_TERRAIN_ZOOM } from "../lib/terrain-data";

describe("intended architectural repeat targets and atlas boundaries", () => {
  it("records consistent repeat targets for the native preview (not its rendered scale)", () => {
    for (const zoom of MATERIAL_ZOOMS) {
      const repeatMetres = MATERIAL_IMAGE_SIZE / materialPixelRatio("stucco", zoom) / 2 ** (zoom - 16);
      expect(repeatMetres).toBeCloseTo(6);
      const roofRepeat = MATERIAL_IMAGE_SIZE / materialPixelRatio("terracotta", zoom) / 2 ** (zoom - 16);
      expect(roofRepeat).toBeCloseTo(2);
    }
  });
  it("keeps wall and roof crops separate and within the generated image", () => {
    for (const material of MATERIALS) {
      const [x, y, width, height] = MATERIAL_REGIONS[material];
      expect(x).toBeGreaterThanOrEqual(0); expect(y).toBeGreaterThanOrEqual(0);
      expect(x + width).toBeLessThanOrEqual(1254); expect(y + height).toBeLessThanOrEqual(1254);
    }
    expect(MATERIAL_REGIONS.stucco[1] + MATERIAL_REGIONS.stucco[3]).toBe(MATERIAL_REGIONS.terracotta[1]);
    const ids = MATERIALS.flatMap((material) => MATERIAL_ZOOMS.map((zoom) => materialImageId(material, zoom)));
    expect(new Set(ids).size).toBe(ids.length);
  });
  it("allows close inspection beyond the old zoom-17 limit", () => {
    expect(CLOSE_BUILDING_ZOOM).toBeGreaterThan(17);
    expect(MAX_TERRAIN_ZOOM).toBeGreaterThan(CLOSE_BUILDING_ZOOM);
  });
});
