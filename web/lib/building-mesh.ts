import earcut from "earcut";
import type { Polygon, Position } from "geojson";
import { MATERIALS, materialRepeatMetres, type BuildingMaterial, type BuildingAppearance, type FacadeMaterial, type Tint } from "./building-material-data";

export const EARTH_CIRCUMFERENCE = 2 * Math.PI * 6371008.8;
export type Point2 = [number, number];
export type MeshBuffers = Record<BuildingMaterial, number[]>;
export const MESH_STRIDE = 11;
export function mercator([longitude, latitude]: Position): Point2 {
  return [(180 + longitude) / 360, (180 - 180 / Math.PI * Math.log(Math.tan(Math.PI / 4 + latitude * Math.PI / 360))) / 360];
}
export function metreScale(latitude: number) { return 1 / (EARTH_CIRCUMFERENCE * Math.cos(latitude * Math.PI / 180)); }
export function emptyMesh(): MeshBuffers { const mesh = {} as MeshBuffers; for (const material of MATERIALS) mesh[material] = []; return mesh; }

export function polygonCenter(polygon: Polygon): Point2 {
  const ring = polygon.coordinates[0];
  let west = Infinity, east = -Infinity, south = Infinity, north = -Infinity;
  for (const [x, y] of ring) { west = Math.min(west, x); east = Math.max(east, x); south = Math.min(south, y); north = Math.max(north, y); }
  return [(west + east) / 2, (south + north) / 2];
}

