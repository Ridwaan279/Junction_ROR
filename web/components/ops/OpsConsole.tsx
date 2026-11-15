"use client";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Map as LibreMap } from "maplibre-gl";
import { loadMapLibre } from "@/lib/terrain";
import { loadOpsStyle, restorePortugalLayers, setThreeD } from "@/lib/ops-map";
import { deploymentsAt, missionAt } from "@/lib/drone-display";
import { useLiveStatus } from "@/lib/use-live-status";
import { MAX_TERRAIN_ZOOM, MAX_TERRAIN_PITCH, PLACES, placeCamera } from "@/lib/terrain-data";
import {
  addReplayLayers, compassPoint, FIRE, frameAt, lisbon, REPLAY_BASES, setReplayVisible, showAgentFleet, showReplayFrame, showVillages, villagesAt,
  showFleet, pinsAt, pinElement, rescueTargetsAt, PIN_COLORS, type ReplayFrame, type ReplayIndex, type ValidationRow,
} from "@/lib/replay";
import { distanceKm, droneTasks, FLEET, outerRing, planFleet, stagingPoint, type FleetPlan } from "@/lib/fleet";
import s from "./ops.module.css";
import SiteNav, { useLocale } from "./SiteNav";

type Locale = "pt" | "en";
type Mode = "live" | "replay";
type Accuracy = { method: string; observed_source: { name: string; url: string }[]; rows: ValidationRow[] };

const PORTUGAL = { center: [-8.05, 39.62] as [number, number], zoom: 6.35, pitch: 46, bearing: -8 };
const REPLAY_SECONDS = 75;
const phone = () => typeof window !== "undefined" && window.innerWidth <= 760;
/** Keep the subject in the free part of the screen, beside (desktop) or below (phone) the card. */
const framePadding = (mode: Mode = "replay") => mode === "live"
  ? (phone() ? { top: 120, bottom: 20, left: 0, right: 0 } : { top: 40, bottom: 20, left: 0, right: 0 })
  : (phone() ? { top: 300, bottom: 80, left: 10, right: 10 } : { top: 60, bottom: 90, left: 380, right: 40 });

