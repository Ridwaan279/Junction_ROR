"use client";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Map as LibreMap, IControl } from "maplibre-gl";
import type { MapboxOverlay } from "@deck.gl/mapbox";
import type { Locale } from "@/lib/i18n";
import { compass, arrowPaths } from "@/lib/firecast";
import { safeUrl } from "@/lib/model";
import { addTerrainLayers, loadMapLibre, loadTerrainStyle } from "@/lib/terrain";
import Icon from "./Icon";

type Geometry = GeoJSON.Geometry | null;
interface Frame {
  t: string;
  cycle: number;
  observed_index: number | null;
  predicted: { p10: Geometry; p50: Geometry; p90: Geometry };
  forecast: { h6?: Geometry; h12?: Geometry };
  smoke: { threshold_ug_m3: number; geometry: GeoJSON.Geometry }[];
  smoke_axis?: GeoJSON.LineString;
  plume?: { stability: string; rise_m: number; heat_mw: number; pm25_t_h: number };
  wind: { path: [number, number][]; speed_mps: number }[];
  weather?: { temp_c: number; rh_pct: number; wind_mps: number; wind_from_deg: number; gust_mps: number | null; wind_100m_mps: number | null; wind_100m_from_deg: number | null };
  model: { area_ha_p10: number; area_ha_p50: number; area_ha_p90: number; front_km: number; growth_ha_h: number; head_ros_m_min: number; head_intensity_kw_m: number; fire_type: string };
  drones: { count: number; bases: [number, number][]; routes: { path: [number, number][]; base: number; role: string; eta_min: number }[]; held?: Record<string, number> };
}
interface Perimeter { time: string; area_ha: number; source: string; url: string; geometry: GeoJSON.Geometry }
interface HistEvent { time: string; time_basis: string; kind: string; title: string; detail: string; source: string; url: string; facts: Record<string, unknown> | null; origin: string }
interface Validation {
  cycle: number; from: string; to: string; hours: number; started_from: string;
  observed_area_ha: number; observed_growth_ha: number;
  predicted_area_ha_p10: number; predicted_area_ha_p50: number; predicted_area_ha_p90: number;
  dice_total: number | null; dice_growth: number | null; growth_within_10pct_band: number | null;
  observed_growth_bearing_deg: number | null; predicted_growth_bearing_deg: number | null;
}
interface Replay {
  meta: {
    id: string; name: string; description: string; glide: string | null;
    ignition: { time: string; position: [number, number]; basis: string };
    detection_time: string; resolved_time: string | null;
    final_area: { area_ha: number; source: string; url: string }[];
    domain: { centre: [number, number]; cell_m: number; size: number };
    vegetation: { note: string };
    weather_source: { name: string; url: string; licence: string | null };
    elevation_source: { name: string; url: string };
    sources: { name: string; url: string }[];
    members: number; frame_step_h: number; model: string; method: string; fleet_rule: string;
  };
  perimeters: Perimeter[];
  events: HistEvent[];
  validation: Validation[];
  frames: Frame[];
}

const L = {
  pt: {
    title: "Replay histórico",
    loading: "A carregar o conjunto de dados histórico…",
    missing: "Conjunto de dados histórico indisponível. Gere-o com os exemplos build_dataset e replay.",
    observed: "Observado (GWIS)",
    modelled: "Modelo agora (10/50/90 %)",
    forecast: "Previsão +6 h / +12 h",
    smoke: "Fumo PM2.5",
    drones: "Drones simulados",
    day: "Dia",
    since: "desde a ignição",
    observedArea: "Área observada",
    modelArea: "Área modelada (mediana)",
    fleet: "Drones em missão",
    routed: "em rota",
    held: "retidos",
    head: "Cabeça do incêndio",
    wind: "Vento 10 m / 100 m",
    tempRh: "Temp. / HR",
    plume: "Coluna de fumo",
    chart: "Área ardida: modelo vs. observação",
    fleetChart: "Frota de drones",
    validation: "Validação contra perímetros observados",
    from: "Partida",
    horizon: "Horizonte",
    dice: "Dice total / crescimento",
    band: "Crescimento na banda 10 %",
    events: "Relatos da web (o que o Context.dev recolheria)",
    method: "Método e fontes",
    noObs: "Ainda sem perímetro observado",
    play: "Reproduzir",
    pause: "Pausa",
    back: "Voltar ao painel",
    ignitionPoint: "ponto de ignição",
    observedPerimeter: "perímetro observado",
    synthetic: "Frota e rotas simuladas; não operacional.",
  },
  en: {
    title: "Historical replay",
    loading: "Loading historical dataset…",
    missing: "Historical dataset unavailable. Generate it with the build_dataset and replay examples.",
    observed: "Observed (GWIS)",
    modelled: "Model now (10/50/90 %)",
    forecast: "Forecast +6 h / +12 h",
    smoke: "PM2.5 smoke",
    drones: "Simulated drones",
    day: "Day",
    since: "since ignition",
    observedArea: "Observed area",
    modelArea: "Modelled area (median)",
    fleet: "Drones on mission",
    routed: "routed",
    held: "held",
    head: "Head fire",
    wind: "Wind 10 m / 100 m",
    tempRh: "Temp. / RH",
    plume: "Smoke plume",
    chart: "Burned area: model vs observation",
    fleetChart: "Drone fleet",
    validation: "Validation against observed perimeters",
    from: "Start",
    horizon: "Horizon",
    dice: "Dice total / growth",
    band: "Growth in 10 % band",
    events: "Web reports (what Context.dev would capture)",
    method: "Method and sources",
    noObs: "No observed perimeter yet",
    play: "Play",
    pause: "Pause",
    back: "Back to dashboard",
    ignitionPoint: "ignition point",
    observedPerimeter: "observed perimeter",
    synthetic: "Fleet and routes are simulated; not operational.",
  },
};

