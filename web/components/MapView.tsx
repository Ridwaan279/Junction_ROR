"use client";
import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import type {
  Map as LibreMap,
  IControl,
} from "maplibre-gl";
import type { MapboxOverlay } from "@deck.gl/mapbox";
import type { Incident, Snapshot } from "@/lib/contracts";
import type { Locale } from "@/lib/i18n";
import { copy } from "@/lib/i18n";
import { forecastFeatures } from "@/lib/model";
import { addTerrainLayers, loadMapLibre, loadTerrainStyle } from "@/lib/terrain";
import Icon from "./Icon";
const BOUNDS: [[number, number], [number, number]] = [
  [-9.6, 36.9],
  [-6.1, 42.2],
];
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
    [fps, setFps] = useState<number | null>(null);
  const t = copy[locale];
  useEffect(() => {
    let cancelled = false,
      instance: LibreMap | null = null;
    (async () => {
      try {
        const [maplibre, { MapboxOverlay }, terrainStyle] = await Promise.all([
          loadMapLibre(),
          import("@deck.gl/mapbox"),
          loadTerrainStyle(),
        ]);
        if (cancelled || !container.current) return;
        instance = new maplibre.Map({
          container: container.current,
          style: terrainStyle,
          center: [-8, 39.6],
          zoom: 5.6,
          minZoom: 4,
          maxZoom: 15,
          maxBounds: [
            [-20, 30],
            [5, 50],
          ],
          attributionControl: { compact: true },
        });
        map.current = instance;
        instance.addControl(
          new maplibre.NavigationControl({ showCompass: false }),
          "top-right",
        );
        instance.on("load", async () => {
          if (cancelled || !instance) return;
          try {
            await addTerrainLayers(instance);
          } catch {
            // The evidence map remains usable when elevation tiles fail.
            console.warn("Terrain detail could not be initialized");
          }
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
          if ("sourceId" in event && String(event.sourceId).startsWith("portugal-")) return;
          if (!instance?.isStyleLoaded()) setFailed(true);
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
      overlay.current.setProps({
        layers: [
          new GeoJsonLayer({
            id: "spread-" + snapshot.scenario.epoch,
            data: forecastFeatures(snapshot),
            filled: true,
            stroked: true,
            getFillColor: [236, 125, 58, 65],
            getLineColor: [255, 177, 98, 190],
            lineWidthMinPixels: 1,
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
  }, [snapshot, ready, selectedId, onSelect]);
  useEffect(() => {
    const selected = snapshot.incidents.find((i) => i.id === selectedId);
    if (ready && selected?.position)
      map.current?.flyTo({
        center: selected.position,
        zoom: 9.5,
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
        <Link className="terrain-explorer-link" href="/terrain">
          <Icon name="layers" size={16} />
          {locale === "pt" ? "Explorar terreno 3D" : "Explore 3D terrain"}
          <Icon name="external" size={13} />
        </Link>
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
        {snapshot.forecasts.length > 0 && (
          <span>
            <i className="legend-spread" />
            {t.spread}
          </span>
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
