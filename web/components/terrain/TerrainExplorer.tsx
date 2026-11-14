"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import Link from "next/link";
import type { Map as LibreMap, GeoJSONSource } from "maplibre-gl";
import { addTerrainLayers, loadMapLibre, loadTerrainStyle, parseCoordinates, placeCamera, PLACES, PORTUGAL_BOUNDS, searchPlaces, setTerrainVisibility, type TerrainPlace } from "@/lib/terrain";
import TerrainIcon from "./TerrainIcon";
import { portugueseDescriptions, terrainCopy } from "./terrain-copy";
import { addBuildingLayers, setBuildingsVisible } from "@/lib/buildings";
import { BUILDING_LAYER, FOOTPRINT_LAYER, buildingInfo, type BuildingInfo } from "@/lib/building-data";
import s from "./terrain.module.css";

export default function TerrainExplorer() {
  const [locale, setLocale] = useState<"en" | "pt">("en");
  const t = terrainCopy[locale];
  const host = useRef<HTMLDivElement>(null);
  const map = useRef<LibreMap | null>(null);
  const [ready, setReady] = useState(false);
  const [failure, setFailure] = useState<keyof typeof terrainCopy.en.errors | null>(null);
  const [terrainWarning, setTerrainWarning] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<TerrainPlace | null>(null);
  const [threeD, setThreeD] = useState(false);
  const [relief, setRelief] = useState(true);
  const [contours, setContours] = useState(true);
  const [buildings, setBuildings] = useState(true);
  const [landmarks, setLandmarks] = useState(false);
  const [selectedBuilding, setSelectedBuilding] = useState<BuildingInfo | null>(null);
  const [buildingPoint, setBuildingPoint] = useState<{ x: number; y: number } | null>(null);
  const [exaggeration, setExaggeration] = useState(1.3);
  const [orbit, setOrbit] = useState(false);
  const [panelOpen, setPanelOpen] = useState(true);
  const [elevation, setElevation] = useState<number | null>(null);
  const [toast, setToast] = useState<"copied" | "manualShare" | "">("");
  const [camera, setCamera] = useState({ longitude: -8.13, latitude: 39.55, zoom: 6.4, pitch: 0, bearing: 0 });
  const results = searchPlaces(query);
  const coordinate = parseCoordinates(query);
  const placeTitle = (place: TerrainPlace) => place.id === "pin" ? t.pin : place.id === "coordinates" ? t.coordinates : place.name;
  const placeDescription = (place: TerrainPlace) => place.id === "pin" ? t.pinDetail : place.id === "coordinates" ? t.coordinatesDetail : locale === "pt" ? portugueseDescriptions[place.id] ?? place.description : place.description;
  const clearBuilding = useCallback(() => {
    setSelectedBuilding(null);
    setBuildingPoint(null);
  }, []);

  useEffect(() => { document.documentElement.lang = locale; }, [locale]);

  useEffect(() => {
    let cancelled = false;
    let instance: LibreMap | null = null;
    let wideScreen = window.innerWidth > 800;
    const resizeCamera = () => {
      clearBuilding();
      const nextWide = window.innerWidth > 800;
      if (!instance || wideScreen === nextWide) return;
      wideScreen = nextWide;
      if (instance.getZoom() < 8) {
        instance.fitBounds(PORTUGAL_BOUNDS, { padding: { top: 100, right: 65, bottom: 100, left: nextWide ? 400 : 50 }, pitch: instance.getPitch(), bearing: instance.getBearing(), duration: 0 });
      } else instance.setPadding({ top: 0, bottom: 0, left: nextWide ? 220 : 0, right: 0 });
    };
    window.addEventListener("resize", resizeCamera);
    const controller = new AbortController();
    const timeout = window.setTimeout(() => {
      controller.abort();
      if (!cancelled) setFailure("timeout");
    }, 25_000);
    (async () => {
      try {
        const [libre, style] = await Promise.all([loadMapLibre(), loadTerrainStyle(controller.signal)]);
        if (cancelled || !host.current) return;
        for (const layer of style.layers) {
          if (layer.type === "symbol" && layer["source-layer"] === "poi") layer.layout = { ...layer.layout, visibility: "none" };
        }
        const hasSharedCamera = window.location.hash.includes("terrain=");
        instance = new libre.Map({
          container: host.current, style, center: [-8.13, 39.55], zoom: 6.4,
          maxBounds: [[-20, 30], [5, 50]], minZoom: 5.2, maxZoom: 17,
          maxPitch: 78, canvasContextAttributes: { antialias: true },
          attributionControl: { compact: true }, hash: "terrain",
        });
        map.current = instance;
        instance.addControl(new libre.ScaleControl({ maxWidth: 120, unit: "metric" }), "bottom-left");
        instance.on("load", async () => {
          if (!instance || cancelled) return;
          try {
            await addTerrainLayers(instance);
            if (cancelled) return;
            addBuildingLayers(instance);
            instance.addSource("explorer-focus", { type: "geojson", data: { type: "FeatureCollection", features: [] } });
            instance.addLayer({ id: "explorer-focus-halo", type: "circle", source: "explorer-focus", paint: { "circle-radius": 18, "circle-color": "#2b6c55", "circle-opacity": 0.12 } });
            instance.addLayer({ id: "explorer-focus-dot", type: "circle", source: "explorer-focus", paint: { "circle-radius": 5, "circle-color": "#2b6c55", "circle-stroke-color": "#fff", "circle-stroke-width": 2 } });
            instance.setSky({ "sky-color": "#c7dce7", "horizon-color": "#edf1e9", "fog-color": "#edf1e9", "sky-horizon-blend": 0.7, "horizon-fog-blend": 0.7, "fog-ground-blend": 0.1 });
            if (!hasSharedCamera) {
              instance.fitBounds(PORTUGAL_BOUNDS, { padding: { top: 100, right: 80, bottom: 80, left: window.innerWidth > 800 ? 400 : 50 }, duration: 0 });
            }
            syncCamera();
            setThreeD(instance.getPitch() > 0);
            setReady(true);
            setFailure(null);
          } catch {
            if (!cancelled) setFailure("elevation");
          } finally {
            window.clearTimeout(timeout);
          }
        });
        const syncCamera = () => {
          if (!instance || cancelled) return;
          const center = instance.getCenter();
          setCamera({ longitude: center.lng, latitude: center.lat, zoom: instance.getZoom(), pitch: instance.getPitch(), bearing: instance.getBearing() });
        };
        instance.on("moveend", syncCamera);
        instance.on("movestart", () => {
          if (!cancelled) { clearBuilding(); setElevation(null); setTerrainWarning(false); }
        });
        instance.on("idle", () => {
          if (!instance || cancelled) return;
          const terrain = instance.getTerrain();
          if (!terrain || !instance.isSourceLoaded("portugal-terrain")) { setElevation(null); return; }
          const height = instance.queryTerrainElevation(instance.getCenter());
          const scale = typeof terrain.exaggeration === "number" ? terrain.exaggeration : 1;
          setElevation(height === null ? null : Math.round(height / scale));
        });
        instance.on("click", (event) => {
          if (!instance || cancelled) return;
          clearBuilding();
          const feature = (instance.getLayer(BUILDING_LAYER) ? instance.queryRenderedFeatures(event.point, { layers: [BUILDING_LAYER] })[0] : undefined)
            ?? (instance.getLayer(FOOTPRINT_LAYER) ? instance.queryRenderedFeatures(event.point, { layers: [FOOTPRINT_LAYER] })[0] : undefined);
          if (feature) {
            const info = buildingInfo(feature.properties, event.lngLat.lng, event.lngLat.lat);
            setSelectedBuilding(info);
            setBuildingPoint({ x: event.point.x, y: event.point.y });
            setSelected(null);
            setPanelOpen(false);
            return;
          }
          const point = event.lngLat;
          const inside = point.lng >= -9.65 && point.lng <= -6.05 && point.lat >= 36.85 && point.lat <= 42.25;
          if (!inside || cancelled) return;
          setSelected({ id: "pin", name: "Dropped pin", region: "Selected location", kind: "mountain",
            description: "Explore this location with the terrain and camera controls.",
            center: [point.lng, point.lat], zoom: instance!.getZoom(), bearing: instance!.getBearing() });
        });
        instance.on("dragstart", () => { if (!cancelled) setOrbit(false); });
        instance.on("webglcontextlost", () => { if (!cancelled) setFailure("context"); });
        instance.on("error", (event) => {
          if (cancelled) return;
          if (event.error?.message.includes("Worker failed")) {
            window.clearTimeout(timeout);
            setFailure("worker");
          }
          if ("sourceId" in event && String(event.sourceId).startsWith("portugal-")) {
            console.warn("Terrain tile unavailable", event.error?.message);
            setTerrainWarning(true);
          }
        });
      } catch {
        if (!cancelled) setFailure("startup");
        window.clearTimeout(timeout);
      }
    })();
    return () => {
      cancelled = true;
      controller.abort();
      window.clearTimeout(timeout);
      window.removeEventListener("resize", resizeCamera);
      instance?.remove();
      map.current = null;
    };
  }, [attempt, clearBuilding]);

  useEffect(() => {
    const instance = map.current;
    if (!instance || !ready) return;
    instance.setTerrain(threeD ? { source: "portugal-terrain", exaggeration } : null);
    setTerrainVisibility(instance, relief, contours && relief);
    setBuildingsVisible(instance, threeD && buildings);
  }, [ready, threeD, exaggeration, relief, contours, buildings]);

  useEffect(() => {
    const instance = map.current;
    if (!instance || !ready) return;
    for (const layer of instance.getStyle().layers) {
      if (layer.type === "symbol" && layer["source-layer"] === "poi") instance.setLayoutProperty(layer.id, "visibility", landmarks ? "visible" : "none");
    }
  }, [ready, landmarks]);

  useEffect(() => {
    if (!ready) return;
    const source = map.current?.getSource("explorer-focus") as GeoJSONSource | undefined;
    source?.setData({ type: "FeatureCollection", features: selected ? [{
      type: "Feature", properties: {}, geometry: { type: "Point", coordinates: selected.center },
    }] : [] });
  }, [selected, ready]);

  useEffect(() => {
    if (!orbit || !threeD || !ready) return;
    const interval = window.setInterval(() => {
      if (!document.hidden && map.current) map.current.jumpTo({ bearing: map.current.getBearing() + 0.25 });
    }, 80);
    return () => window.clearInterval(interval);
  }, [orbit, threeD, ready]);

  useEffect(() => {
    if (!toast) return;
    const timeout = window.setTimeout(() => setToast(""), 3000);
    return () => window.clearTimeout(timeout);
  }, [toast]);

  function fly(place: TerrainPlace, nextMode = threeD) {
    clearBuilding();
    setSelected(place);
    setOrbit(false);
    if (nextMode && place.kind === "city") setExaggeration(1);
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    map.current?.flyTo({ ...placeCamera(place, nextMode), duration: reduce ? 0 : 1700, essential: false, padding: { left: window.innerWidth > 800 ? 220 : 0, right: 0, top: 0, bottom: 0 } });
    if (window.innerWidth <= 800) setPanelOpen(false);
  }
  function changeMode(next: boolean) {
    if (!ready) return;
    setThreeD(next);
    setOrbit(false);
    if (next && ((map.current?.getZoom() ?? 0) < 9 || selected?.kind === "city")) fly(selected ?? PLACES[0], true);
    else map.current?.easeTo({ pitch: next ? 62 : 0, bearing: next ? map.current.getBearing() : 0, duration: 900 });
  }
  function overview() {
    clearBuilding();
    setOrbit(false);
    setSelected(null);
    setQuery("");
    map.current?.fitBounds(PORTUGAL_BOUNDS, { padding: { top: 100, right: 65, bottom: 80, left: window.innerWidth > 800 && panelOpen ? 400 : 65 }, pitch: threeD ? 35 : 0, bearing: 0, duration: 1200 });
  }
  async function share() {
    try { await navigator.clipboard.writeText(window.location.href); setToast("copied"); }
    catch { setToast("manualShare"); }
  }
  function submitSearch(event: React.FormEvent) {
    event.preventDefault();
    if (!ready) return;
    if (coordinate) fly({ id: "coordinates", name: "Selected coordinates", region: "Portugal", description: "A location selected by latitude and longitude.", kind: "mountain", center: coordinate, zoom: 12, bearing: -20 });
    else if (results[0]) fly(results[0]);
  }
  function enterCity() {
    setThreeD(true);
    setBuildings(true);
    setExaggeration(1);
    setContours(false);
    fly(selected?.kind === "city" ? selected : PLACES.find((place) => place.id === "lisboa")!, true);
  }

  return (
    <main className={s.explorer} lang={locale} data-panel-open={panelOpen} data-building-selected={Boolean(selectedBuilding)}>
      <div ref={host} className={s.canvas} aria-label={t.mapLabel} />
      {buildingPoint && <span className={s.buildingPin} style={{ left: buildingPoint.x, top: buildingPoint.y }} aria-hidden="true" />}
      <header className={s.header}>
        <Link href="/" className={s.brand} aria-label={t.back}>
          <span className={s.brandMark}><TerrainIcon name="mountain" size={25} /></span>
          <span>EmberWatch<span className={s.brandDivider}>/</span><strong>Portugal</strong></span>
        </Link>
        <div className={s.headerEnd}>
          <span className={s.mapTag}><span />{t.explorer}</span>
          <Link href="/" className={s.workspaceLink}>{t.workspace} <TerrainIcon name="chevron" size={14} /></Link>
          <div className={s.languages} aria-label="Language / Idioma">{(["en", "pt"] as const).map((code) => <button key={code} aria-pressed={locale === code} onClick={() => setLocale(code)}>{code.toUpperCase()}</button>)}</div>
          <button className={s.share} onClick={() => void share()} aria-label={t.shareLabel}><TerrainIcon name="share" size={17} /><span>{t.share}</span></button>
        </div>
      </header>

      <div className={s.leftColumn}>
        <form className={s.search} onSubmit={submitSearch}>
          <button type="button" className={s.searchMenu} aria-label={panelOpen ? t.hidePanel : t.showPanel} aria-expanded={panelOpen} onClick={() => { if (!panelOpen) clearBuilding(); setPanelOpen(!panelOpen); }}><TerrainIcon name="mountain" size={22} /></button>
          <input aria-label={t.searchLabel} placeholder={t.search} value={query} onChange={(event) => { clearBuilding(); setQuery(event.target.value); setPanelOpen(true); }} />
          {query ? <button type="button" aria-label={t.clear} onClick={() => setQuery("")}><TerrainIcon name="close" size={17} /></button> : <button type="submit" aria-label={t.searchButton}><TerrainIcon name="search" size={19} /></button>}
        </form>
        {panelOpen && <aside className={s.panel}>
          <div className={s.panelHeading}>
            <div className={s.eyebrow}><span className={s.portugalFlag}><i /><i /></span>{t.eyebrow}</div>
            <h1>{t.titleFirst}<br />{t.titleSecond}</h1>
            <p>{t.introFirst}<br />{t.introSecond}</p>
            <div className={s.pills}><span><TerrainIcon name="mountain" size={13} />{t.relief}</span><span><TerrainIcon name="contour" size={13} />{t.contoursPill}</span></div>
            <button className={s.cityLaunch} disabled={!ready} onClick={enterCity}><TerrainIcon name="city" size={18} /><span>{t.exploreBuildings}</span><TerrainIcon name="chevron" size={14} /></button>
          </div>
          <div className={s.listHeading}><span>{query ? t.results : t.find}</span><span>{coordinate ? 1 : results.length} {t.places}</span></div>
          <div className={s.placeList}>
            {coordinate && <button className={s.place} disabled={!ready} onClick={() => fly({ id: "coordinates", name: "Selected coordinates", region: "Portugal", description: "Selected coordinates.", kind: "mountain", center: coordinate, zoom: 12, bearing: -20 })}><span className={s.placeArt}><TerrainIcon name="pin" /></span><span><strong>{coordinate[1].toFixed(4)}, {coordinate[0].toFixed(4)}</strong><small>{t.goCoordinates}</small></span><TerrainIcon name="chevron" size={16} /></button>}
            {!coordinate && results.map((place) => <button key={place.id} className={s.place + (selected?.id === place.id ? " " + s.selectedPlace : "")} onClick={() => fly(place)} disabled={!ready}>
              <span className={s.placeArt + " " + (s[place.kind] ?? "")}><TerrainIcon name={place.kind === "city" ? "pin" : place.kind === "river" ? "contour" : "mountain"} size={23} /></span>
              <span className={s.placeText}><strong>{place.name}</strong><small>{place.region} <span>·</span> {t.kinds[place.kind]}</small></span>
              <TerrainIcon name="chevron" size={16} />
            </button>)}
            {query && !coordinate && results.length === 0 && <p className={s.noResults}>{t.noResults}</p>}
          </div>
          <div className={s.panelFooter}><TerrainIcon name="globe" size={14} /><span>{t.buildingSummary}</span></div>
        </aside>}
        {selectedBuilding && <section className={s.buildingDetail} aria-label={t.buildingDetails}>
          <div className={s.buildingDetailTitle}><TerrainIcon name="city" size={22} /><div><span className={s.eyebrow}>OPENSTREETMAP</span><h2>{selectedBuilding.name ?? t.mappedBuilding}</h2></div><button aria-label={t.closeBuilding} onClick={clearBuilding}><TerrainIcon name="close" size={16} /></button></div>
          <p>{t.footprintDetail}</p>
          <dl><div><dt>{t.renderHeight}</dt><dd>≈ {selectedBuilding.renderHeightM.toLocaleString(locale === "pt" ? "pt-PT" : "en-GB")} m</dd></div><div><dt>{t.heightBasis}</dt><dd>{selectedBuilding.heightBasis === "provider-derived" ? t.derivedHeight : t.defaultHeight}</dd></div><div><dt>{t.measuredHeight}</dt><dd>{t.unknown}</dd></div></dl>
          <p className={s.buildingCaveat}>{t.heightCaveat}</p>
          <a href={selectedBuilding.sourceUrl} target="_blank" rel="noreferrer">{t.viewFootprint}<TerrainIcon name="chevron" size={13} /></a>
        </section>}
        {selected && !query && <section className={s.selection}>
          <span className={s.selectionIcon}><TerrainIcon name={selected.kind === "city" ? "pin" : "mountain"} size={23} /></span>
          <div><span className={s.eyebrow}>{t.current}</span><h2>{placeTitle(selected)}</h2><p>{placeDescription(selected)}</p></div>
          <button aria-label={t.dismiss} onClick={() => setSelected(null)}><TerrainIcon name="close" size={15} /></button>
        </section>}
      </div>

      <div className={s.viewControls}>
        <div className={s.modeSwitch} aria-label={t.perspective}>
          <button aria-pressed={!threeD} onClick={() => changeMode(false)} disabled={!ready}>{t.map2D}</button>
          <button aria-pressed={threeD} onClick={() => changeMode(true)} disabled={!ready}><TerrainIcon name="mountain" size={16} />{t.map3D}</button>
        </div>
        <button className={s.iconButton} aria-label={t.fit} title={t.fit} onClick={overview} disabled={!ready}><TerrainIcon name="expand" size={19} /></button>
      </div>
      <div className={s.cameraTools}>
        <button className={s.compass} aria-label={t.north} title={t.north} disabled={!ready} onClick={() => map.current?.easeTo({ bearing: 0, duration: 600 })}>
          <span style={{ transform: "rotate(" + -camera.bearing + "deg)" }}><i /><i /></span><small>N</small>
        </button>
        <div className={s.zoom}><button aria-label={t.zoomIn} onClick={() => map.current?.zoomIn()} disabled={!ready}><TerrainIcon name="plus" /></button><button aria-label={t.zoomOut} onClick={() => map.current?.zoomOut()} disabled={!ready}><TerrainIcon name="minus" /></button></div>
      </div>

      <section className={s.settings} aria-label={t.display}>
        <div className={s.settingsTitle}><TerrainIcon name="contour" size={19} /><strong>{t.settings}</strong><span>{threeD ? "3D" : "2D"}</span></div>
        <div className={s.switchRow}><label htmlFor="terrain-relief">{t.relief}</label><input id="terrain-relief" type="checkbox" role="switch" checked={relief} onChange={(e) => setRelief(e.target.checked)} disabled={!ready} /></div>
        <div className={s.switchRow}><label htmlFor="terrain-contours">{t.contours}</label><input id="terrain-contours" type="checkbox" role="switch" checked={contours} onChange={(e) => setContours(e.target.checked)} disabled={!ready || !relief} /></div>
        <div className={s.switchRow}><label htmlFor="terrain-buildings">{t.buildings}</label><input id="terrain-buildings" type="checkbox" role="switch" checked={buildings && threeD} onChange={(e) => { setBuildings(e.target.checked); clearBuilding(); }} disabled={!ready || !threeD} /></div>
        <div className={s.switchRow}><label htmlFor="terrain-landmarks">{t.landmarks}</label><input id="terrain-landmarks" type="checkbox" role="switch" checked={landmarks} onChange={(e) => setLandmarks(e.target.checked)} disabled={!ready} /></div>
        {threeD && <div className={s.threeDSettings}>
          <label htmlFor="terrain-exaggeration"><span>{t.exaggeration}</span><output>{exaggeration.toFixed(1)}×</output></label>
          <input id="terrain-exaggeration" type="range" min="1" max="2.5" step="0.1" value={exaggeration} onChange={(e) => { clearBuilding(); setExaggeration(Number(e.target.value)); }} />
          <label htmlFor="terrain-tilt"><span>{t.tilt}</span><output>{Math.round(camera.pitch)}°</output></label>
          <input id="terrain-tilt" type="range" min="25" max="75" step="1" value={Math.round(camera.pitch)} onChange={(e) => map.current?.jumpTo({ pitch: Number(e.target.value) })} />
          <button className={s.orbit} aria-pressed={orbit} onClick={() => setOrbit(!orbit)}><TerrainIcon name="rotate" size={15} />{orbit ? t.pause : t.orbit}</button>
        </div>}
        <p className={s.gestureHint}>{threeD ? t.hint3D : t.hint2D}</p>
        {threeD && buildings && <p className={s.buildingHint}>{camera.zoom < 14 ? t.zoomBuildings : t.inspectBuildings}</p>}
      </section>

      <div className={s.statusBar}><span className={s.statusDot + (ready ? " " + s.loaded : "")} />{failure ? t.unavailable : ready ? threeD ? t.map3D : t.map : t.loading}<span className={s.statusDivider} /><span>{camera.latitude.toFixed(4)}° N, {Math.abs(camera.longitude).toFixed(4)}° W</span>{threeD && elevation !== null && <><span className={s.statusDivider} /><span>≈ {elevation.toLocaleString(locale === "pt" ? "pt-PT" : "en-GB")} m {t.elevation}</span></>}</div>
      {threeD && <div className={s.perspectiveLabel}><span>PORTUGAL</span><strong>{t.closer}</strong></div>}
      {terrainWarning && !failure && <div className={s.terrainWarning} role="status">{t.warning}</div>}
      {(!ready || failure) && <div className={s.mapNotice} role="status"><TerrainIcon name="mountain" size={28} /><strong>{failure ? t.retryTitle : t.loadingTitle}</strong><p>{failure ? t.errors[failure] : t.loadingDetail}</p>{failure && <button onClick={() => { setFailure(null); setReady(false); setTerrainWarning(false); setThreeD(false); setOrbit(false); setAttempt(attempt + 1); }}>{t.reload}</button>}</div>}
      {toast && <div className={s.toast} role="status">{t[toast]}</div>}
    </main>
  );
}