const T = {
  pt: {
    live: "Ao vivo", replay: "Arganil 2025",
    noFires: "Sem incêndios ativos", activeFires: (n: number) => `${n} incêndio${n === 1 ? "" : "s"} ativo${n === 1 ? "" : "s"}`,
    checkFailed: "Não foi possível verificar", partial: "Verificação incompleta", checking: "A verificar…",
    checkedAt: "Portugal continental · verificado às", area: "Área", spread: "Avanço", wind: "Vento", evacuated: "Evacuadas",
    waiting: "A aguardar o primeiro alerta.", alert: "Alerta", drones: "drones", simulated: "simulado",
    sources: "Fontes", accuracy: "Precisão", dronesStat: "Drones", inAir: "no ar", launched: "drones lançados", legendPin: "Ocorrência",
    pinKinds: { event: "Evento", shelter: "Abrigo", road: "Estrada cortada", people: "Pessoas no local" }, source: "fonte",
    sarLine: (n: number) => `Context.dev: ${n} ${n === 1 ? "local com pessoas" : "locais com pessoas"} · enviar drone de busca e salvamento`,
    sarTitle: "Busca e salvamento", sarNote: "Locais com pessoas que o Context.dev encontrou nas notícias, por distância ao fogo. Sugestão: enviar um drone de busca e salvamento a cada um (simulação).", sarAction: "Enviar drone de busca e salvamento", peopleN: (n: number) => `${n} pessoas`, km: "km do fogo",
    fleetTitle: "Quantos drones", fleetNote: "Cálculo simples com dados já públicos no momento. Drones e despacho são simulados; constantes de demonstração.",
    fleetRows: (p: FleetPlan) => [
      [`Frente ativa ${dec(p.front_km, 1)} km ÷ ${dec(p.segment_km, 1)} km por drone (passagem a cada ${FLEET.revisit_min} min)`, p.perimeter],
      [`Vento ${Math.round(p.wind_kmh)} km/h: 1 batedor na cabeça + 1 por cada ${FLEET.head_wind_step_kmh} km/h`, p.head],
      [`${p.sites} locais e ocorrências a menos de ${FLEET.site_radius_km} km ÷ ${FLEET.sites_per_drone} por drone`, p.overwatch],
      ["No ar", p.airborne],
      [`Rotação: ${FLEET.endurance_min} min de voo, ${FLEET.recharge_min} min de recarga, ${Math.round(p.transit_min)} min de trânsito (÷ ${dec(p.duty, 2)}) e ${FLEET.reserve * 100} % de reserva`, p.total],
    ] as [string, number][],
    grounded: (g: number) => `Rajadas de ${g} km/h acima do limite de ${FLEET.gust_limit_kmh} km/h: drones em terra.`,
    agentTerms: { P_front: "Frente prevista a 1 h", t_transit: "Trânsito até ao local", airborne_perimeter: "Drones na frente", airborne_head: "Drones na cabeça do fogo", airborne_exposure: "Drones sobre locais expostos", duty: "Fração do tempo em voo", fleet: "Frota (com reserva de 20 %)" }, close: "Fechar", play: "Reproduzir", pause: "Pausa", timeline: "Linha temporal",
    legendArea: "Fogo", legendForecast: "Previsão 6 h", legendVillage: "Evacuada", legendDrone: "Drone",
    sourcesNote: "Cada facto aparece a partir da hora em que foi publicado.",
    accuracyTitle: "Previsão vs. realidade", observed: "Observado", predicted: "Previsto", overlap: "Sobreposição",
    method: "Cada previsão parte do perímetro observado por satélite (GWIS) e é comparada com o seguinte. 1 é perfeito.",
    growth: (g: string) => `Só no crescimento novo, a sobreposição mediana é ${g}. O modelo não inclui o combate.`,
    mapFailed: "O mapa não carregou.", map: "Mapa 3D de Portugal",
  },
  en: {
    live: "Live", replay: "Arganil 2025",
    noFires: "No active fires", activeFires: (n: number) => `${n} active fire${n === 1 ? "" : "s"}`,
    checkFailed: "Could not check", partial: "Check incomplete", checking: "Checking…",
    checkedAt: "Mainland Portugal · checked at", area: "Area", spread: "Spread", wind: "Wind", evacuated: "Evacuated",
    waiting: "Waiting for the first alert.", alert: "Alert", drones: "drones", simulated: "simulated",
    sources: "Sources", accuracy: "Accuracy", dronesStat: "Drones", inAir: "airborne", launched: "drones launched", legendPin: "Nearby event",
    pinKinds: { event: "Event", shelter: "Shelter", road: "Road closed", people: "People on site" }, source: "source",
    sarLine: (n: number) => `Context.dev: ${n} ${n === 1 ? "place with people" : "places with people"} · send a search-and-rescue drone`,
    sarTitle: "Search and rescue", sarNote: "Places with people that Context.dev found in the news, by distance from the fire. Suggestion: send a search-and-rescue drone to each (simulation).", sarAction: "Send search-and-rescue drone", peopleN: (n: number) => `${n} people`, km: "km from the fire",
    fleetTitle: "How many drones", fleetNote: "A simple calculation using only data public at the time. Drones and dispatch are simulated; demonstration constants.",
    fleetRows: (p: FleetPlan) => [
      [`Active front ${p.front_km.toFixed(1)} km ÷ ${p.segment_km.toFixed(1)} km per drone (each point seen every ${FLEET.revisit_min} min)`, p.perimeter],
      [`Wind ${Math.round(p.wind_kmh)} km/h: 1 scout at the head + 1 per ${FLEET.head_wind_step_kmh} km/h`, p.head],
      [`${p.sites} places and events within ${FLEET.site_radius_km} km ÷ ${FLEET.sites_per_drone} per drone`, p.overwatch],
      ["Airborne", p.airborne],
      [`Rotation: ${FLEET.endurance_min} min flight, ${FLEET.recharge_min} min recharge, ${Math.round(p.transit_min)} min transit (÷ ${p.duty.toFixed(2)}) and ${FLEET.reserve * 100} % reserve`, p.total],
    ] as [string, number][],
    grounded: (g: number) => `Gusts of ${g} km/h above the ${FLEET.gust_limit_kmh} km/h limit: drones grounded.`,
    agentTerms: { P_front: "Predicted 1 h front", t_transit: "Transit to the fire", airborne_perimeter: "Drones on the front", airborne_head: "Drones at the head", airborne_exposure: "Drones over exposed places", duty: "Share of time flying", fleet: "Fleet (with 20 % reserve)" }, close: "Close", play: "Play", pause: "Pause", timeline: "Timeline",
    legendArea: "Fire", legendForecast: "6 h forecast", legendVillage: "Evacuated", legendDrone: "Drone",
    sourcesNote: "Each fact appears from the time it was published.",
    accuracyTitle: "Forecast vs. reality", observed: "Observed", predicted: "Predicted", overlap: "Overlap",
    method: "Each forecast starts from the observed satellite perimeter (GWIS) and is scored against the next. 1 is perfect.",
    growth: (g: string) => `On new growth only, median overlap is ${g}. The model does not include firefighting.`,
    mapFailed: "The map did not load.", map: "3D map of Portugal",
  },
};

