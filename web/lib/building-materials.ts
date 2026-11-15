import type { CustomLayerInterface, CustomRenderMethodInput, Map as LibreMap, MapSourceDataEvent } from "maplibre-gl";
import type { Polygon } from "geojson";
import { BUILDING_LAYER, FOOTPRINT_LAYER, buildingInfo, type BuildingInfo } from "./building-data";
import { MATERIALS, buildingAppearance, type BuildingMaterial, type BuildingAppearance } from "./building-material-data";
import { appendBuildingMesh, emptyMesh, mercator, metreScale, polygonCenter, MESH_STRIDE, type MeshBuffers, type Point2 } from "./building-mesh";
import { loadMaterialImages } from "./material-images";
import { pickMesh, type PickMesh } from "./building-picking";
import { setBuildingsVisible } from "./buildings";

const DETAIL_ZOOM = 16.5;
const DETAIL_RADIUS_M = 1400;
const MAX_BUILDINGS = 3000;
type GpuMaterial = { buffer: WebGLBuffer; vao: WebGLVertexArrayObject; texture: WebGLTexture; count: number };
const vertexShader = `#version 300 es
layout(location=0) in vec3 a_position;
layout(location=1) in vec2 a_uv;
layout(location=2) in vec3 a_normal;
layout(location=3) in vec3 a_tint;
uniform mat4 u_matrix;
out vec2 v_uv;
out vec3 v_normal;
out vec3 v_tint;
void main() { gl_Position = u_matrix * vec4(a_position, 1.0); v_uv = a_uv; v_normal = a_normal; v_tint = a_tint; }`;
const fragmentShader = `#version 300 es
precision highp float;
uniform sampler2D u_material;
in vec2 v_uv;
in vec3 v_normal;
in vec3 v_tint;
out vec4 color;
void main() {
  float light = 0.57 + 0.43 * max(dot(normalize(v_normal), normalize(vec3(-0.35,-0.45,0.85))), 0.0);
  color = vec4(texture(u_material, v_uv).rgb * light * v_tint, 1.0);
}`;

function compile(gl: WebGL2RenderingContext, type: number, source: string) {
  const shader = gl.createShader(type);
  if (!shader) throw new Error("Building shader unavailable");
  gl.shaderSource(shader, source); gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) { gl.deleteShader(shader); throw new Error("Building shader could not compile"); }
  return shader;
}

/** Metre-scaled illustrative materials on nearby mapped polygons. Plain extrusions are the fallback. */
export class BuildingMaterialLayer implements CustomLayerInterface {
  readonly id = "portugal-building-materials";
  readonly type = "custom" as const;
  readonly renderingMode = "3d" as const;
  private map!: LibreMap;
  private program?: WebGLProgram;
  private materials: Partial<Record<BuildingMaterial, GpuMaterial>> = {};
  private bitmaps: Partial<Record<BuildingMaterial, ImageBitmap>> = {};
  private matrixLocation: WebGLUniformLocation | null = null;
  private source = "";
  private loaded = false;
  private failed = false;
  private removed = false;
  private visible = false;
  private textured = false;
  private active = false;
  private revision = 0;
  private refreshTimer: ReturnType<typeof setTimeout> | null = null;
  private lastFrameCheck = 0;
  private signature = "";
  private dirty = true;
  private origin: Point2 = [0, 0];
  private scale = 1;
  private mesh: MeshBuffers = emptyMesh();
  private picks: PickMesh[] = [];
  private matrix = new Float32Array(16);

  constructor(private onFailure: () => void) {}

  onAdd(map: LibreMap) {
    this.map = map;
    const layer = map.getStyle().layers.find((layer) => layer.id === BUILDING_LAYER);
    if (!layer || !("source" in layer) || typeof layer.source !== "string") throw new Error("Building source unavailable");
    this.source = layer.source;
    map.on("idle", this.refresh); map.on("moveend", this.refresh); map.on("zoom", this.syncVisibility);
    map.on("sourcedata", this.sourceChanged);
    map.on("render", this.refreshAfterFrame);
  }

