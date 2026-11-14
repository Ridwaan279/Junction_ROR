import { describe, expect, it } from "vitest";
import type { StyleSpecification } from "maplibre-gl";
import { parseCoordinates, placeCamera, PLACES, prepareTerrainStyle, searchPlaces } from "../lib/terrain-data";

describe("terrain explorer navigation", () => {
  it("finds Portuguese places without requiring accents or network geocoding", () => {
    expect(searchPlaces("  geres ")).toEqual([PLACES[1]]);
    expect(searchPlaces("MONCHIQUE")[0].id).toBe("monchique");
    expect(searchPlaces("Alentejo")[0].id).toBe("marvao");
    expect(searchPlaces("")).toHaveLength(PLACES.length);
    expect(searchPlaces("not a saved view")).toEqual([]);
  });
  it("interprets latitude,longitude and rejects swapped or out-of-region values", () => {
    expect(parseCoordinates("40.322, -7.612")).toEqual([-7.612, 40.322]);
    expect(parseCoordinates("-7.612,40.322")).toBeNull();
    expect(parseCoordinates("91, -8")).toBeNull();
    expect(parseCoordinates("NaN, -8")).toBeNull();
    expect(parseCoordinates("40.3,-7.6,12")).toBeNull();
  });
  it("uses actual pitched cameras for 3D and a north-up flat view for 2D", () => {
    expect(placeCamera(PLACES[0], true)).toMatchObject({ pitch: 62, bearing: -28, center: PLACES[0].center });
    expect(placeCamera(PLACES[0], false)).toMatchObject({ pitch: 0, bearing: 0, center: PLACES[0].center });
    for (const place of PLACES) expect(parseCoordinates(place.center[1] + "," + place.center[0])).toEqual(place.center);
  });
});

describe("terrain basemap styling", () => {
  it("keeps source URLs, attribution and layer order while changing the terrain palette", () => {
    const original: StyleSpecification = { version: 8, sources: { test: { type: "vector", url: "https://example.invalid/tiles.json", attribution: "Original attribution" } },
      layers: [{ id: "background", type: "background", paint: { "background-color": "#000" } },
        { id: "water", type: "fill", source: "test", "source-layer": "water", paint: { "fill-color": "#00f" } }] };
    const styled = prepareTerrainStyle(original);
    expect(styled.sources).toEqual(original.sources);
    expect(styled.layers.map((layer) => layer.id)).toEqual(["background", "water"]);
    expect(original.layers[0].paint).toEqual({ "background-color": "#000" });
    expect(styled.layers[1].paint).toEqual({ "fill-color": "#b8d5e3" });
  });
});
