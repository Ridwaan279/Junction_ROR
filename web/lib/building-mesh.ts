import earcut from "earcut";
import type { Polygon, Position } from "geojson";
import { materialRepeatMetres, type BuildingMaterial } from "./building-material-data";

export const EARTH_CIRCUMFERENCE = 2 * Math.PI * 6371008.8;
export type Point2 = [number, number];
export type MeshBuffers = Record<BuildingMaterial, number[]>;
export function mercator([longitude, latitude]: Position): Point2 {
  return [(180 + longitude) / 360, (180 - 180 / Math.PI * Math.log(Math.tan(Math.PI / 4 + latitude * Math.PI / 360))) / 360];
}
export function metreScale(latitude: number) { return 1 / (EARTH_CIRCUMFERENCE * Math.cos(latitude * Math.PI / 180)); }
export function emptyMesh(): MeshBuffers { return { stucco: [], azulejo: [], granite: [], terracotta: [] }; }

export function polygonCenter(polygon: Polygon): Point2 {
  const ring = polygon.coordinates[0];
  let west = Infinity, east = -Infinity, south = Infinity, north = -Infinity;
  for (const [x, y] of ring) { west = Math.min(west, x); east = Math.max(east, x); south = Math.min(south, y); north = Math.max(north, y); }
  return [(west + east) / 2, (south + north) / 2];
}

export function appendBuildingMesh(buffers: MeshBuffers, polygon: Polygon, origin: Point2, scale: number, groundM: number, baseM: number, heightM: number, facade: BuildingMaterial) {
  const rings = polygon.coordinates.map((ring) => {
    const points = ring.length > 1 && ring[0][0] === ring[ring.length - 1][0] && ring[0][1] === ring[ring.length - 1][1] ? ring.slice(0, -1) : ring;
    return points.map((point): Point2 => { const p = mercator(point); return [(p[0] - origin[0]) / scale, (p[1] - origin[1]) / scale]; });
  }).filter((ring) => ring.length >= 3);
  if (!rings.length) return;
  const bottom = groundM + baseM, top = groundM + heightM;
  const push = (buffer: number[], point: Point2, z: number, u: number, v: number, normal: number[]) => buffer.push(point[0] * scale, point[1] * scale, z * scale, u, v, ...normal);
  const flat: number[] = [], holes: number[] = [];
  let count = 0;
  for (let r = 0; r < rings.length; r++) {
    const ring = rings[r];
    if (r) holes.push(count);
    for (const point of ring) { flat.push(...point); count++; }
    let area = 0;
    for (let i = 0; i < ring.length; i++) { const a = ring[i], b = ring[(i + 1) % ring.length]; area += a[0] * b[1] - b[0] * a[1]; }
    const sign = (area >= 0 ? 1 : -1) * (r ? -1 : 1);
    let distance = 0;
    const repeat = materialRepeatMetres(facade);
    for (let i = 0; i < ring.length; i++) {
      const a = ring[i], b = ring[(i + 1) % ring.length];
      const dx = b[0] - a[0], dy = b[1] - a[1], length = Math.hypot(dx, dy);
      if (length < 0.01) continue;
      const normal = [sign * dy / length, -sign * dx / length, 0];
      const u0 = distance / repeat, u1 = (distance + length) / repeat;
      const v0 = baseM / repeat, v1 = heightM / repeat;
      const wall = buffers[facade];
      push(wall, a, bottom, u0, v0, normal); push(wall, b, bottom, u1, v0, normal); push(wall, a, top, u0, v1, normal);
      push(wall, a, top, u0, v1, normal); push(wall, b, bottom, u1, v0, normal); push(wall, b, top, u1, v1, normal);
      distance += length;
    }
  }
  const roof = buffers.terracotta, repeat = materialRepeatMetres("terracotta"), anchor = rings[0][0];
  for (const index of earcut(flat, holes, 2)) {
    const point: Point2 = [flat[index * 2], flat[index * 2 + 1]];
    push(roof, point, top, (point[0] - anchor[0]) / repeat, (point[1] - anchor[1]) / repeat, [0, 0, 1]);
  }
}
