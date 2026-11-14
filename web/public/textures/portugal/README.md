# Portuguese material atlas — WIP

Generated on 15 November 2026 using the built-in `image_gen` tool. No API key,
paid map service, downloaded façade photograph or external texture pack is used.
`material-atlas.webp` is the project asset: 1254 × 1254 pixels, 616,962 bytes,
encoded as WebP quality 90 using Sharp. Generation requested 2048 × 2048; the
returned image was 1254 × 1254. Crop coordinates are in
`web/lib/building-material-data.ts`; crops are resampled to 512 × 512 at runtime.

The atlas contains ivory rendered façades with green shutters and iron balconies,
blue-and-white azulejo façades, terracotta roof tiles and granite masonry.
These are illustrative architectural materials, not photographs of mapped
addresses or evidence of a building's actual appearance. See
[Visit Portugal's architectural tile overview](https://www.visitportugal.com/en/content/country-tiles)
for regional context. Individual material assignments are synthetic; mapped
footprints and approximate height attributes still come from OSM/OpenMapTiles.

The current native MapLibre pattern preview is off by default because roof
patterns can appear oversized and texture scale/aliasing remains unfinished.
`building-mesh.ts` starts a replacement renderer's metre-based geometry and UVs;
it is not connected to the map yet. Pitched roofs and surveyed façade geometry
are not included.

## Final generation prompt

```text
Use case: photorealistic-natural. Asset type: game-ready Portuguese architecture seamless albedo material atlas for a geographic 3D building renderer. Generate ONE square 2048 by 2048 image, exactly a 2 by 2 grid of equal square material panels with NO borders, NO padding, NO labels, NO text. Every panel is front-on orthographic, physically realistic, evenly lit diffuse albedo with NO cast shadows, NO perspective, NO vignette, NO scenic background, no sky, no pavement. TOP LEFT quadrant: seamless repeating Lisbon Portuguese pale warm ivory lime-render facade, two window bays by two floors. Authentic tall dark glazed sash / French windows, pale limestone surrounds, narrow dark green wooden shutters, slim black wrought-iron balcony rails, subtle aged plaster. All edges terminate in plain plaster so the facade repeats horizontally and vertically without cutting windows. TOP RIGHT quadrant: seamless repeating Porto traditional muted blue-and-white small-scale azulejo-tiled facade, two bays by two floors of tall white-painted wood sash windows, carved grey granite surrounds, fine black iron balcony railings, restrained realistic weathering. Tile design distinctly Portuguese, not Moroccan fantasy. All edges continue the tile wall and do not cut windows, repeatable on both axes. BOTTOM LEFT quadrant: seamless top-down photorealistic Portuguese orange and russet terracotta barrel roof tiles, overlapping rows, plausible curved clay tiles, fine moss and weathering, no ridge, eaves, chimney or facade, repeatable both axes. BOTTOM RIGHT quadrant: seamless front-on northern Portuguese light-grey weathered granite ashlar wall albedo, irregular dressed stone blocks with subtle lime mortar, subtle natural stone grain, no windows or plants. Photographic-quality surface detail, realistic subdued colours. The four quadrants must meet exactly on the central horizontal and vertical lines, no blend between quadrants. This is an illustrative architectural material atlas, not an image of a specific real building.
```