  async load(signal: AbortSignal) {
    this.bitmaps = await loadMaterialImages(MATERIALS, signal);
    if (signal.aborted || this.removed) { this.releaseBitmaps(); throw new DOMException("Aborted", "AbortError"); }
    this.loaded = true; this.refresh();
  }

  setDisplay(visible: boolean, textured: boolean) {
    this.visible = visible; this.textured = textured;
    this.signature = ""; this.refresh(); this.syncVisibility();
  }

  private sourceChanged = (event: MapSourceDataEvent) => {
    if (event.sourceDataType === "content" && (event.sourceId === this.source || event.sourceId === "portugal-terrain")) {
      this.revision++;
      // Terrain/label fading can postpone idle. Refresh when source data arrives
      // too, coalescing bursts of tile events into one mesh rebuild.
      if (this.refreshTimer === null && this.visible && this.textured && this.map.getZoom() >= DETAIL_ZOOM) this.refreshTimer = setTimeout(() => {
        this.refreshTimer = null;
        this.refresh();
      }, 180);
    }
  };

  private syncVisibility = () => {
    if (this.removed) return;
    this.active = this.visible && this.textured && this.loaded && !this.failed && this.map.getZoom() >= DETAIL_ZOOM && this.picks.length > 0;
    // A visible 3D source consumer must remain registered while terrain is on.
    // Hiding this layer discards the non-draped tile coverage querySourceFeatures
    // needs. Zero opacity skips its draw/depth pass while retaining that coverage.
    if (this.map.getLayoutProperty(BUILDING_LAYER, "visibility") !== (this.visible ? "visible" : "none")) setBuildingsVisible(this.map, this.visible);
    const nativeOpacity = this.active ? 0 : 1;
    if (this.map.getPaintProperty(BUILDING_LAYER, "fill-extrusion-opacity") !== nativeOpacity) this.map.setPaintProperty(BUILDING_LAYER, "fill-extrusion-opacity", nativeOpacity);
    const footprintOpacity = this.visible ? 0.25 : 0.7;
    if (this.map.getPaintProperty(FOOTPRINT_LAYER, "fill-opacity") !== footprintOpacity) this.map.setPaintProperty(FOOTPRINT_LAYER, "fill-opacity", footprintOpacity);
  };

  private refreshAfterFrame = () => {
    // Terrain coverage/refinement becomes queryable after sourcedata. Recheck on
    // actual rendered frames; this does not create a continuous repaint loop.
    if (!this.loaded || !this.visible || !this.textured || this.failed || this.map.getZoom() < DETAIL_ZOOM || this.map.isMoving()) return;
    const now = performance.now();
    if (now - this.lastFrameCheck < 400) return;
    this.lastFrameCheck = now;
    if (!this.picks.length) this.signature = "";
    this.refresh();
  };

