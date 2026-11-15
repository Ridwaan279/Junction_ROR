import { MATERIAL_IMAGE_SIZE, MATERIAL_REGIONS, materialAtlasUrl, type TextureMaterial } from "./building-material-data";

/** Decode each shared atlas once; caller owns/ closes the resulting cropped bitmaps. */
export async function loadMaterialImages<T extends TextureMaterial>(materials: readonly T[], signal: AbortSignal): Promise<Record<T, ImageBitmap>> {
  const atlases = new Map<string, ImageBitmap>(), images = {} as Record<T, ImageBitmap>;
  try {
    for (const material of materials) {
      const url = materialAtlasUrl(material);
      if (!atlases.has(url)) {
        const response = await fetch(url, { signal });
        if (!response.ok) throw new Error("Material atlas could not load");
        const atlas = await createImageBitmap(await response.blob());
        atlases.set(url, atlas);
        if (atlas.width !== 1254 || atlas.height !== 1254) throw new Error("Invalid material atlas");
      }
      const [x, y, width, height] = MATERIAL_REGIONS[material];
      images[material] = await createImageBitmap(atlases.get(url)!, x, y, width, height, { resizeWidth: MATERIAL_IMAGE_SIZE, resizeHeight: MATERIAL_IMAGE_SIZE, resizeQuality: "high", imageOrientation: "flipY" });
      if (signal.aborted) throw new DOMException("Aborted", "AbortError");
    }
    return images;
  } catch (error) {
    for (const bitmap of Object.values(images) as ImageBitmap[]) bitmap.close();
    throw error;
  } finally { for (const bitmap of atlases.values()) bitmap.close(); }
}
