import { describe, expect, it, vi } from "vitest";
import type { Map as LibreMap } from "maplibre-gl";
import { BUILDING_LAYER } from "../lib/building-data";
import { BuildingMaterialLayer } from "../lib/building-materials";

vi.mock("../lib/material-images", () => ({ loadMaterialImages: async () => ({}) }));

describe("textured building source coverage", () => {
  it("retains the native 3D source consumer without drawing or picking its opaque roof", async () => {
    let visibility = "visible";
    const paint = new Map<string, unknown>([["fill-extrusion-opacity", 1], ["fill-opacity", 0.25]]);
    const handlers = new Map<string, () => void>();
    let elevation = 0;
    const features = [{ properties: { render_height: 12 }, geometry: { type: "Polygon", coordinates: [[[-9.139, 38.722], [-9.1389, 38.722], [-9.1389, 38.7221], [-9.139, 38.7221], [-9.139, 38.722]]] } }];
    const map = {
      getStyle: () => ({ layers: [{ id: BUILDING_LAYER, source: "osm" }] }),
      getLayer: () => true, getZoom: () => 18, getCenter: () => ({ lng: -9.139, lat: 38.722 }), getTerrain: () => null,
      queryTerrainElevation: () => elevation, isMoving: () => false,
      // Terrain-only coverage cannot answer this query after the 3D consumer is hidden.
      querySourceFeatures: () => visibility === "visible" ? features : [],
      getLayoutProperty: () => visibility,
      setLayoutProperty: (_id: string, _key: string, value: string) => { visibility = value; },
      getPaintProperty: (_id: string, key: string) => paint.get(key),
      setPaintProperty: (_id: string, key: string, value: unknown) => { paint.set(key, value); },
      triggerRepaint: vi.fn(), on: (event: string, handler: () => void) => { handlers.set(event, handler); }, off: vi.fn(),
    } as unknown as LibreMap;
    const layer = new BuildingMaterialLayer(vi.fn());
    layer.onAdd(map); layer.setDisplay(true, true);
    await layer.load(new AbortController().signal);
    expect(visibility).toBe("visible");
    expect(paint.get("fill-extrusion-opacity")).toBe(0);
    handlers.get("moveend")?.();
    expect(paint.get("fill-extrusion-opacity")).toBe(0);
    const priorRepaints = vi.mocked(map.triggerRepaint).mock.calls.length;
    // The same source revision can gain queryable DEM coverage after its event.
    elevation = 54; handlers.get("idle")?.();
    expect(vi.mocked(map.triggerRepaint).mock.calls.length).toBeGreaterThan(priorRepaints);
    layer.setDisplay(true, false);
    expect(paint.get("fill-extrusion-opacity")).toBe(1);
    expect(visibility).toBe("visible");
    layer.onRemove(map, {} as WebGL2RenderingContext);
    expect(map.off).toHaveBeenCalledWith("render", expect.any(Function));
  });
});