  private refresh = () => {
    if (this.removed) return;
    if (!this.loaded || !this.visible || !this.textured || this.failed || this.map.getZoom() < DETAIL_ZOOM) { this.syncVisibility(); return; }
    const center = this.map.getCenter(), origin = mercator([center.lng, center.lat]), scale = metreScale(center.lat);
    // DEMs can become drawable after their source event, so revision alone is
    // insufficient. Sample the nearby terrain to catch late elevation updates.
    const groundStamp = [-1, 0, 1].flatMap((dx) => [-1, 0, 1].map((dy) => Math.round((this.map.queryTerrainElevation([center.lng + dx * 0.012, center.lat + dy * 0.009]) ?? 0) * 5))).join(",");
    const signature = `${Math.round(origin[0] / scale / 20)}/${Math.round(origin[1] / scale / 20)}/${Math.floor(this.map.getZoom())}/${this.revision}/${this.map.getTerrain()?.exaggeration}/${groundStamp}`;
    if (signature === this.signature) return;
    this.signature = signature;
    const seen = new Set<string>();
    const candidates: { polygon: Polygon; info: BuildingInfo; distance: number; appearance: BuildingAppearance }[] = [];
    const features = this.map.querySourceFeatures(this.source, { sourceLayer: "building" });
    for (const feature of features) {
      if (feature.properties?.hide_3d === true) continue;
      const polygons = feature.geometry.type === "Polygon" ? [feature.geometry.coordinates] : feature.geometry.type === "MultiPolygon" ? feature.geometry.coordinates : [];
      for (const coordinates of polygons) {
        if (!coordinates[0]?.length) continue;
        const polygon: Polygon = { type: "Polygon", coordinates };
        const point = polygonCenter(polygon), position = mercator(point);
        const distance = Math.hypot(position[0] - origin[0], position[1] - origin[1]) / scale;
        if (distance > DETAIL_RADIUS_M) continue;
        const key = JSON.stringify(coordinates);
        if (seen.has(key)) continue;
        seen.add(key);
        candidates.push({ polygon, info: buildingInfo(feature.properties ?? {}, ...point), distance, appearance: buildingAppearance(point) });
      }
    }
    candidates.sort((a, b) => a.distance - b.distance);
    const mesh = emptyMesh(), picks: PickMesh[] = [];
    for (const { polygon, info, appearance } of candidates.slice(0, MAX_BUILDINGS)) {
      const ranges = Object.fromEntries(MATERIALS.map((material) => [material, [mesh[material].length, 0]])) as PickMesh["ranges"];
      const groundAt = (point: Point2) => this.map.queryTerrainElevation(point) ?? 0;
      // Level roof over the highest sampled ground; foundations follow the DEM.
      const ground = polygon.coordinates[0].reduce((height, point) => Math.max(height, groundAt([point[0], point[1]])), groundAt(info.center));
      appendBuildingMesh(mesh, polygon, origin, scale, ground, info.baseHeightM, info.renderHeightM, appearance.facade, groundAt, appearance);
      for (const material of MATERIALS) ranges[material][1] = mesh[material].length;
      if (MATERIALS.some((material) => ranges[material][1] > ranges[material][0])) picks.push({ info, ranges });
    }
    this.origin = origin; this.scale = scale; this.mesh = mesh; this.picks = picks; this.dirty = true;
    this.syncVisibility(); this.map.triggerRepaint();
  };

