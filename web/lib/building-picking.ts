import type { BuildingInfo } from "./building-data";
import { MATERIALS, type BuildingMaterial } from "./building-material-data";
import type { MeshBuffers } from "./building-mesh";

export type PickMesh = { info: BuildingInfo; ranges: Record<BuildingMaterial, [number, number]> };

/** Pick the nearest visible roof/wall triangle, independently of non-unique vector tile IDs. */
export function pickMesh(mesh: MeshBuffers, picks: PickMesh[], matrix: Float32Array, width: number, height: number, x: number, y: number, maxDepth = 1): BuildingInfo | null {
  const project = (data: number[], i: number) => {
    const px = data[i], py = data[i + 1], pz = data[i + 2];
    const w = matrix[3] * px + matrix[7] * py + matrix[11] * pz + matrix[15];
    return [((matrix[0] * px + matrix[4] * py + matrix[8] * pz + matrix[12]) / w + 1) * width / 2,
      (1 - (matrix[1] * px + matrix[5] * py + matrix[9] * pz + matrix[13]) / w) * height / 2,
      (matrix[2] * px + matrix[6] * py + matrix[10] * pz + matrix[14]) / w, w];
  };
  let nearest = Infinity, result: BuildingInfo | null = null;
  for (const { info, ranges } of picks) for (const material of MATERIALS) {
    const [start, end] = ranges[material], data = mesh[material];
    for (let i = start; i < end; i += 24) {
      const a = project(data, i), b = project(data, i + 8), c = project(data, i + 16);
      if (a[3] <= 0 || b[3] <= 0 || c[3] <= 0) continue;
      const d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
      if (Math.abs(d) < 0.01) continue;
      const u = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / d;
      const v = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / d;
      if (u < 0 || v < 0 || u + v > 1) continue;
      const depth = u * a[2] + v * b[2] + (1 - u - v) * c[2];
      if (depth >= -1 && depth <= maxDepth && depth < nearest) { nearest = depth; result = info; }
    }
  }
  return result;
}
