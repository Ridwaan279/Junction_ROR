import { describe, expect, it } from "vitest";
import type { StyleSpecification } from "maplibre-gl";
import { validateStyleMin } from "@maplibre/maplibre-gl-style-spec";
import { GROUND_ZOOMS, groundImageId, groundPixelRatio, groundTextureLayers } from "../lib/ground-material-data";
import { GROUND_MATERIALS } from "../lib/building-material-data";

describe("mapped road and park materials", () => {
  it("preserves mapped boundaries, filters, road widths, labels and original styles", () => {
    const style: StyleSpecification = { version: 8, sources: { osm: { type: "vector", url: "https://example.invalid/tiles.json" } }, layers: [
      { id: "park", type: "fill", source: "osm", "source-layer": "park", filter: ["==", ["get", "class"], "public_park"], paint: { "fill-color": "green" } },
      { id: "road_minor", type: "line", source: "osm", "source-layer": "transportation", filter: ["==", ["get", "class"], "minor"], paint: { "line-width": 8, "line-color": "white" } },
      { id: "road_rail", type: "line", source: "osm", "source-layer": "transportation", paint: { "line-width": 2 } },
      { id: "road_minor_casing", type: "line", source: "osm", "source-layer": "transportation", paint: { "line-width": 10 } },
      { id: "labels", type: "symbol", source: "osm", "source-layer": "transportation_name" },
    ] };
    const original = structuredClone(style), overlays = groundTextureLayers(style);
    expect(overlays).toHaveLength(2);
    const road = overlays.find((o) => o.group === "roads")!.layer;
    expect(road.type).toBe("line");
    if (road.type === "line") expect(road.paint?.["line-width"]).toBe(8);
    expect(road).toMatchObject({ filter: ["==", ["get", "class"], "minor"] });
    expect(overlays[0].layer).toMatchObject({ filter: ["==", ["get", "class"], "public_park"] });
    expect(overlays[0].before).toBe("road_minor");
    expect(style).toEqual(original);
    // Undefined paint values are rejected by MapLibre rather than omitted.
    expect(JSON.parse(JSON.stringify(overlays))).toEqual(overlays);
    expect(validateStyleMin({ ...style, layers: [...style.layers, ...overlays.map((o) => o.layer)] })).toEqual([]);
    expect(groundTextureLayers({ ...style, layers: overlays.map((o) => o.layer) })).toEqual([]);
  });
  it("keeps native ground-pattern repeat targets consistent between integer zooms", () => {
    for (const material of GROUND_MATERIALS) for (const zoom of GROUND_ZOOMS.slice(1)) expect(groundPixelRatio(material, zoom)).toBeCloseTo(groundPixelRatio(material, zoom - 1) / 2);
    const ids = GROUND_MATERIALS.flatMap((material) => GROUND_ZOOMS.map((zoom) => groundImageId(material, zoom)));
    expect(new Set(ids).size).toBe(ids.length);
  });
});
