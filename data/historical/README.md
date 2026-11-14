# Historical fire datasets

Datasets for replaying real Portuguese wildfires through the EmberWatch forecast,
smoke and drone-planning pipeline when no fire is burning.

## `arganil-piodao-2025/dataset.json`

Arganil / Piódão wildfire, 13–24 August 2025: the largest burned area on record in
Portugal (64,451 ha, ICNF provisional). Built by
`cargo run --release -p emberwatch-firecast --example build_dataset`.

| Content | Source | Notes |
|---|---|---|
| 12 dated burned-area perimeters, 15–20 Aug (7,614 → 54,670 ha) | GWIS via the GDACS API, event 1024588, GLIDE WF-2025-000140-PRT | Satellite-derived perimeters with `polygondate`; © European Union, reuse with attribution. |
| Hourly weather 12–21 Aug at 9 points | ERA5 reanalysis via the Open-Meteo historical API | 10 m and 100 m wind, gusts, temperature, humidity, cloud, radiation, rain, VPD. CC BY 4.0 (Open-Meteo); ERA5 © ECMWF / Copernicus. |
| Terrain, 211 × 211 cells at 200 m | Terrain Tiles (Mapzen / AWS Open Data), zoom 11 | Open data; attribution to Mapzen and the underlying DEM sources. |
| Dated web reports with extracted facts | Portuguese news (Renascença, JN, Observador, DN, NiT, Bombeiros.pt …) and the GDACS media monitor | Titles, URLs and short factual extractions only, as Context.dev would capture them. |
| Ignition and detection | Reports: start around 05:00 local on the slope above Piódão, alert at 05:08 (ANEPC), attributed by local officials to dry lightning | Ignition position is the village coordinate (approximate). |
| Vegetation | Altitudinal zonation assumption (OSM land cover was unreachable) | Grass/rock above 1500 m, shrubland 800–1500 m, maritime pine 400–800 m, eucalyptus below 400 m. |

The replay (`--example replay`) writes `web/public/historical/arganil-piodao-2025.json`,
which the web app plays in its "Arganil 2025 (histórico)" view.
