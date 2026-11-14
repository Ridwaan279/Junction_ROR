# Portugal building views

Checked 15 November 2026. The user chose the free building layer first.
The existing terrain version is preserved on `codex/portugal-terrain-3d`;
building work is on `codex/portugal-earth-buildings`.

## What is real and what is approximate

The [OpenFreeMap Liberty style](https://tiles.openfreemap.org/styles/liberty)
already contained an OpenMapTiles `building` layer and a `building-3d` extrusion
layer at zoom 14. Our previous city presets were too far out to show much of it.
This branch makes building views discoverable, improves opaque lighting/styling,
adds 2D/3D visibility controls and exposes source/height limitations on click.
It reuses the style's OpenFreeMap vector source, or adds that known source if a
custom style uses a different dataset. Existing building layers are hidden to
avoid rendering two overlapping models.
Selection checks visible extrusions before flat footprints. Tile-local IDs can
repeat and one feature may group separate buildings. The inspector therefore
marks the clicked screen position instead of recolouring a feature group. The
marker and inspector clear on camera movement so the marker cannot drift onto
a different building. Tile attributes do not establish an individual building's
identity or surveyed measurements.

| Property | Meaning and limitation |
| --- | --- |
| Position / footprint | Real mapped OSM polygon geometry, not generated placement. Mapping accuracy, generalization, completeness and age vary. A polygon is not proof of surveyed dimensions or current existence. |
| `render_height` | [OpenMapTiles documents an approximate rendering height](https://openmaptiles.org/docs/schema/#building). It can derive from OSM height, floors or a default. The tile does not expose per-feature provenance sufficient to call it measured. |
| `render_min_height` | Approximate height of the bottom of a building part. Rendering constrains it between ground and the rendered top. |
| Missing/invalid render height | A labelled 5 m display fallback. The source height stays null. This fallback is never stored as a measured dimension. |
| Verified height | Unknown. No survey, LiDAR or per-building authoritative measurements are integrated. |
| Roofs / façades | Simplified extruded polygons with provider colour or an optional generated Portuguese material preview. Preview is off by default; roof tile scale and aliasing are unfinished. No address-matched photographs or detailed roof meshes. |
| Ground elevation | Mapterhorn terrain. City presets choose 1× terrain exaggeration; changing ground exaggeration does not scale building height values. Alignment remains limited by the source DEM and mapped polygon resolution. |

OpenMapTiles' [building implementation](https://github.com/openmaptiles/openmaptiles/tree/master/layers/building)
and the [MapLibre building example](https://maplibre.org/maplibre-gl-js/docs/examples/display-buildings-in-3d/)
are the primary implementation references. OpenFreeMap/OpenMapTiles/OSM attribution
stays visible. OpenStreetMap data is subject to its
[ODbL terms and attribution requirements](https://www.openstreetmap.org/copyright).
There is no bulk geometry download, external building database or new paid service.

## Texture and close-view WIP checkpoint

Zoom now reaches 22, tilt reaches 85°, and Close view / the building inspector's
closer-view action move the camera to building scale. Source geometry retains
its existing resolution. The generated Portuguese atlas includes ivory stucco,
blue-and-white azulejos, granite and terracotta. The native-pattern preview is
optional and off by default because roof tiles can appear too large at close
range. Generated materials are labelled illustrative in English and Portuguese.
See the [asset provenance and complete prompt](../web/public/textures/portugal/README.md).

Remaining: connect the metre-based polygon/UV builder to a custom WebGL renderer,
add mipmaps/anisotropic filtering, verify terrain placement and picking, then
repeat close-up desktop/mobile checks. This checkpoint is not the finished
realistic building renderer. No claim is made that a generated façade matches a
particular address, or that OSM height attributes establish surveyed dimensions.

## Why Context.dev is not the geometry source

Context.dev remains the project's Fogos public-page extraction route. It could
help research a particular landmark's public description or cited dimensions,
with source permission and matching checked separately. Scraping text or webpage
screenshots does not supply a licensed, georeferenced city mesh, building survey,
or complete photographic textures. This branch uses the existing open vector
geometry directly and makes no Context calls or credit purchases.

## A closer Google Earth appearance later

[Google 3D Maps](https://developers.google.com/maps/documentation/javascript/3d/overview)
can display photorealistic aerial imagery and building meshes using `Map3DElement`
in satellite/hybrid modes. Another route is
[Photorealistic 3D Tiles](https://developers.google.com/maps/documentation/tile/3d-tiles)
with a compatible 3D Tiles renderer such as CesiumJS. Both are separate from the
free extrusion layer; neither is implemented or presented as working here.

Before implementation, the owner would need to configure the applicable enabled
API, a restricted browser key, billing and usage limits under Google's current
terms. [3D surface coverage is location-dependent](https://developers.google.com/maps/documentation/javascript/3d/coverage),
so Lisboa/Porto and the intended rural area must be checked individually.
EEA billing-address terms and available content also require checking. Provider
attribution must remain visible. No key should be pasted into chat or committed.
Google Earth imagery is not scraped or copied. AI-generated building textures
would be illustrative and cannot establish actual appearance or dimensions.

## Verification

Frontend unit tests cover unknown/invalid height provenance, numeric tile
properties, safe base heights, source-coordinate ordering and city-scale camera
navigation. Browser QA checks the city geometry, footprint/3D toggle, selection
panel, Portuguese/English controls and responsive layout. A successful rendering
check establishes that the app draws the supplied geometry; it does not validate
the buildings against a ground survey.
