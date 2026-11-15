import type { Map as LibreMap } from "maplibre-gl";
import { GROUND_MATERIALS } from "./building-material-data";
import { GROUND_SPRITE_SIZE, GROUND_ZOOMS, groundImageId, groundPixelRatio, groundTextureLayers, type SurfaceGroup } from "./ground-material-data";
import { loadMaterialImages } from "./material-images";

export class GroundMaterials {
  private layers: { id: string; group: SurfaceGroup }[] = [];
  private loaded = false;
  private enabled: Record<SurfaceGroup, boolean> = { roads: false, parks: false };
  constructor(private map: LibreMap) {}

  async load(signal: AbortSignal) {
    const images = await loadMaterialImages(GROUND_MATERIALS, signal);
    try {
      for (const material of GROUND_MATERIALS) {
        // Native line patterns clip vertically to the sprite's logical height.
        // Tile road materials vertically so even broad roads remain covered,
        // retaining the small aggregate/paving scale instead of stretching it.
        const repeats = material === "asphalt" || material === "calcada" ? 8 : 1;
        const canvas = new OffscreenCanvas(GROUND_SPRITE_SIZE, GROUND_SPRITE_SIZE * repeats);
        const context = canvas.getContext("2d");
        if (!context) throw new Error("Surface texture packing unavailable");
        for (let row = 0; row < repeats; row++) context.drawImage(images[material], 0, row * GROUND_SPRITE_SIZE, GROUND_SPRITE_SIZE, GROUND_SPRITE_SIZE);
        const sprite = await createImageBitmap(canvas);
        try {
          if (signal.aborted) throw new DOMException("Aborted", "AbortError");
          for (const zoom of GROUND_ZOOMS) this.map.addImage(groundImageId(material, zoom), sprite, { pixelRatio: groundPixelRatio(material, zoom) });
        } finally { sprite.close(); }
      }
      for (const { layer, before, group } of groundTextureLayers(this.map.getStyle())) {
        this.map.addLayer(layer, before);
        if (!this.map.getLayer(layer.id)) throw new Error("Surface layer could not be added");
        this.layers.push({ id: layer.id, group });
      }
      this.loaded = true; this.setDisplay(this.enabled.roads, this.enabled.parks);
    } finally { for (const bitmap of Object.values(images)) bitmap.close(); }
  }

  setDisplay(roads: boolean, parks: boolean) {
    this.enabled = { roads, parks };
    if (!this.loaded) return;
    for (const { id, group } of this.layers) {
      const visibility = this.enabled[group] ? "visible" : "none";
      if (this.map.getLayoutProperty(id, "visibility") !== visibility) this.map.setLayoutProperty(id, "visibility", visibility);
    }
  }
}
