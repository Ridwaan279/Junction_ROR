"use client";
import { useEffect, useRef, useState } from "react";
import type {
  Map as LibreMap,
  IControl,
  StyleSpecification,
} from "maplibre-gl";
import type { MapboxOverlay } from "@deck.gl/mapbox";
import type { Incident, Snapshot } from "@/lib/contracts";
import type { Locale } from "@/lib/i18n";
import { copy } from "@/lib/i18n";
import { forecastFeatures } from "@/lib/model";
import {
  arrowPaths,
  featuresOf,
  firecastFor,
  horizonColor,
  horizonsOf,
  isFirecast,
  smokeAlpha,
} from "@/lib/firecast";
import Icon from "./Icon";
const BOUNDS: [[number, number], [number, number]] = [
  [-9.6, 36.9],
  [-6.1, 42.2],
];
export const style: StyleSpecification = {
  version: 8,
  sources: {
    osm: {
      type: "raster",
      tiles: ["https://tile.openstreetmap.org/{z}/{x}/{y}.png"],
      tileSize: 256,
      attribution:
        '© <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noreferrer">OpenStreetMap</a> contributors',
      maxzoom: 19,
    },
    terrain: {
      type: "raster-dem",
      tiles: [
        "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png",
      ],
      tileSize: 256,
      encoding: "terrarium",
      maxzoom: 13,
      attribution:
        'Terrain: <a href="https://registry.opendata.aws/terrain-tiles/" target="_blank" rel="noreferrer">Mapzen / AWS Terrain Tiles</a>',
    },
  },
  layers: [
    {
      id: "base",
      type: "background",
      paint: { "background-color": "#223b3a" },
    },
    {
      id: "osm",
      type: "raster",
      source: "osm",
      paint: {
        "raster-saturation": -0.65,
        "raster-brightness-max": 0.68,
        "raster-brightness-min": 0.1,
        "raster-contrast": 0.08,
      },
    },
    {
      id: "hillshade",
      type: "hillshade",
      source: "terrain",
      paint: {
        "hillshade-exaggeration": 0.6,
        "hillshade-shadow-color": "#0c1d1b",
        "hillshade-highlight-color": "#eef4e4",
        "hillshade-accent-color": "#2b4a43",
        "hillshade-illumination-direction": 315,
      },
    },
  ],
};
const T = {
  pt: {
    now: "Agora",
    smoke: "Fumo",
    wind: "Vento",
    spreadAt: "Propagação prevista",
    probability: "probabilidade",
    isochrones: "Frente provável (50 %)",
    band: "Banda 10–90 %",
    smokeLegend: "Fumo PM2.5 ≥ 35 / 150 / 500 µg/m³",
    windLegend: "Vento a 10 m",
    head: "Cabeça do incêndio (1 h)",
    model: "Modelo físico (não operacional)",
  },
  en: {
    now: "Now",
    smoke: "Smoke",
    wind: "Wind",
    spreadAt: "Predicted spread",
    probability: "probability",
    isochrones: "Likely front (50 %)",
    band: "10–90 % band",
    smokeLegend: "PM2.5 smoke ≥ 35 / 150 / 500 µg/m³",
    windLegend: "10 m wind",
    head: "Head fire (1 h)",
    model: "Physics model (not operational)",
  },
};
export default function MapView({
  snapshot,
  selectedId,
  onSelect,
  locale,
}: {
  snapshot: Snapshot;
  selectedId: string | null;
  onSelect: (id: string) => void;
  locale: Locale;
}) {
  const container = useRef<HTMLDivElement>(null),
    map = useRef<LibreMap | null>(null),
    overlay = useRef<MapboxOverlay | null>(null);
  const [ready, setReady] = useState(false),
    [failed, setFailed] = useState(false),
    [fps, setFps] = useState<number | null>(null),
    [horizon, setHorizon] = useState<number | null>(null),
    [showSmoke, setShowSmoke] = useState(true),
    [showWind, setShowWind] = useState(true);
  const t = copy[locale],
    f = T[locale];
  const firecast = firecastFor(snapshot, selectedId);
  const horizons = horizonsOf(firecast);
  const activeHorizon =
    horizon !== null && horizons.includes(horizon)
      ? horizon
      : (horizons.find((h) => h >= 1) ?? horizons[0] ?? null);
  useEffect(() => {
    let cancelled = false,
      instance: LibreMap | null = null;
    (async () => {
      try {
        const [maplibre, { MapboxOverlay }] = await Promise.all([
          import("maplibre-gl"),
          import("@deck.gl/mapbox"),
        ]);
        if (cancelled || !container.current) return;
        instance = new maplibre.Map({
          container: container.current,
          style: process.env.NEXT_PUBLIC_MAP_STYLE_URL || style,
          center: [-8, 39.6],
          zoom: 5.6,
          minZoom: 4,
          maxZoom: 15,
          maxBounds: [
            [-12, 35],
            [-4, 44],
          ],
          attributionControl: { compact: true },
        });
        map.current = instance;
        instance.addControl(
          new maplibre.NavigationControl({ showCompass: false }),
          "top-right",
        );
        instance.on("load", () => {
          if (cancelled || !instance) return;
          const deck = new MapboxOverlay({ interleaved: false, layers: [] });
          overlay.current = deck;
          instance.addControl(deck as unknown as IControl);
          instance.fitBounds(BOUNDS, { padding: 65, duration: 1200 });
          setReady(true);
        });
        let frames = 0,
          start = 0;
        instance.on("render", () => {
          if (!instance?.isMoving()) {
            start = 0;
            frames = 0;
            return;
          }
          const now = performance.now();
          if (!start) start = now;
          frames++;
          if (now - start >= 600) {
            setFps(Math.round((frames * 1000) / (now - start)));
            frames = 0;
            start = now;
          }
        });
        instance.on("error", (event) => {
          // A blocked tile source (base map or terrain) must not hide the map.
          const sourceError = Boolean((event as { sourceId?: string }).sourceId);
          if (!sourceError && !instance?.isStyleLoaded()) setFailed(true);
        });
      } catch (error) {
        if (!cancelled) {
          console.warn("Base map unavailable", error);
          setFailed(true);
        }
      }
    })();
    return () => {
      cancelled = true;
      overlay.current?.finalize();
      overlay.current = null;
      instance?.remove();
      map.current = null;
    };
  }, []);
  useEffect(() => {
    let cancelled = false;
    if (!ready || !overlay.current) return;
    (async () => {
      const { ScatterplotLayer, GeoJsonLayer, PathLayer } =
        await import("@deck.gl/layers");
      if (cancelled || !overlay.current) return;
      const incidents = snapshot.incidents.filter((i) => i.position);
      const epoch = snapshot.scenario.epoch;
      const fc = (features: GeoJSON.Feature[]): GeoJSON.FeatureCollection => ({
        type: "FeatureCollection",
        features,
      });
      const spread = featuresOf(firecast, "spread");
      const band = spread.filter(
        (x) => Number(x.properties.horizon_h) === activeHorizon,
      );
      const isochrones = spread
        .filter(
          (x) =>
            Number(x.properties.probability) === 0.5 &&
            Number(x.properties.horizon_h) <= (activeHorizon ?? 99),
        )
        .sort(
          (a, b) => Number(b.properties.horizon_h) - Number(a.properties.horizon_h),
        );
      const smoke = featuresOf(firecast, "smoke")
        .filter((x) => Number(x.properties.hour_offset) === activeHorizon)
        .sort(
          (a, b) =>
            Number(a.properties.threshold_ug_m3) -
            Number(b.properties.threshold_ug_m3),
        );
      const smokeAxis = featuresOf(firecast, "smoke_axis").filter(
        (x) => Number(x.properties.hour_offset) === activeHorizon,
      );
      const arrows = (layer: string) =>
        featuresOf(firecast, layer).flatMap((x) =>
          arrowPaths(
            (x.geometry as GeoJSON.LineString).coordinates as [number, number][],
          ).map((path) => ({ path, properties: x.properties })),
        );
      const legacy = snapshot.forecasts.some((x) => !isFirecast(x));
      overlay.current.setProps({
        getTooltip: ({ object }: { object?: unknown }) => {
          const p = (object as { properties?: Record<string, unknown> })
            ?.properties;
          if (!p) return null;
          if (p.layer === "spread")
            return (
              (Number(p.horizon_h) === 0 ? f.now : "+" + p.horizon_h + " h") +
              " · " +
              Math.round(Number(p.probability) * 100) +
              " % " +
              f.probability
            );
          if (p.layer === "smoke")
            return "PM2.5 ≥ " + p.threshold_ug_m3 + " µg/m³";
          if (p.layer === "wind")
            return (
              f.wind + ": " + p.speed_mps + " m/s · " + p.from_deg + "°" +
              (p.gust_mps ? " · gust " + p.gust_mps + " m/s" : "")
            );
          if (p.layer === "head_vector")
            return f.head + ": " + p.ros_m_min + " m/min → " + p.bearing_deg + "°";
          return null;
        },
        layers: [
          ...(legacy
            ? [
                new GeoJsonLayer({
                  id: "spread-" + epoch,
                  data: forecastFeatures({
                    ...snapshot,
                    forecasts: snapshot.forecasts.filter((x) => !isFirecast(x)),
                  }),
                  filled: true,
                  stroked: true,
                  getFillColor: [236, 125, 58, 65],
                  getLineColor: [255, 177, 98, 190],
                  lineWidthMinPixels: 1,
                }),
              ]
            : []),
          new GeoJsonLayer({
            id: "fc-smoke-" + epoch + "-" + activeHorizon,
            data: fc(showSmoke ? smoke : []),
            filled: true,
            stroked: true,
            pickable: true,
            getFillColor: (x: GeoJSON.Feature) => [
              96,
              90,
              122,
              smokeAlpha(Number(x.properties?.threshold_ug_m3)),
            ],
            getLineColor: [180, 172, 205, 110],
            lineWidthMinPixels: 1,
          }),
          new PathLayer({
            id: "fc-smoke-axis-" + epoch,
            data: showSmoke
              ? smokeAxis.map(
                  (x) => (x.geometry as GeoJSON.LineString).coordinates,
                )
              : [],
            getPath: (d: number[][]) => d as [number, number][],
            getColor: [205, 196, 235, 170],
            widthMinPixels: 1.5,
          }),
          new GeoJsonLayer({
            id: "fc-band-" + epoch + "-" + activeHorizon,
            data: fc(band),
            filled: true,
            stroked: false,
            pickable: true,
            getFillColor: (x: GeoJSON.Feature) => {
              const p = Number(x.properties?.probability);
              const [r, g, b] = horizonColor(activeHorizon ?? 1);
              return [r, g, b, p >= 0.9 ? 120 : p >= 0.5 ? 80 : 45];
            },
          }),
          new GeoJsonLayer({
            id: "fc-isochrones-" + epoch + "-" + activeHorizon,
            data: fc(isochrones),
            filled: false,
            stroked: true,
            getLineColor: (x: GeoJSON.Feature) => [
              ...horizonColor(Number(x.properties?.horizon_h)),
              235,
            ] as [number, number, number, number],
            lineWidthMinPixels: 2,
          }),
          new PathLayer({
            id: "fc-wind-" + epoch,
            data: showWind ? arrows("wind") : [],
            getPath: (d: { path: [number, number][] }) => d.path,
            getColor: [176, 224, 255, 120],
            widthMinPixels: 1.1,
            pickable: true,
            capRounded: true,
            jointRounded: true,
          }),
          new PathLayer({
            id: "fc-head-" + epoch,
            data: arrows("head_vector"),
            getPath: (d: { path: [number, number][] }) => d.path,
            getColor: [255, 72, 48, 245],
            widthMinPixels: 3,
            pickable: true,
            capRounded: true,
            jointRounded: true,
          }),
          new PathLayer({
            id: "drone-" + snapshot.scenario.epoch,
            data: snapshot.drone_plans,
            getPath: (d) => d.route,
            getColor: [139, 209, 228, 220],
            widthMinPixels: 3,
            getWidth: 3,
          }),
          new ScatterplotLayer<Incident>({
            id: "uncertainty-" + snapshot.scenario.epoch,
            data: incidents,
            getPosition: (d) => d.position!,
            getRadius: (d) => d.uncertainty_m ?? 0,
            getFillColor: [244, 143, 66, 30],
            getLineColor: [246, 163, 87, 140],
            stroked: true,
            lineWidthMinPixels: 1,
            radiusMinPixels: 12,
          }),
          new ScatterplotLayer<Incident>({
            id: "reports-" + snapshot.scenario.epoch,
            data: incidents,
            getPosition: (d) => d.position!,
            getRadius: 180,
            radiusMinPixels: 7,
            radiusMaxPixels: 12,
            getFillColor: (d) =>
              d.id === selectedId ? [255, 202, 135, 255] : [245, 123, 58, 255],
            getLineColor: [255, 238, 213, 255],
            stroked: true,
            lineWidthMinPixels: 2,
            pickable: true,
            onClick: (info) => {
              if (info.object) onSelect(info.object.id);
            },
          }),
        ],
      });
    })();
    return () => {
      cancelled = true;
    };
  }, [
    snapshot,
    ready,
    selectedId,
    onSelect,
    firecast,
    activeHorizon,
    showSmoke,
    showWind,
    f,
  ]);
  useEffect(() => {
    const selected = snapshot.incidents.find((i) => i.id === selectedId);
    if (ready && selected?.position)
      map.current?.flyTo({
        center: selected.position,
        zoom: 12.3,
        duration: 800,
      });
  }, [selectedId, ready, snapshot.incidents]);
  const project = ([lon, lat]: [number, number]) => [
    (lon + 10) * 130 + 50,
    (42.5 - lat) * 110 + 35,
  ];
  const outline = [
    [-8.88, 41.86],
    [-8.2, 42.13],
    [-6.55, 41.94],
    [-6.2, 41.57],
    [-6.95, 41.05],
    [-6.9, 40.2],
    [-7.05, 39.68],
    [-7.52, 39.52],
    [-7.02, 38.95],
    [-7.42, 38.24],
    [-7.45, 37.2],
    [-7.9, 37.02],
    [-8.95, 37.02],
    [-8.8, 37.8],
    [-8.8, 38.45],
    [-9.5, 38.72],
    [-9.4, 39.35],
    [-8.9, 40.2],
    [-8.88, 41.86],
  ] as [number, number][];
  return (
    <div className="map-area">
      <div className="geographic-fallback" role="img" aria-label={t.simplified}>
        <svg viewBox="0 0 620 680" preserveAspectRatio="xMidYMid meet">
          <defs>
            <pattern
              id="grid"
              width="60"
              height="60"
              patternUnits="userSpaceOnUse"
            >
              <path
                d="M60 0H0V60"
                fill="none"
                stroke="#426362"
                strokeWidth=".6"
              />
            </pattern>
          </defs>
          <rect width="620" height="680" fill="url(#grid)" />
          <path
            d={
              outline
                .map((p, i) => (i ? "L" : "M") + project(p).join(","))
                .join(" ") + "Z"
            }
            fill="#3b5146"
            stroke="#9dac8d"
            strokeWidth="1.5"
          />
          <text
            x="65"
            y="390"
            fill="#86a5a4"
            fontSize="14"
            transform="rotate(-90,65,390)"
          >
            Atlântico
          </text>
          {(
            [
              [[-8.61, 41.15], "Porto"],
              [[-9.14, 38.72], "Lisboa"],
              [[-7.93, 37.02], "Faro"],
              [[-7.93, 40.66], "Viseu"],
            ] as [[number, number], string][]
          ).map(([p, name]) => (
            <g key={name} transform={"translate(" + project(p).join(",") + ")"}>
              <circle r="3" fill="#bed0ba" />
              <text x="10" y="5" fill="#c4d2c4" fontSize="12">
                {name}
              </text>
            </g>
          ))}
          {snapshot.incidents
            .filter((i) => i.position)
            .map((i) => (
              <g
                key={i.id}
                transform={"translate(" + project(i.position!).join(",") + ")"}
                role="button"
                tabIndex={0}
                aria-label={i.title}
                onClick={() => onSelect(i.id)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ")
                    onSelect(i.id);
                }}
                className="schematic-pin"
              >
                <circle r="24" fill="#f7944c" opacity=".16" />
                <circle r="8" fill="#f7944c" stroke="#ffdec2" strokeWidth="2" />
              </g>
            ))}
          <text x="390" y="320" fill="#97aba0" fontSize="13">
            España
          </text>
        </svg>
      </div>
      <div
        className={"map-canvas " + (!ready || failed ? "map-pending" : "")}
        ref={container}
        aria-label={t.map}
      />
      <div className="map-heading">
        <span className="map-location">
          <Icon name="pin" size={15} />
          {t.explore}
        </span>
        <h1>
          {locale === "pt"
            ? "Cada relato, com a sua fonte."
            : "Every report, with its source."}
        </h1>
      </div>
      <div className="map-tools">
        <button
          className="map-fit"
          aria-label={t.recenter}
          onClick={() =>
            map.current?.fitBounds(BOUNDS, { padding: 65, duration: 1000 })
          }
        >
          <Icon name="target" size={18} />
        </button>
      </div>
      {firecast && horizons.length > 0 && (
        <div className="forecast-controls" role="group" aria-label={f.spreadAt}>
          <span className="forecast-controls-title">{f.spreadAt}</span>
          <div className="horizon-buttons">
            {horizons.map((h) => (
              <button
                key={h}
                className={h === activeHorizon ? "active" : ""}
                aria-pressed={h === activeHorizon}
                onClick={() => setHorizon(h)}
              >
                {h === 0 ? f.now : "+" + h + " h"}
              </button>
            ))}
          </div>
          <div className="layer-toggles">
            <label>
              <input
                type="checkbox"
                checked={showSmoke}
                onChange={(e) => setShowSmoke(e.target.checked)}
              />
              {f.smoke}
            </label>
            <label>
              <input
                type="checkbox"
                checked={showWind}
                onChange={(e) => setShowWind(e.target.checked)}
              />
              {f.wind}
            </label>
          </div>
          <span className="forecast-controls-note">{f.model}</span>
        </div>
      )}
      {(failed || !ready) && (
        <div className="map-fallback-notice">
          {failed ? t.mapUnavailable : t.simplified}
        </div>
      )}
      <div className="map-legend">
        <span>
          <i className="legend-dot" />
          {t.reported}
        </span>
        <span>
          <i className="legend-ring" />
          {t.radius}
        </span>
        {firecast ? (
          <>
            <span>
              <i className="legend-isochrone" />
              {f.isochrones}
            </span>
            <span>
              <i className="legend-band" />
              {f.band}
            </span>
            <span>
              <i className="legend-smoke" />
              {f.smokeLegend}
            </span>
            <span>
              <i className="legend-wind" />
              {f.windLegend}
            </span>
            <span>
              <i className="legend-head" />
              {f.head}
            </span>
          </>
        ) : (
          snapshot.forecasts.length > 0 && (
            <span>
              <i className="legend-spread" />
              {t.spread}
            </span>
          )
        )}
        {snapshot.drone_plans.length > 0 && (
          <span>
            <i className="legend-route" />
            {t.drone}
          </span>
        )}
      </div>
      <div className="map-metrics">
        <span>{t.flatMap}</span>
        <span>
          {t.fps}: {fps ?? t.noFps}
        </span>
      </div>
    </div>
  );
}
