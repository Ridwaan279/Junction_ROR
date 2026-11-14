import { describe, expect, it } from "vitest";
import type { Polygon } from "geojson";
import { MATERIAL_REGIONS, MATERIALS } from "../lib/building-material-data";
import { CLOSE_BUILDING_ZOOM, MAX_TERRAIN_ZOOM } from "../lib/terrain-data";
import { appendBuildingMesh, emptyMesh, mercator, metreScale, type Point2 } from "../lib/building-mesh";
import { buildingInfo } from "../lib/building-data";
import { pickMesh, type PickMesh } from "../lib/building-picking";

const location: Point2 = [-9.139, 38.722];
const origin = mercator(location), scale = metreScale(location[1]);
const coordinate = (x: number, y: number) => [(origin[0] + x * scale) * 360 - 180, Math.atan(Math.sinh(Math.PI * (1 - 2 * (origin[1] + y * scale)))) * 180 / Math.PI];
const polygon: Polygon = { type: "Polygon", coordinates: [[[0, 0], [12, 0], [12, 12], [0, 12], [0, 0]].map(([x, y]) => coordinate(x, y))] };

describe("architectural mesh geometry", () => {
  it("keeps 6 m facade repeats and 2 m roof repeats at different local camera origins", () => {
    const a = emptyMesh(), b = emptyMesh();
    appendBuildingMesh(a, polygon, origin, scale, 40, 0, 12, "stucco");
    appendBuildingMesh(b, polygon, [origin[0] + scale * 100, origin[1] - scale * 60], scale, 40, 0, 12, "stucco");
    expect(a.stucco[11] - a.stucco[3]).toBeCloseTo(2, 5);
    expect(a.stucco[20] - a.stucco[4]).toBeCloseTo(2, 5);
    for (const material of MATERIALS) for (let i = 0; i < a[material].length; i += 8) {
      expect(a[material][i + 3]).toBeCloseTo(b[material][i + 3], 5);
      expect(a[material][i + 4]).toBeCloseTo(b[material][i + 4], 5);
    }
    expect(Math.max(...a.terracotta.filter((_, i) => i % 8 === 3))).toBeCloseTo(6, 5);
    expect(a.stucco.every(Number.isFinite)).toBe(true);
  });
  it("preserves courtyard holes and roof elevation without inventing pitched geometry", () => {
    const mesh = emptyMesh();
    const courtyard: Polygon = { type: "Polygon", coordinates: [...polygon.coordinates, [[3, 3], [3, 9], [9, 9], [9, 3], [3, 3]].map(([x, y]) => coordinate(x, y))] };
    appendBuildingMesh(mesh, courtyard, origin, scale, 40, 3, 12, "azulejo");
    let area = 0;
    for (let i = 0; i < mesh.terracotta.length; i += 24) {
      const d = mesh.terracotta;
      area += Math.abs((d[i + 8] - d[i]) * (d[i + 17] - d[i + 1]) - (d[i + 16] - d[i]) * (d[i + 9] - d[i + 1])) / 2 / scale ** 2;
      expect(d[i + 2] / scale).toBeCloseTo(52);
    }
    expect(area).toBeCloseTo(144 - 36, 4);
    expect(Math.min(...mesh.azulejo.filter((_, i) => i % 8 === 2)) / scale).toBeCloseTo(43);
    expect(mesh.stucco).toEqual([]);
    // Outer and courtyard walls both exist.
    expect(mesh.azulejo.length / 8).toBe(8 * 6);
  });
  it("embeds foundations in variable ground without scaling the supplied height", () => {
    const mesh = emptyMesh();
    appendBuildingMesh(mesh, polygon, origin, scale, 44, 0, 12, "stucco", (point) => point[0] > location[0] + 0.00005 ? 44 : 40);
    const elevations = mesh.stucco.filter((_, i) => i % 8 === 2).map((z) => z / scale);
    expect(Math.min(...elevations)).toBeCloseTo(39.9);
    expect(Math.max(...elevations)).toBeCloseTo(56);
  });
  it("ignores invalid polygons without uploading NaN coordinates", () => {
    const mesh = emptyMesh();
    appendBuildingMesh(mesh, { type: "Polygon", coordinates: [[[NaN, 40], [-9, 40], [-9, 41]]] }, origin, scale, 0, 0, 5, "stucco");
    expect(mesh).toEqual(emptyMesh());
  });
});

describe("atlas boundaries and close camera", () => {
  it("keeps wall and roof crops separate and within the generated image", () => {
    for (const material of MATERIALS) {
      const [x, y, width, height] = MATERIAL_REGIONS[material];
      expect(x).toBeGreaterThanOrEqual(0); expect(y).toBeGreaterThanOrEqual(0);
      expect(x + width).toBeLessThanOrEqual(1254); expect(y + height).toBeLessThanOrEqual(1254);
    }
    expect(MATERIAL_REGIONS.stucco[1] + MATERIAL_REGIONS.stucco[3]).toBe(MATERIAL_REGIONS.terracotta[1]);
  });
  it("allows close inspection beyond the old zoom-17 limit", () => {
    expect(CLOSE_BUILDING_ZOOM).toBeGreaterThan(17);
    expect(MAX_TERRAIN_ZOOM).toBeGreaterThan(CLOSE_BUILDING_ZOOM);
  });
});

describe("textured building selection", () => {
  it("selects the front triangle and rejects misses and geometry behind the camera", () => {
    const mesh = emptyMesh();
    const info = buildingInfo({ render_height: 12 }, ...location);
    const range = (start: number, end: number): PickMesh["ranges"] => ({ stucco: [start, end], azulejo: [0, 0], granite: [0, 0], terracotta: [0, 0] });
    for (const z of [0.6, -0.4]) for (const [x, y] of [[-1, -1], [1, -1], [0, 1]]) mesh.stucco.push(x, y, z, 0, 0, 0, 0, 1);
    const front = { ...info, name: "Front polygon" };
    const picks = [{ info, ranges: range(0, 24) }, { info: front, ranges: range(24, 48) }];
    const matrix = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
    expect(pickMesh(mesh, picks, matrix, 100, 100, 50, 50)).toBe(front);
    expect(pickMesh(mesh, picks, matrix, 100, 100, 99, 1)).toBeNull();
    // A nearer terrain surface occludes both buildings.
    expect(pickMesh(mesh, picks, matrix, 100, 100, 50, 50, -0.6)).toBeNull();
    matrix[15] = -1;
    expect(pickMesh(mesh, picks, matrix, 100, 100, 50, 50)).toBeNull();
  });
});