export function appendBuildingMesh(buffers: MeshBuffers, polygon: Polygon, origin: Point2, scale: number, groundM: number, baseM: number, heightM: number, facade: FacadeMaterial, groundAt?: (point: Point2) => number, appearance?: BuildingAppearance) {
  if (!polygon.coordinates[0] || polygon.coordinates[0].length < 3 || !polygon.coordinates.flat().every((p) => p.length >= 2 && Number.isFinite(p[0]) && Number.isFinite(p[1]))) return;
  const rings = polygon.coordinates.map((ring) => {
    const points = ring.length > 1 && ring[0][0] === ring[ring.length - 1][0] && ring[0][1] === ring[ring.length - 1][1] ? ring.slice(0, -1) : ring;
    return points.map((point): Point2 => { const p = mercator(point); return [(p[0] - origin[0]) / scale, (p[1] - origin[1]) / scale]; });
  }).filter((ring) => ring.length >= 3);
  if (!rings.length) return;
  const top = groundM + heightM;
  const tint = appearance?.tint ?? [1, 1, 1], roofTint = appearance?.roofTint ?? [1, 1, 1];
  const shell = rings[0];
  const lengths = shell.map((a, i) => Math.hypot(a[0] - shell[(i + 1) % shell.length][0], a[1] - shell[(i + 1) % shell.length][1]));
  const rectangular = rings.length === 1 && shell.length === 4 && lengths.every((length) => length > 2) && shell.every((a, i) => {
    const b = shell[(i + 1) % 4], c = shell[(i + 2) % 4];
    return Math.abs((b[0] - a[0]) * (c[0] - b[0]) + (b[1] - a[1]) * (c[1] - b[1])) / lengths[i] / lengths[(i + 1) % 4] < 0.3;
  });
  const rise = rectangular && appearance?.profile !== "flat" && appearance ? Math.min(3.2, Math.min(...lengths) * 0.3, (heightM - baseM) * 0.3) : 0;
  const eaves = top - rise;
  const bottomAt = (point: Point2) => {
    if (!groundAt) return groundM + baseM;
    const mx = point[0] * scale + origin[0], my = point[1] * scale + origin[1];
    const latitude = Math.atan(Math.sinh(Math.PI * (1 - 2 * my))) * 180 / Math.PI;
    return Math.min(groundM, groundAt([mx * 360 - 180, latitude])) + baseM - 0.1;
  };
  const push = (buffer: number[], point: Point2, z: number, u: number, v: number, normal: number[], colour: Tint) => buffer.push(point[0] * scale, point[1] * scale, z * scale, u, v, ...normal, ...colour);
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
      const v1 = (eaves - groundM) / repeat;
      const wall = buffers[facade];
      const bottomA = bottomAt(a), bottomB = bottomAt(b);
      const v0A = (bottomA - groundM) / repeat, v0B = (bottomB - groundM) / repeat;
      push(wall, a, bottomA, u0, v0A, normal, tint); push(wall, b, bottomB, u1, v0B, normal, tint); push(wall, a, eaves, u0, v1, normal, tint);
      push(wall, a, eaves, u0, v1, normal, tint); push(wall, b, bottomB, u1, v0B, normal, tint); push(wall, b, eaves, u1, v1, normal, tint);
      distance += length;
    }
  }
  const roofMaterial = appearance?.roof ?? "terracotta", roof = buffers[roofMaterial], repeat = materialRepeatMetres(roofMaterial), anchor = rings[0][0];
  const uv = (point: Point2): Point2 => {
    const x = point[0] - anchor[0], y = point[1] - anchor[1], angle = appearance?.rotation ?? 0;
    return [(Math.cos(angle) * x - Math.sin(angle) * y) / repeat + (appearance?.phase ?? 0), (Math.sin(angle) * x + Math.cos(angle) * y) / repeat];
  };
  if (rise > 0.2) {
    // Synthetic roof profiles stay inside suitable quadrilaterals and retain
    // the provider rendering height at the ridge. Courtyard roofs stay flat.
    type Vertex = [number, number, number];
    const start = lengths.indexOf(Math.max(...lengths));
    const [a, b, c, d] = Array.from({ length: 4 }, (_, i) => shell[(start + i) % 4]);
    const left: Point2 = [(a[0] + d[0]) / 2, (a[1] + d[1]) / 2], right: Point2 = [(b[0] + c[0]) / 2, (b[1] + c[1]) / 2];
    const inset = appearance?.profile === "hipped" ? Math.min(0.3, Math.min(...lengths) / Math.max(...lengths) * 0.45) : 0;
    const r0: Vertex = [left[0] + (right[0] - left[0]) * inset, left[1] + (right[1] - left[1]) * inset, top];
    const r1: Vertex = [right[0] + (left[0] - right[0]) * inset, right[1] + (left[1] - right[1]) * inset, top];
    const at = (p: Point2): Vertex => [p[0], p[1], eaves];
    const face = (vertices: Vertex[], attic = false) => {
      const [p, q, r] = vertices, u = q.map((v, i) => v - p[i]), v = r.map((n, i) => n - p[i]);
      let normal = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
      const length = Math.hypot(...normal);
      if (length < 0.001) return;
      normal = normal.map((n) => n / length);
      if (normal[2] < 0) normal = normal.map((n) => -n);
      if (attic) {
        const cx = (p[0] + q[0] + r[0]) / 3 - (left[0] + right[0]) / 2, cy = (p[1] + q[1] + r[1]) / 3 - (left[1] + right[1]) / 2;
        if (normal[0] * cx + normal[1] * cy < 0) normal = normal.map((n) => -n);
      }
      for (let i = 1; i < vertices.length - 1; i++) for (const point of [vertices[0], vertices[i], vertices[i + 1]]) {
        const end = vertices[vertices.length - 1], dx = end[0] - p[0], dy = end[1] - p[1], edge = Math.hypot(dx, dy);
        const tex = attic ? [((point[0] - p[0]) * dx + (point[1] - p[1]) * dy) / Math.max(edge, 0.01) / 6, (point[2] - groundM) / 6] : uv([point[0], point[1]]);
        push(attic ? buffers[facade] : roof, [point[0], point[1]], point[2], tex[0], tex[1], normal, attic ? tint : roofTint);
      }
    };
    face([at(a), at(b), r1, r0]); face([at(d), r0, r1, at(c)]);
    face([at(a), r0, at(d)], !inset); face([at(b), at(c), r1], !inset);
    return;
  }
  for (const index of earcut(flat, holes, 2)) {
    const point: Point2 = [flat[index * 2], flat[index * 2 + 1]];
    const tex = uv(point);
    push(roof, point, top, tex[0], tex[1], [0, 0, 1], roofTint);
  }
}