export default function OpsConsole() {
  const [locale, setLocale] = useLocale();
  const [mode, setMode] = useState<Mode>("live");
  const [threeD, set3D] = useState(true);
  const [ready, setReady] = useState(false);
  const [mapFailed, setMapFailed] = useState(false);
  const { data: live, error: liveError, loading: liveLoading } = useLiveStatus();
  const [eventsVisible, setEventsVisible] = useState(true);
  const [textures, setTextures] = useState(true);
  const [textureFailed, setTextureFailed] = useState(false);
  const materials = useRef<Awaited<ReturnType<typeof restorePortugalLayers>>>(null);
  const [index, setIndex] = useState<ReplayIndex | null>(null);
  const [frame, setFrame] = useState<ReplayFrame | null>(null);
  const [t, setT] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [panel, setPanel] = useState<"sources" | "accuracy" | "fleet" | "sar" | "deployments" | null>(null);
  const [accuracy, setAccuracy] = useState<Accuracy | null>(null);
  const container = useRef<HTMLDivElement>(null);
  const map = useRef<LibreMap | null>(null);
  const frames = useRef(new Map<string, ReplayFrame>());
  const lastT = useRef(0);
  const base = useRef<string>(REPLAY_BASES[0]);
  const L = T[locale];
  const [loaded, setLoaded] = useState<Record<string, ReplayFrame>>({});


  // Original Portugal terrain, buildings, varied roofs and ground textures.
  useEffect(() => {
    let cancelled = false, instance: LibreMap | null = null;
    const abort = new AbortController();
    (async () => {
      const [maplibre, style] = await Promise.all([loadMapLibre(), loadOpsStyle(abort.signal)]);
      if (cancelled || !container.current) return;
      instance = new maplibre.Map({
        container: container.current, style, ...PORTUGAL, zoom: PORTUGAL.zoom - (phone() ? 0.9 : 0),
        maxPitch: MAX_TERRAIN_PITCH, maxZoom: MAX_TERRAIN_ZOOM, attributionControl: false, fadeDuration: 150,
      });
      map.current = instance;
      instance.addControl(new maplibre.AttributionControl({ compact: true }), "bottom-left");
      instance.addControl(new maplibre.NavigationControl({ visualizePitch: true }), "bottom-right");
      instance.on("load", async () => {
        if (cancelled || !instance) return;
        instance.setPadding(framePadding("live"));
        try {
          materials.current = await restorePortugalLayers(instance, abort.signal, () => { if (!cancelled) setTextureFailed(true); });
        } catch { if (!cancelled) setTextureFailed(true); }
        if (cancelled || !instance) return;
        setThreeD(instance, true, false);
        addReplayLayers(instance);
        instance.addSource("ops-live", { type: "geojson", data: { type: "FeatureCollection", features: [] } });
        instance.addLayer({ id: "ops-live-fire", type: "circle", source: "ops-live", paint: {
          "circle-radius": 7, "circle-color": "#e94724", "circle-stroke-color": "#ffffff", "circle-stroke-width": 2,
        } });
        setReplayVisible(instance, false);
        instance.getContainer().querySelector(".maplibregl-ctrl-attrib")?.classList.remove("maplibregl-compact-show");
        setReady(true);
      });
      instance.on("error", (e) => {
        // Missing tiles must not blank the screen; only a style failure does.
        if (!(e as { sourceId?: string }).sourceId && !instance?.isStyleLoaded()) setMapFailed(true);
      });
    })().catch(() => { if (!cancelled) setMapFailed(true); });
    return () => { cancelled = true; abort.abort(); instance?.remove(); map.current = null; materials.current = null; };
  }, []);

  useEffect(() => {
    if (!ready) return;
    materials.current?.buildings.setDisplay(true, textures);
    materials.current?.ground.setDisplay(textures, textures);
  }, [textures, ready]);
  useEffect(() => {
    const m = map.current;
    if (!ready || !m) return;
    const features: GeoJSON.Feature[] = mode === "live" && !liveError ? (live?.active ?? []).flatMap((incident) =>
      incident.position ? [{ type: "Feature" as const, geometry: { type: "Point" as const, coordinates: incident.position }, properties: { name: incident.place } }] : []) : [];
    (m.getSource("ops-live") as import("maplibre-gl").GeoJSONSource | undefined)?.setData({ type: "FeatureCollection", features });
  }, [live, liveError, mode, ready]);

  const loadFrame = useCallback(async (file: string) => {
    const hit = frames.current.get(file);
    if (hit) return hit;
    const r = await fetch(base.current + file);
    if (!r.ok) throw new Error("frame");
    const f = (await r.json()) as ReplayFrame;
    frames.current.set(file, f);
    setLoaded((prev) => (prev[file] ? prev : { ...prev, [file]: f }));
    return f;
  }, []);

  const enterReplay = useCallback(async () => {
    setMode("replay");
    setPanel(null);
    let idx = index;
    if (!idx) {
      // Agent 2's generated replay first; the hand-built fixture if it is missing.
      for (const b of REPLAY_BASES) {
        const r = await fetch(b + "index.json").catch(() => null);
        if (r?.ok) { base.current = b; idx = (await r.json()) as ReplayIndex; break; }
      }
      if (!idx) return;
      setIndex(idx);
    }
    const start = Date.parse(idx.window.from);
    lastT.current = start;
    setT(start);
    setPlaying(true);
    idx.frames.forEach((f) => void loadFrame(f.file).catch(() => undefined));
  }, [index, loadFrame]);

  const enterLive = () => { setPlaying(false); setPanel(null); setMode("live"); };

  // Camera and layers follow the mode.
  useEffect(() => {
    const m = map.current;
    if (!ready || !m) return;
    setReplayVisible(m, mode === "replay");
    if (mode === "replay" && index) {
      m.flyTo({ center: index.ignition.position, zoom: phone() ? 10.9 : 11.6, padding: framePadding(),
        pitch: threeD ? 64 : 0, bearing: threeD ? -28 : 0, duration: 3200, essential: true });
    } else if (mode === "live") {
      m.flyTo({ ...PORTUGAL, zoom: PORTUGAL.zoom - (phone() ? 0.9 : 0), padding: framePadding("live"),
        pitch: threeD ? PORTUGAL.pitch : 0, bearing: threeD ? PORTUGAL.bearing : 0, duration: 2200, essential: true });
    }
  }, [mode, ready, index]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => { if (ready && map.current) setThreeD(map.current, threeD); }, [threeD, ready]);

  // The frame on screen is the latest one not after t.
  useEffect(() => {
    if (!index || mode !== "replay") return;
    const file = index.frames[frameAt(index, t)].file;
    let cancelled = false;
    void loadFrame(file).then((f) => { if (!cancelled) setFrame(f); }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [t, index, mode, loadFrame]);
  useEffect(() => {
    if (ready && map.current && frame && index) showReplayFrame(map.current, frame, index.ignition.position);
  }, [frame, ready, index]);

  // Playback.
  useEffect(() => {
    if (!playing || !index || mode !== "replay") return;
    const from = Date.parse(index.window.from), to = Date.parse(index.window.to);
    const rate = (to - from) / (REPLAY_SECONDS * 1000);
    let raf = 0, prev = performance.now();
    const step = (ts: number) => {
      const next = Math.min(to, lastT.current + (ts - prev) * rate);
      prev = ts;
      lastT.current = next;
      setT(next);
      if (next >= to) { setPlaying(false); return; }
      raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  }, [playing, index, mode]);

  // Everything below is filtered to the minute: nothing appears before it was public.
  const minute = Math.floor(t / 60_000);
  const cut = minute * 60_000;
  const known = useMemo(() => (index ? index.evidence
    .filter((e) => e.quality === "verified" && Date.parse(e.known_at) <= cut)
    .sort((a, b) => b.known_at.localeCompare(a.known_at)) : []), [index, cut]);
  const villages = useMemo(() => (index ? villagesAt(index, cut) : []), [index, cut]);
  const evacuated = villages.filter((v) => v.evacuated).length;
  const latest = useMemo(() => known.find((e) => ["evacuation", "shelter", "road_closure", "event_gathering", "ignition_cause", "threatened_place"].includes(e.category)), [known]);
  const steps = useMemo(() => (index?.automation ?? frame?.automation ?? []).filter((a) => Date.parse(a.at) <= cut), [index, frame, cut]);
  const officialStart = index?.timeline.find((x) => x.status === "Início")?.at;
  const alertAt = steps.find((a) => a.step === "reported")?.at ?? (officialStart && Date.parse(officialStart) <= cut ? officialStart : undefined);
  const pins = useMemo(() => (index ? pinsAt(index, cut) : []), [index, cut]);
  const targets = useMemo(() => (index ? rescueTargetsAt(index, cut) : []), [index, cut]);
  useEffect(() => { if (ready && map.current) showVillages(map.current, villages); }, [villages, ready]);

  // Drone fleet: sized from the front, the wind and nearby places, all as known at the minute on screen.
  const sitesAt = useCallback((at: number) => (index ? villagesAt(index, at).filter((v) => distanceKm(v.position, index.ignition.position) <= FLEET.site_radius_km) : []), [index]);
  const planFor = useCallback((f: ReplayFrame, at: number) => {
    const w = f.weather;
    return planFleet(f.model.front_km, w ? w.wind_mps * 3.6 : 0, w?.gust_mps != null ? w.gust_mps * 3.6 : null, sitesAt(at).length);
  }, [sitesAt]);
  const alertMs = alertAt ? Date.parse(alertAt) : null;
  // Drones needed in the air at every moment the inputs change (alert, new frame, new report), computed once.
  const schedule = useMemo(() => {
    if (!index || alertMs == null) return [] as [number, number][];
    return [...new Set([alertMs, ...index.frames.map((f) => Date.parse(f.t)), ...index.evidence.map((e) => Date.parse(e.known_at))])]
      .filter((m) => m >= alertMs).sort((x, y) => x - y)
      .flatMap((m) => { const f = loaded[index.frames[frameAt(index, m)].file]; return f ? [[m, planFor(f, m).airborne] as [number, number]] : []; });
  }, [index, alertMs, loaded, planFor]);
  const plan = useMemo(() => (frame && alertMs != null && cut >= alertMs ? planFor(frame, cut) : null), [frame, alertMs, cut, planFor]);
  const fleetView = useMemo(() => {
    if (!index || !frame || !plan || alertMs == null || plan.grounded) return null;
    const alertFrame = loaded[index.frames[frameAt(index, alertMs)].file] ?? frame;
    const base = stagingPoint(index.ignition.position, alertFrame.weather?.wind_from_deg ?? 0);
    const tasks = droneTasks(plan, index.ignition.position, outerRing(frame.predicted.p50), outerRing(frame.forecast.h6), sitesAt(cut).map((v) => v.position));
    // Slot i launches the first moment the plan needed more than i drones in the air.
    const launch: number[] = [];
    for (const [m, need] of schedule) {
      if (m > cut) break;
      while (launch.length < need) launch.push(m);
    }
    while (launch.length < tasks.length) launch.push(cut);
    const drones = tasks.map((task, i) => {
      const transitMs = (distanceKm(base, task.target) / FLEET.cruise_kmh) * 3_600_000;
      const k = Math.max(0, Math.min(1, (cut - launch[i]) / Math.max(60_000, transitMs)));
      return { ...task, position: [base[0] + (task.target[0] - base[0]) * k, base[1] + (task.target[1] - base[1]) * k] as [number, number] };
    });
    return { base, drones };
  }, [index, frame, plan, alertMs, cut, sitesAt, schedule, loaded]);
  // Agent 2's fleet (stations, terrain-following routes) is the source when the generated replay has it.
  const agent = frame?.fleet && alertMs != null && cut >= alertMs ? frame.fleet : null;
  const deployments = useMemo(() => index ? deploymentsAt(index, loaded, cut) : [], [index, loaded, cut]);
  const launches = useMemo(() => new Map(deployments.map((d) => [d.drone.id, d.at])), [deployments]);
  const agentAirborne = deployments.filter((d) => missionAt(d.drone, d.at, cut)?.phase === "flight").length;
  const firstLaunch = deployments[0]?.at ?? null;
  useEffect(() => {
    if (!ready || !map.current) return;
    if (agent && index) showAgentFleet(map.current, index.stations ?? [], deployments.map((d) => d.drone), launches, cut);
    else showFleet(map.current, fleetView?.base ?? null, fleetView?.drones ?? []);
  }, [agent, index, deployments, launches, cut, fleetView, ready]);

  // Pins: DOM markers (visible from any 3D angle) with a small card of what was reported there.
  const markers = useRef(new Map<string, import("maplibre-gl").Marker>());
  const pinPopups = useRef(new Map<string, import("maplibre-gl").Popup>());
  useEffect(() => {
    const m = map.current;
    if (!ready || !m) return;
    let cancelled = false;
    void loadMapLibre().then(({ Marker, Popup }) => {
      if (cancelled) return;
      const want = mode === "replay" && eventsVisible ? pins : [];
      const keep = new Set(want.map((p) => p.id + ":" + p.items.length + ":" + locale));
      for (const [key, marker] of markers.current) if (!keep.has(key)) { marker.remove(); markers.current.delete(key); pinPopups.current.get(key)?.remove(); pinPopups.current.delete(key); }
      for (const pin of want) {
        const key = pin.id + ":" + pin.items.length + ":" + locale;
        if (markers.current.has(key)) continue;
        const Lx = T[locale];
        const card = document.createElement("div");
        const h = document.createElement("strong");
        h.textContent = `${Lx.pinKinds[pin.kind]} · ${pin.place}`;
        card.append(h);
        for (const item of pin.items.slice(0, 3)) {
          const p = document.createElement("p");
          p.textContent = locale === "pt" ? item.quote : item.statement_en;
          const a = document.createElement("a");
          a.href = item.url; a.target = "_blank"; a.rel = "noreferrer";
          a.textContent = `${lisbon(item.known_at, locale, true)} · ${item.publisher}`;
          card.append(p, a);
        }
        const popup = new Popup({ closeButton: false, maxWidth: "280px", offset: 22, className: "ops-popup" }).setDOMContent(card);
        const el = pinElement(pin.kind, h.textContent);
        const marker = new Marker({ element: el, anchor: "bottom", opacityWhenCovered: "1" }).setLngLat(pin.position).addTo(m);
        // The map must not see pin presses, or its own click handling closes the card at once.
        for (const type of ["mousedown", "mouseup", "pointerdown", "pointerup", "touchstart", "touchend", "dblclick"]) el.addEventListener(type, (e) => e.stopPropagation());
        el.addEventListener("click", (e) => {
          e.stopPropagation();
          if (popup.isOpen()) popup.remove(); else popup.setLngLat(pin.position).addTo(m);
        });
        markers.current.set(key, marker); pinPopups.current.set(key, popup);
      }
    });
    return () => { cancelled = true; };
  }, [pins, ready, mode, locale, eventsVisible]);
  useEffect(() => () => { for (const marker of markers.current.values()) marker.remove(); markers.current.clear(); for (const popup of pinPopups.current.values()) popup.remove(); pinPopups.current.clear(); }, []);

  const openAccuracy = useCallback(async () => {
    setPanel("accuracy");
    if (!accuracy) {
      const r = await fetch("/fixtures/v2/arganil-validation.json");
      if (r.ok) setAccuracy(await r.json());
    }
  }, [accuracy]);

  const togglePlay = () => {
    if (!index) return;
    if (!playing && t >= Date.parse(index.window.to)) { lastT.current = Date.parse(index.window.from); setT(lastT.current); }
    setPlaying(!playing);
  };

  return (
    <main className={s.shell} data-panel={panel ? "open" : undefined}>
      <div ref={container} className={s.map} aria-label={L.map} />

      <SiteNav active="map" locale={locale} />

      <nav className={s.controls} aria-label="EmberWatch">
        <div className={s.segmented} role="group">
          <button aria-pressed={mode === "live"} onClick={enterLive}>{L.live}</button>
          <button aria-pressed={mode === "replay"} onClick={() => void enterReplay()}>{L.replay}</button>
        </div>
        <button className={s.toggle} onClick={() => set3D(!threeD)} aria-label={threeD ? "2D" : "3D"}>{threeD ? "2D" : "3D"}</button>
        <button className={s.toggle} onClick={() => setLocale(locale === "pt" ? "en" : "pt")} lang={locale === "pt" ? "en" : "pt"}>{locale === "pt" ? "EN" : "PT"}</button>
      </nav>

      <div className={s.mapTools} role="group" aria-label={locale === "pt" ? "Camadas do mapa" : "Map layers"}>
        <div className={s.mapToolRow}>
          <button aria-pressed={textures} onClick={() => setTextures(!textures)}>{locale === "pt" ? "Texturas" : "Textures"} <span>{textures ? "ON" : "OFF"}</span></button>
          <button aria-pressed={eventsVisible} onClick={() => setEventsVisible(!eventsVisible)}>{locale === "pt" ? "Eventos" : "Events"} <span>{eventsVisible ? "ON" : "OFF"}</span></button>
        </div>
        <label className={s.placePicker}>{locale === "pt" ? "Explorar" : "Explore"}
          <select disabled={!ready} aria-label={locale === "pt" ? "Explorar Portugal" : "Explore Portugal"} defaultValue="" onChange={(e) => {
            const place = PLACES.find((p) => p.id === e.target.value);
            if (place) { set3D(true); map.current?.flyTo({ ...placeCamera(place, true), padding: framePadding("live"), duration: 1800 }); }
            else map.current?.flyTo({ ...PORTUGAL, padding: framePadding("live"), duration: 1800 });
          }}>
            <option value="">Portugal</option><option value="lisboa">Lisboa · 3D</option><option value="porto">Porto · 3D</option><option value="estrela">Serra da Estrela</option>
          </select>
        </label>
        <p>{locale === "pt" ? "Aproxime-se para ver os edifícios e telhados." : "Zoom in to see the buildings and roofs."}</p>
        {textureFailed && <p role="status">{locale === "pt" ? "Algumas texturas indisponíveis. Geometria mantida." : "Some textures unavailable. Building geometry remains visible."}</p>}
      </div>

      {mode === "live" && (
        <section className={`${s.card} ${s.compact}`} aria-live="polite">
          {liveLoading && live === null ? (
            <p className={s.secondary}>{L.checking}</p>
          ) : liveError || !live || live.state === "check_failed" || live.state === "partial" ? (
            <h1 className={s.title}><i className={s.dotWarn} />{live?.state === "partial" ? L.partial : L.checkFailed}</h1>
          ) : (
            <>
              <h1 className={s.title}>
                <i className={live.state === "no_active_fires" ? s.dotOk : s.dotFire} />
                {live.state === "no_active_fires" ? L.noFires : L.activeFires(live.active.length)}
              </h1>
              <p className={s.secondary}>
                {L.checkedAt} {lisbon(live.checked_at, locale, true)} ·{" "}
                {live.sources.map((src) => <a key={src.url} href={src.url} target="_blank" rel="noreferrer">Fogos.pt</a>)}
              </p>
            </>
          )}
          <a className={s.liveDataLink} href="/data">{locale === "pt" ? "Ver dados ao vivo" : "View live data"} →</a>
        </section>
      )}

      {mode === "replay" && index && frame && (
        <section className={s.card}>
          <div className={s.head}>
            <h1 className={s.title}>Piódão, Arganil</h1>
            {frame.fogos_status && alertAt && <span className={s.tertiary}>{frame.fogos_status.status}</span>}
          </div>
          <p className={s.secondary}>{lisbon(new Date(t).toISOString(), locale, true)}</p>

          {!alertAt ? (
            <p className={s.waiting}>{L.waiting}</p>
          ) : (
            <>
              <dl className={s.stats}>
                <div><dt>{L.area}</dt><dd>{fmt(frame.model.area_ha_p50, locale)}<small>ha</small></dd></div>
                {frame.weather && <div><dt>{L.wind}</dt><dd>{Math.round(frame.weather.wind_mps * 3.6)}<small>km/h {compassPoint(frame.weather.wind_from_deg, locale)}</small></dd></div>}
                <div><dt>{L.evacuated}</dt><dd>{evacuated}</dd></div>
                {(agent || plan) && (
                  <div>
                    <dt>{L.dronesStat}</dt>
                    <dd><button className={s.statButton} onClick={() => setPanel(panel === "fleet" ? null : "fleet")}>{agent ? agent.count : plan!.total}</button><small>{agent ? agentAirborne : plan!.airborne} {L.inAir}</small></dd>
                  </div>
                )}
              </dl>
              <p className={s.tertiary}>
                {L.alert} {lisbon(alertAt, locale)}
                {agent && firstLaunch != null && <> · {locale === "pt" ? "Primeiro despacho" : "First deployment"} {lisbon(new Date(firstLaunch).toISOString(), locale)} ({L.simulated})</>}
                {!agent && plan && !plan.grounded && <> · {L.launched} {lisbon(alertAt, locale)} ({L.simulated})</>}
              </p>
              {deployments.length > 0 && <div className={s.deploymentPreview}>
                <button className={s.deploymentHeading} onClick={() => setPanel(panel === "deployments" ? null : "deployments")}>
                  {locale === "pt" ? "Despachos de drones" : "Drone deployments"} <span>{deployments.length} · {L.simulated} ↗</span>
                </button>
                <ol>{[...deployments].reverse().slice(0, 3).map((d) => <li key={d.drone.id}>
                  <time dateTime={new Date(d.at).toISOString()}>{lisbon(new Date(d.at).toISOString(), locale)}</time>
                  <span>{d.drone.id.replace("demo-osm-", "")} <small>{d.phase === "ground_transfer" ? (locale === "pt" ? "transporte terrestre" : "ground transfer") : (locale === "pt" ? "voo" : "flight")}</small></span>
                </li>)}</ol>
              </div>}
              {targets.length > 0 && (
                <button className={s.sarLine} onClick={() => setPanel(panel === "sar" ? null : "sar")}>{L.sarLine(targets.length)} →</button>
              )}
              {latest && (
                <a className={s.latest} href={latest.url} target="_blank" rel="noreferrer">
                  <span>{lisbon(latest.known_at, locale)} · {latest.publisher}</span>
                  {locale === "pt" ? latest.quote : latest.statement_en}
                </a>
              )}
            </>
          )}

          <div className={s.foot}>
            <ul className={s.legend} aria-label="Legenda">
              <li><i style={{ background: FIRE.p50 }} />{L.legendArea}</li>
              <li><i className={s.dash} />{L.legendForecast}</li>
              <li><i className={s.round} style={{ background: "#ff453a" }} />{L.legendVillage}</li>
              <li><i className={s.round} style={{ background: FIRE.drone }} />{L.legendDrone}</li>
              <li><i className={s.pin} style={{ background: PIN_COLORS.event }} />{L.legendPin}</li>
            </ul>
            <div className={s.links}>
              <button onClick={() => setPanel(panel === "sources" ? null : "sources")}>{L.sources}</button>
              <button onClick={() => void (panel === "accuracy" ? setPanel(null) : openAccuracy())}>{L.accuracy}</button>
            </div>
          </div>
        </section>
      )}

      {mode === "replay" && panel === "deployments" && <aside className={s.panel} aria-label={locale === "pt" ? "Despachos de drones" : "Drone deployments"}>
        <PanelHead title={locale === "pt" ? "Despachos de drones" : "Drone deployments"}
          note={locale === "pt" ? "Horas de despacho da simulação, no fuso de Lisboa. As reservas em terra não contam como despachadas." : "Simulation deployment times in Lisbon time. Ground reserves are not counted as deployed."}
          close={L.close} onClose={() => setPanel(null)} />
        <ol className={s.deploymentList}>{[...deployments].reverse().map((d) => <li key={d.drone.id}>
          <time dateTime={new Date(d.at).toISOString()}>{lisbon(new Date(d.at).toISOString(), locale, true)}</time>
          <strong>{d.drone.id}</strong><span>{d.station?.name ?? d.drone.station_id}</span>
          <small>{d.drone.role} · {d.phase === "ground_transfer" ? (locale === "pt" ? "transporte terrestre" : "ground transfer") : (locale === "pt" ? "voo" : "flight")} · {L.simulated}</small>
        </li>)}</ol>
      </aside>}

      {mode === "replay" && panel === "sources" && (
        <aside className={s.panel} aria-label={L.sources}>
          <PanelHead title={L.sources} note={L.sourcesNote} close={L.close} onClose={() => setPanel(null)} />
          <ol className={s.list}>
            {known.slice(0, 80).map((e) => (
              <li key={e.id}>
                <a href={e.url} target="_blank" rel="noreferrer">
                  <span>{lisbon(e.known_at, locale, true)} · {e.publisher}</span>
                  {locale === "pt" ? e.quote : e.statement_en}
                </a>
              </li>
            ))}
          </ol>
        </aside>
      )}

      {mode === "replay" && panel === "sar" && (
        <aside className={s.panel} aria-label={L.sarTitle}>
          <PanelHead title={L.sarTitle} note={L.sarNote} close={L.close} onClose={() => setPanel(null)} />
          <ol className={s.list}>
            {targets.map((tg) => (
              <li key={tg.id}>
                <div className={s.target}>
                  <button className={s.targetHead} onClick={() => map.current?.flyTo({ center: tg.position, zoom: 13, duration: 1600, essential: true })}>
                    <i style={{ background: PIN_COLORS[tg.kind] }} />
                    <strong>{L.pinKinds[tg.kind]} · {tg.place}</strong>
                    <span>{tg.distance_km.toFixed(1).replace(".", locale === "pt" ? "," : ".")} {L.km}{tg.people != null ? ` · ${L.peopleN(tg.people)}` : ""}</span>
                  </button>
                  <p>{locale === "pt" ? tg.items[0].quote : tg.items[0].statement_en}</p>
                  <a href={tg.items[0].url} target="_blank" rel="noreferrer">{lisbon(tg.items[0].known_at, locale, true)} · {tg.items[0].publisher}</a>
                  <em className={s.sarAction}>→ {L.sarAction}</em>
                </div>
              </li>
            ))}
          </ol>
        </aside>
      )}

      {mode === "replay" && panel === "fleet" && agent && (
        <aside className={s.panel} aria-label={L.fleetTitle}>
          <PanelHead title={L.fleetTitle} note={(locale === "pt" ? agent.change_reason_pt : agent.change_reason_en) ?? L.fleetNote} close={L.close} onClose={() => setPanel(null)} />
          <div className={s.table}>
            <table>
              <tbody>
                {agent.breakdown.filter((b) => b.term in L.agentTerms).map((b) => (
                  <tr key={b.term} className={b.term === "fleet" ? s.total : undefined}>
                    <td>{L.agentTerms[b.term as keyof typeof L.agentTerms]}<br /><span className={s.basis}>{b.basis}</span></td>
                    <td>{termValue(b, locale)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className={s.tertiary}>{L.fleetNote}</p>
          </div>
        </aside>
      )}

      {mode === "replay" && panel === "fleet" && !agent && plan && (
        <aside className={s.panel} aria-label={L.fleetTitle}>
          <PanelHead title={L.fleetTitle} note={L.fleetNote} close={L.close} onClose={() => setPanel(null)} />
          <div className={s.table}>
            <table>
              <tbody>
                {L.fleetRows(plan).map(([label, value], i) => (
                  <tr key={i} className={i === 3 || i === 4 ? s.total : undefined}><td>{label}</td><td>{value}</td></tr>
                ))}
              </tbody>
            </table>
            {plan.grounded && plan.gust_kmh != null && <p className={s.tertiary}>{L.grounded(Math.round(plan.gust_kmh))}</p>}
          </div>
        </aside>
      )}

      {mode === "replay" && panel === "accuracy" && (
        <aside className={s.panel} aria-label={L.accuracyTitle}>
          <PanelHead title={L.accuracyTitle} note={L.method} close={L.close} onClose={() => setPanel(null)} />
          {accuracy && (
            <div className={s.table}>
              <table>
                <thead><tr><th /><th>{L.observed}</th><th>{L.predicted}</th><th>{L.overlap}</th></tr></thead>
                <tbody>
                  {accuracy.rows.map((r) => (
                    <tr key={r.cycle}>
                      <td>{lisbon(r.to, locale, true)}</td>
                      <td>{fmt(r.observed_area_ha, locale)}</td>
                      <td>{fmt(r.predicted_area_ha_p50, locale)}</td>
                      <td>{r.dice_total.toFixed(2)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <p className={s.tertiary}>
                {L.growth(median(accuracy.rows.map((r) => r.dice_growth)).toFixed(2))}{" "}
                {accuracy.observed_source[0] && <a href={accuracy.observed_source[0].url} target="_blank" rel="noreferrer">GWIS / GDACS</a>}
              </p>
            </div>
          )}
        </aside>
      )}

      {mode === "replay" && index && (
        <footer className={s.timeline}>
          <button className={s.play} onClick={togglePlay} aria-label={playing ? L.pause : L.play}>
            {playing
              ? <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden><path fill="currentColor" d="M7 5h3.5v14H7zM13.5 5H17v14h-3.5z" /></svg>
              : <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden><path fill="currentColor" d="M8 5.5v13l10.5-6.5Z" /></svg>}
          </button>
          <input type="range" aria-label={L.timeline}
            min={Date.parse(index.window.from)} max={Date.parse(index.window.to)} step={60_000} value={t}
            style={{ "--p": pct(t, index) + "%" } as React.CSSProperties}
            onChange={(e) => { setPlaying(false); lastT.current = Number(e.target.value); setT(lastT.current); }} />
        </footer>
      )}

      {mapFailed && <p className={s.failed} role="alert">{L.mapFailed}</p>}
    </main>
  );
}

function PanelHead({ title, note, close, onClose }: { title: string; note: string; close: string; onClose: () => void }) {
  return (
    <div className={s.panelHead}>
      <div><h2>{title}</h2><p>{note}</p></div>
      <button onClick={onClose} aria-label={close}>
        <svg viewBox="0 0 24 24" width="12" height="12" aria-hidden><path stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" d="M6 6l12 12M18 6 6 18" /></svg>
      </button>
    </div>
  );
}

const termValue = (b: { term: string; value: number; unit: string }, locale: Locale) => {
  const n = (x: number, d: number) => (locale === "pt" ? x.toFixed(d).replace(".", ",") : x.toFixed(d));
  if (b.unit === "m") return `${n(b.value / 1000, 1)} km`;
  if (b.unit === "s") return `${Math.round(b.value / 60)} min`;
  if (b.unit === "fraction") return n(b.value, 2);
  return String(Math.round(b.value));
};
const dec = (n: number, d: number) => n.toFixed(d).replace(".", ",");
const median = (xs: number[]) => { const v = [...xs].sort((a, b) => a - b), m = v.length >> 1; return v.length % 2 ? v[m] : (v[m - 1] + v[m]) / 2; };
const fmt = (n: number, locale: Locale) => new Intl.NumberFormat(locale === "pt" ? "pt-PT" : "en-GB", { maximumFractionDigits: 0 }).format(n);
const pct = (t: number, index: ReplayIndex) => {
  const a = Date.parse(index.window.from), b = Date.parse(index.window.to);
  return Math.max(0, Math.min(100, ((t - a) / (b - a)) * 100));
};