const fmtTime = (iso: string, locale: Locale) =>
  new Intl.DateTimeFormat(locale === "pt" ? "pt-PT" : "en-GB", {
    timeZone: "Europe/Lisbon",
    day: "2-digit",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(iso));
const n0 = (v: number | null | undefined, unit = "") =>
  v === null || v === undefined || !Number.isFinite(v) ? "—" : Math.round(v).toLocaleString("pt-PT") + unit;
const feature = (geometry: Geometry, properties: Record<string, unknown> = {}): GeoJSON.Feature[] =>
  geometry ? [{ type: "Feature", geometry, properties }] : [];

export default function HistoricalView({ locale, onExit }: { locale: Locale; onExit: () => void }) {
  const l = L[locale];
  const [data, setData] = useState<Replay | null>(null);
  const [failed, setFailed] = useState(false);
  const [index, setIndex] = useState(0);
  const [playing, setPlaying] = useState(false);
  const container = useRef<HTMLDivElement>(null),
    map = useRef<LibreMap | null>(null),
    overlay = useRef<MapboxOverlay | null>(null);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    let cancelled = false;
    fetch("/historical/arganil-piodao-2025.json")
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error("missing"))))
      .then((json: Replay) => {
        if (!cancelled) setData(json);
      })
      .catch(() => !cancelled && setFailed(true));
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!playing || !data) return;
    const timer = setInterval(() => {
      setIndex((i) => {
        if (i + 1 >= data.frames.length) {
          setPlaying(false);
          return i;
        }
        return i + 1;
      });
    }, 900);
    return () => clearInterval(timer);
  }, [playing, data]);

  useEffect(() => {
    if (!data) return;
    let cancelled = false,
      instance: LibreMap | null = null;
    (async () => {
      const [maplibre, { MapboxOverlay }, terrainStyle] = await Promise.all([loadMapLibre(), import("@deck.gl/mapbox"), loadTerrainStyle()]);
      if (cancelled || !container.current) return;
      instance = new maplibre.Map({
        container: container.current,
        style: terrainStyle,
        center: data.meta.domain.centre,
        zoom: 9.6,
        attributionControl: { compact: true },
      });
      map.current = instance;
      instance.addControl(new maplibre.NavigationControl({ showCompass: false }), "top-right");
      instance.on("load", async () => {
        if (cancelled || !instance) return;
        try {
          await addTerrainLayers(instance);
        } catch {
          // Relief and contours are optional; the replay stays usable without them.
          console.warn("Terrain detail could not be initialized");
        }
        if (cancelled || !instance) return;
        const deck = new MapboxOverlay({ interleaved: false, layers: [] });
        overlay.current = deck;
        instance.addControl(deck as unknown as IControl);
        // Frame the final observed burned area (plus the ignition point).
        const coords: number[][] = [data.meta.ignition.position];
        const last = data.perimeters[data.perimeters.length - 1]?.geometry;
        const walk = (c: unknown): void => {
          if (Array.isArray(c) && typeof c[0] === "number") coords.push(c as number[]);
          else if (Array.isArray(c)) c.forEach(walk);
        };
        if (last && "coordinates" in last) walk(last.coordinates);
        const lons = coords.map((c) => c[0]), lats = coords.map((c) => c[1]);
        instance.fitBounds(
          [[Math.min(...lons), Math.min(...lats)], [Math.max(...lons), Math.max(...lats)]],
          { padding: { top: 150, bottom: 60, left: 60, right: 60 }, duration: 0 },
        );
        setReady(true);
      });
    })().catch((error) => {
      // Basemap style unavailable: timeline, charts and validation still render.
      if (!cancelled) console.warn("Replay basemap unavailable", error);
    });
    return () => {
      cancelled = true;
      overlay.current?.finalize();
      overlay.current = null;
      instance?.remove();
      map.current = null;
    };
  }, [data]);

  const frame = data?.frames[Math.min(index, (data?.frames.length ?? 1) - 1)];
  const observed = frame && frame.observed_index !== null && data ? data.perimeters[frame.observed_index] : null;

  useEffect(() => {
    if (!ready || !overlay.current || !frame || !data) return;
    let cancelled = false;
    (async () => {
      const { GeoJsonLayer, PathLayer, ScatterplotLayer } = await import("@deck.gl/layers");
      if (cancelled || !overlay.current) return;
      const fc = (features: GeoJSON.Feature[]): GeoJSON.FeatureCollection => ({ type: "FeatureCollection", features });
      const smoke = [...frame.smoke].sort((a, b) => a.threshold_ug_m3 - b.threshold_ug_m3);
      const arrows = frame.wind.flatMap((w) => arrowPaths(w.path).map((path) => ({ path })));
      const targets = frame.drones.routes.map((r) => r.path[r.path.length - 1]).filter(Boolean);
      overlay.current.setProps({
        layers: [
          new GeoJsonLayer({
            id: "h-smoke-" + index,
            data: fc(smoke.flatMap((s) => feature(s.geometry, { th: s.threshold_ug_m3 }))),
            filled: true,
            stroked: false,
            getFillColor: (f: GeoJSON.Feature) => {
              const th = Number(f.properties?.th);
              return [96, 90, 122, th >= 500 ? 140 : th >= 150 ? 95 : 50];
            },
          }),
          new GeoJsonLayer({
            id: "h-observed-fill-" + index,
            data: fc(feature(observed?.geometry ?? null)),
            filled: true,
            stroked: false,
            getFillColor: [40, 24, 24, 110],
          }),
          new GeoJsonLayer({
            id: "h-p10-" + index,
            data: fc(feature(frame.predicted.p10)),
            filled: true,
            stroked: false,
            getFillColor: [246, 128, 32, 45],
          }),
          new GeoJsonLayer({
            id: "h-p50-" + index,
            data: fc(feature(frame.predicted.p50)),
            filled: true,
            stroked: true,
            getFillColor: [222, 45, 25, 85],
            getLineColor: [255, 92, 52, 240],
            lineWidthMinPixels: 2,
          }),
          new GeoJsonLayer({
            id: "h-p90-" + index,
            data: fc(feature(frame.predicted.p90)),
            filled: true,
            stroked: false,
            getFillColor: [150, 18, 18, 90],
          }),
          new GeoJsonLayer({
            id: "h-forecast-" + index,
            data: fc([...feature(frame.forecast.h12 ?? null, { h: 12 }), ...feature(frame.forecast.h6 ?? null, { h: 6 })]),
            filled: false,
            stroked: true,
            getLineColor: (f: GeoJSON.Feature) => (Number(f.properties?.h) === 6 ? [252, 205, 64, 230] : [252, 236, 160, 170]),
            lineWidthMinPixels: 1.5,
          }),
          new GeoJsonLayer({
            id: "h-observed-line-" + index,
            data: fc(feature(observed?.geometry ?? null)),
            filled: false,
            stroked: true,
            getLineColor: [255, 255, 255, 245],
            lineWidthMinPixels: 2.2,
          }),
          new PathLayer({
            id: "h-wind-" + index,
            data: arrows,
            getPath: (d: { path: [number, number][] }) => d.path,
            getColor: [176, 224, 255, 110],
            widthMinPixels: 1.1,
          }),
          new PathLayer({
            id: "h-smoke-axis-" + index,
            data: frame.smoke_axis ? [frame.smoke_axis.coordinates as [number, number][]] : [],
            getPath: (d: [number, number][]) => d,
            getColor: [205, 196, 235, 150],
            widthMinPixels: 1.2,
          }),
          new PathLayer({
            id: "h-routes-" + index,
            data: frame.drones.routes,
            getPath: (d: { path: [number, number][] }) => d.path,
            getColor: (d: { role: string }) => (d.role === "scout" ? [125, 240, 210, 230] : [139, 209, 228, 210]),
            widthMinPixels: 2,
          }),
          new ScatterplotLayer({
            id: "h-targets-" + index,
            data: targets,
            getPosition: (d: [number, number]) => d,
            getRadius: 220,
            radiusMinPixels: 3.5,
            getFillColor: [139, 209, 228, 255],
            getLineColor: [16, 40, 46, 255],
            stroked: true,
            lineWidthMinPixels: 1,
          }),
          new ScatterplotLayer({
            id: "h-bases-" + index,
            data: frame.drones.bases,
            getPosition: (d: [number, number]) => d,
            getRadius: 500,
            radiusMinPixels: 6,
            getFillColor: [16, 40, 46, 230],
            getLineColor: [139, 209, 228, 255],
            stroked: true,
            lineWidthMinPixels: 2,
          }),
          new ScatterplotLayer({
            id: "h-ignition",
            data: [data.meta.ignition.position],
            getPosition: (d: [number, number]) => d,
            getRadius: 250,
            radiusMinPixels: 5,
            getFillColor: [255, 230, 120, 255],
            getLineColor: [60, 20, 0, 255],
            stroked: true,
            lineWidthMinPixels: 1.5,
          }),
        ],
      });
    })();
    return () => {
      cancelled = true;
    };
  }, [ready, frame, observed, index, data]);

  const chart = useMemo(() => {
    if (!data) return null;
    const W = 330, H = 130, P = 26;
    const t0 = Date.parse(data.frames[0].t), t1 = Date.parse(data.frames[data.frames.length - 1].t);
    const maxA = Math.max(...data.frames.map((f) => f.model.area_ha_p90), ...data.perimeters.map((p) => p.area_ha), 1);
    const x = (iso: string) => P + ((Date.parse(iso) - t0) / (t1 - t0)) * (W - P - 6);
    const y = (a: number) => H - 18 - (a / maxA) * (H - 30);
    const band =
      data.frames.map((f) => `${x(f.t)},${y(f.model.area_ha_p90)}`).join(" ") +
      " " +
      [...data.frames].reverse().map((f) => `${x(f.t)},${y(f.model.area_ha_p10)}`).join(" ");
    const median = data.frames.map((f) => `${x(f.t)},${y(f.model.area_ha_p50)}`).join(" ");
    const maxD = Math.max(...data.frames.map((f) => f.drones.count), 1);
    const yd = (d: number) => 54 - (d / maxD) * 40;
    const fleet = data.frames.map((f, i) => {
      const nx = i + 1 < data.frames.length ? x(data.frames[i + 1].t) : x(f.t);
      return `${x(f.t)},${yd(f.drones.count)} ${nx},${yd(f.drones.count)}`;
    }).join(" ");
    return { W, H, P, x, y, band, median, maxA, fleet, maxD, yd };
  }, [data]);

  if (failed) return <div className="historical-empty">{l.missing}</div>;
  if (!data || !frame || !chart) return <div className="historical-empty">{l.loading}</div>;
  const hoursSince = (Date.parse(frame.t) - Date.parse(data.meta.ignition.time)) / 3.6e6;
  const visibleEvents = data.events.filter((e) => Date.parse(e.time) <= Date.parse(frame.t)).reverse();
  const tNow = chart.x(frame.t);

  return (
    <section className="historical">
      <div className="historical-map">
        <div className="map-canvas" ref={container} />
        <div className="historical-heading">
          <span className="map-location"><Icon name="reset" size={14} /> {l.title}</span>
          <h1>{data.meta.name}</h1>
          <p>{data.meta.description}</p>
        </div>
        <div className="map-legend historical-legend">
          <span><i className="legend-observed" />{l.observed}</span>
          <span><i className="legend-band" />{l.modelled}</span>
          <span><i className="legend-forecast" />{l.forecast}</span>
          <span><i className="legend-smoke" />{l.smoke}</span>
          <span><i className="legend-route" />{l.drones}</span>
        </div>
        <div className="historical-clock">
          <strong>{fmtTime(frame.t, locale)}</strong>
          <span>{l.day} {Math.floor(hoursSince / 24) + 1} · {Math.round(hoursSince)} h {l.since}</span>
        </div>
      </div>
      <aside className="historical-panel">
        <button className="text-button" onClick={onExit}><Icon name="reset" size={13} /> {l.back}</button>
        <div className="metric-grid historical-metrics">
          <div>
            <span>{l.observedArea}</span>
            <strong>{observed ? n0(observed.area_ha, " ha") : "—"}</strong>
            <small>{observed ? fmtTime(observed.time, locale) : l.noObs}</small>
          </div>
          <div>
            <span>{l.modelArea}</span>
            <strong>{n0(frame.model.area_ha_p50, " ha")}</strong>
            <small>{n0(frame.model.area_ha_p10)}–{n0(frame.model.area_ha_p90, " ha")}</small>
          </div>
          <div>
            <span>{l.fleet}</span>
            <strong>{frame.drones.count}</strong>
            <small>
              {frame.drones.routes.length} {l.routed}
              {frame.drones.count > frame.drones.routes.length &&
                " · " + (frame.drones.count - frame.drones.routes.length) + " " + l.held + " (" + Object.keys(frame.drones.held ?? {}).join(", ") + ")"}
              <br />
              {n0(frame.model.front_km, " km")} · {n0(frame.model.growth_ha_h, " ha/h")}
            </small>
          </div>
          <div>
            <span>{l.head}</span>
            <strong>{frame.model.head_ros_m_min.toFixed(1)} m/min</strong>
            <small>{n0(frame.model.head_intensity_kw_m, " kW/m")} · {frame.model.fire_type}</small>
          </div>
        </div>
        {frame.weather && (
          <dl className="provenance-list firecast-list">
            <div>
              <dt>{l.wind}</dt>
              <dd>
                {frame.weather.wind_mps} m/s {compass(frame.weather.wind_from_deg, locale)}
                {frame.weather.gust_mps ? " (" + frame.weather.gust_mps + ")" : ""} / {frame.weather.wind_100m_mps ?? "—"} m/s {compass(frame.weather.wind_100m_from_deg, locale)}
              </dd>
            </div>
            <div>
              <dt>{l.tempRh}</dt>
              <dd>{frame.weather.temp_c} °C / {frame.weather.rh_pct} %</dd>
            </div>
            {frame.plume && (
              <div>
                <dt>{l.plume}</dt>
                <dd>{frame.plume.rise_m} m · {frame.plume.heat_mw} MW · {frame.plume.stability}</dd>
              </div>
            )}
          </dl>
        )}
        <h4 className="historical-h4">{l.chart}</h4>
        <svg className="area-chart" viewBox={`0 0 ${chart.W} ${chart.H}`} role="img" aria-label={l.chart}>
          <polygon points={chart.band} fill="#f6802033" />
          <polyline points={chart.median} fill="none" stroke="#de2d19" strokeWidth="1.6" />
          {data.perimeters.map((p) => (
            <circle key={p.time} cx={chart.x(p.time)} cy={chart.y(p.area_ha)} r="2.6" fill="#233a37" />
          ))}
          <line x1={tNow} x2={tNow} y1="6" y2={chart.H - 18} stroke="#245749" strokeDasharray="3 3" />
          <text x="2" y="12" fontSize="8" fill="#7b8782">{n0(chart.maxA, " ha")}</text>
          <text x={chart.P} y={chart.H - 4} fontSize="8" fill="#7b8782">{fmtTime(data.frames[0].t, locale)}</text>
          <text x={chart.W - 6} y={chart.H - 4} fontSize="8" fill="#7b8782" textAnchor="end">{fmtTime(data.frames[data.frames.length - 1].t, locale)}</text>
        </svg>
        <p className="chart-key"><i className="key-line" /> model p50 <i className="key-band" /> 10–90 % <i className="key-dot" /> GWIS</p>
        <h4 className="historical-h4">{l.fleetChart}</h4>
        <svg className="area-chart" viewBox={`0 0 ${chart.W} 60`} role="img" aria-label={l.fleetChart}>
          <polyline points={chart.fleet} fill="none" stroke="#2b8fa6" strokeWidth="1.6" />
          <line x1={tNow} x2={tNow} y1="6" y2="56" stroke="#245749" strokeDasharray="3 3" />
          <text x="2" y="14" fontSize="8" fill="#7b8782">{chart.maxD}</text>
        </svg>
        <h4 className="historical-h4">{l.validation}</h4>
        <table className="validation-table">
          <thead>
            <tr><th>{l.from}</th><th>{l.horizon}</th><th>{l.observedArea}</th><th>{l.modelArea}</th><th>{l.dice}</th></tr>
          </thead>
          <tbody>
            {data.validation.map((v) => (
              <tr key={v.cycle} className={Date.parse(v.to) <= Date.parse(frame.t) ? "" : "future"}>
                <td>{v.started_from === "ignition point" ? l.ignitionPoint : l.observedPerimeter}<small>{fmtTime(v.from, locale)}</small></td>
                <td>{v.hours} h</td>
                <td>{n0(v.observed_area_ha)}</td>
                <td>{n0(v.predicted_area_ha_p50)}<small>{n0(v.predicted_area_ha_p10)}–{n0(v.predicted_area_ha_p90)}</small></td>
                <td className={"dice dice-" + ((v.dice_growth ?? 0) >= 0.5 ? "good" : (v.dice_growth ?? 0) >= 0.25 ? "fair" : "poor")}>
                  {v.dice_total?.toFixed(2) ?? "—"} / {v.dice_growth?.toFixed(2) ?? "—"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <h4 className="historical-h4">{l.events}</h4>
        <ol className="event-feed">
          {visibleEvents.slice(0, 14).map((e) => (
            <li key={e.time + e.url} className={"event-" + e.kind}>
              <time>{fmtTime(e.time, locale)}{e.time_basis === "article_date" ? " · data" : ""}</time>
              <a href={safeUrl(e.url)} target="_blank" rel="noreferrer">{e.title}<Icon name="external" size={11} /></a>
              {e.detail && <p>{e.detail}</p>}
              <span className="event-source">{e.source}{e.origin === "gdacs_emm" ? " · EMM" : ""}</span>
              {e.facts && Object.keys(e.facts).length > 0 && (
                <div className="quality-flags">
                  {Object.entries(e.facts).map(([k, v]) => (
                    <span key={k}>{k.replaceAll("_", " ")}: {Array.isArray(v) ? v.join(", ") : String(v)}</span>
                  ))}
                </div>
              )}
            </li>
          ))}
        </ol>
        <details className="firecast-details">
          <summary>{l.method}</summary>
          <p className="source-note">{data.meta.method}</p>
          <p className="source-note">{data.meta.fleet_rule}</p>
          <p className="source-note">{data.meta.vegetation.note}</p>
          <p className="source-note">{data.meta.ignition.basis}</p>
          <p className="source-note">{l.synthetic}</p>
          <ul className="basis-list">
            <li><a href={safeUrl(data.meta.weather_source.url)} target="_blank" rel="noreferrer">{data.meta.weather_source.name}</a></li>
            <li><a href={safeUrl(data.meta.elevation_source.url)} target="_blank" rel="noreferrer">{data.meta.elevation_source.name}</a></li>
            {data.meta.sources.map((s) => (
              <li key={s.url}><a href={safeUrl(s.url)} target="_blank" rel="noreferrer">{s.name}</a></li>
            ))}
            {data.meta.final_area.map((a) => (
              <li key={a.url}><a href={safeUrl(a.url)} target="_blank" rel="noreferrer">{a.source}: {n0(a.area_ha, " ha")}</a></li>
            ))}
          </ul>
          <p className="source-note">{data.meta.model}</p>
        </details>
      </aside>
      <footer className="historical-bar">
        <button className="primary-button" onClick={() => setPlaying((p) => !p)}>
          <Icon name={playing ? "pause" : "play"} size={14} /> {playing ? l.pause : l.play}
        </button>
        <div className="historical-scrub">
          <input
            type="range"
            min={0}
            max={data.frames.length - 1}
            value={index}
            aria-label={l.title}
            onChange={(e) => {
              setPlaying(false);
              setIndex(Number(e.target.value));
            }}
          />
          <div className="scrub-marks">
            {data.perimeters.map((p) => (
              <i key={p.time} className="mark-observed" style={{ left: ((Date.parse(p.time) - Date.parse(data.frames[0].t)) / (Date.parse(data.frames[data.frames.length - 1].t) - Date.parse(data.frames[0].t))) * 100 + "%" }} />
            ))}
            {data.events.filter((e) => e.origin === "web_research").map((e) => (
              <i key={e.time + e.url} className="mark-event" title={e.title} style={{ left: Math.min(100, ((Date.parse(e.time) - Date.parse(data.frames[0].t)) / (Date.parse(data.frames[data.frames.length - 1].t) - Date.parse(data.frames[0].t))) * 100) + "%" }} />
            ))}
          </div>
        </div>
        <span className="historical-step">{index + 1} / {data.frames.length}</span>
      </footer>
    </section>
  );
}
