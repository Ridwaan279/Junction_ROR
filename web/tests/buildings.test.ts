import { describe, expect, it } from "vitest";
import { buildingInfo, BUILDING_FALLBACK_HEIGHT_M } from "../lib/building-data";

describe("building height provenance", () => {
  it("keeps provider-derived render heights separate from measured height", () => {
    const info = buildingInfo({ render_height: 18, render_min_height: 3 }, -9.139, 38.722);
    expect(info).toMatchObject({ renderHeightM: 18, providerHeightM: 18, measuredHeightM: null, baseHeightM: 3, heightBasis: "provider-derived" });
    expect(info.sourceUrl).toBe("https://www.openstreetmap.org/#map=19/38.722000/-9.139000");
  });
  it.each([undefined, null, "", "unknown", -10, 0, Number.NaN, Number.POSITIVE_INFINITY])("labels absent or unusable height %s as a display default", (height) => {
    expect(buildingInfo({ render_height: height }, -8, 40)).toMatchObject({ renderHeightM: BUILDING_FALLBACK_HEIGHT_M, providerHeightM: null, measuredHeightM: null, heightBasis: "display-default" });
  });
  it("accepts numeric tile strings and constrains the building base", () => {
    expect(buildingInfo({ render_height: "12", render_min_height: 40 }, -8, 40)).toMatchObject({ renderHeightM: 12, baseHeightM: 12 });
    expect(buildingInfo({ render_height: 12, render_min_height: -1 }, -8, 40).baseHeightM).toBe(0);
  });
});