  // All GPU uploads happen inside MapLibre's custom render pass. Uploading from
  // async fetch callbacks would invalidate its cached GL texture/buffer state.
  private initialize(gl: WebGL2RenderingContext) {
    const program = gl.createProgram();
    if (!program) throw new Error("Building renderer unavailable");
    this.program = program;
    const vert = compile(gl, gl.VERTEX_SHADER, vertexShader), frag = compile(gl, gl.FRAGMENT_SHADER, fragmentShader);
    gl.attachShader(program, vert); gl.attachShader(program, frag); gl.linkProgram(program);
    gl.deleteShader(vert); gl.deleteShader(frag);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error("Building program could not link");
    this.matrixLocation = gl.getUniformLocation(program, "u_matrix");
    gl.useProgram(program); gl.uniform1i(gl.getUniformLocation(program, "u_material"), 0); gl.activeTexture(gl.TEXTURE0);
    const anisotropy = gl.getExtension("EXT_texture_filter_anisotropic");
    for (const material of MATERIALS) {
      const buffer = gl.createBuffer(), vao = gl.createVertexArray(), texture = gl.createTexture();
      if (!buffer || !vao || !texture) throw new Error("Building GPU resources unavailable");
      this.materials[material] = { buffer, vao, texture, count: 0 };
      gl.bindVertexArray(vao); gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
      for (const [index, size, offset] of [[0, 3, 0], [1, 2, 12], [2, 3, 20], [3, 3, 32]]) {
        gl.enableVertexAttribArray(index); gl.vertexAttribPointer(index, size, gl.FLOAT, false, MESH_STRIDE * 4, offset);
      }
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, this.bitmaps[material]!);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.REPEAT); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.REPEAT);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.generateMipmap(gl.TEXTURE_2D);
      if (anisotropy) gl.texParameterf(gl.TEXTURE_2D, anisotropy.TEXTURE_MAX_ANISOTROPY_EXT, Math.min(8, gl.getParameter(anisotropy.MAX_TEXTURE_MAX_ANISOTROPY_EXT)));
    }
    this.releaseBitmaps();
  }

  render(gl: WebGL2RenderingContext, input: CustomRenderMethodInput) {
    if (!this.active) return;
    try {
      if (!this.program) this.initialize(gl);
      const matrix = input.defaultProjectionData.mainMatrix;
      this.matrix.set(matrix);
      for (let i = 0; i < 4; i++) this.matrix[12 + i] = matrix[12 + i] + matrix[i] * this.origin[0] + matrix[4 + i] * this.origin[1];
      gl.useProgram(this.program!); gl.uniformMatrix4fv(this.matrixLocation, false, this.matrix);
      gl.enable(gl.DEPTH_TEST); gl.depthMask(true); gl.depthFunc(gl.LEQUAL); gl.disable(gl.CULL_FACE); gl.activeTexture(gl.TEXTURE0);
      for (const material of MATERIALS) {
        const gpu = this.materials[material]!;
        gl.bindVertexArray(gpu.vao);
        if (this.dirty) {
          gl.bindBuffer(gl.ARRAY_BUFFER, gpu.buffer); gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(this.mesh[material]), gl.STATIC_DRAW);
          gpu.count = this.mesh[material].length / MESH_STRIDE;
        }
        gl.bindTexture(gl.TEXTURE_2D, gpu.texture); gl.drawArrays(gl.TRIANGLES, 0, gpu.count);
      }
      this.dirty = false;
    } catch {
      this.failed = true; this.syncVisibility(); this.onFailure();
    } finally { gl.bindVertexArray(null); }
  }

  pick(x: number, y: number): BuildingInfo | null {
    if (!this.active || this.dirty) return null;
    const canvas = this.map.getCanvas();
    // Public unproject uses the drawn terrain surface. A hill in front of a
    // building must occlude the CPU pick just as it occludes the GPU geometry.
    const groundPoint = this.map.unproject([x, y]), ground = mercator([groundPoint.lng, groundPoint.lat]);
    const px = ground[0] - this.origin[0], py = ground[1] - this.origin[1], pz = (this.map.queryTerrainElevation(groundPoint) ?? 0) * this.scale;
    const m = this.matrix, w = m[3] * px + m[7] * py + m[11] * pz + m[15];
    const depth = (m[2] * px + m[6] * py + m[10] * pz + m[14]) / w;
    const maxDepth = w > 0 && Number.isFinite(depth) ? Math.min(1, depth + 1e-6) : 1;
    return pickMesh(this.mesh, this.picks, m, canvas.clientWidth, canvas.clientHeight, x, y, maxDepth);
  }

  private releaseBitmaps() {
    for (const bitmap of Object.values(this.bitmaps)) bitmap.close();
    this.bitmaps = {};
  }

  onRemove(map: LibreMap, gl: WebGL2RenderingContext) {
    this.removed = true; this.active = false; this.releaseBitmaps();
    if (this.refreshTimer !== null) clearTimeout(this.refreshTimer);
    map.off("idle", this.refresh); map.off("moveend", this.refresh); map.off("zoom", this.syncVisibility); map.off("sourcedata", this.sourceChanged);
    map.off("render", this.refreshAfterFrame);
    for (const gpu of Object.values(this.materials)) { gl.deleteBuffer(gpu.buffer); gl.deleteVertexArray(gpu.vao); gl.deleteTexture(gpu.texture); }
    if (this.program) gl.deleteProgram(this.program);
  }
}
