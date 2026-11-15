"use client";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Map as LibreMap } from "maplibre-gl";
import { loadMapLibre } from "@/lib/terrain";
import { loadOpsStyle, setThreeD } from "@/lib/ops-map";
import {
  addReplayLayers, compassPoint, FIRE, frameAt, lisbon, REPLAY_BASE, setReplayVisible, showReplayFrame, showVillages, villagesAt,
  type ReplayFrame, type ReplayIndex,
} from "@/lib/replay";
import s from "./ops.module.css";

type Locale = "pt" | "en";
type Mode = "live" | "replay";
type LiveStatus = {
  mode: "live"; origin: "service" | "snapshot"; checked_at: string;
  state: "no_active_fires" | "active_fires" | "check_failed" | "partial";
  statement: string | null; statement_en: string | null;
  active: { id: string; place: string; position: [number, number] | null; status: string; url: string }[];
  sources: { name: string; url: string; via: string; captured_at: string; status: string }[];
};

const PORTUGAL = { center: [-9.35, 39.7] as [number, number], zoom: 6.55, pitch: 46, bearing: -8 };
const REPLAY_SECONDS = 75; // the 42-hour window plays in about 75 s

const T = {
  pt: {
    live: "Ao vivo", replay: "Arganil 2025", replayLong: "Reprodução histórica · dados reais", threeD: "3D", twoD: "2D",
    noFires: "Sem incêndios ativos neste momento", activeFires: (n: number) => `${n} incêndio${n === 1 ? "" : "s"} ativo${n === 1 ? "" : "s"}`,
    checkFailed: "Não foi possível confirmar a situação", partial: "Verificação incompleta",
    checked: "Verificado", via: "via", snapshot: "Serviço ao vivo desligado: a mostrar a última verificação",
    loadingLive: "A verificar fontes…", watch: "Ver o incêndio de Arganil 2025",
    area: "Área modelada", spread: "Avanço da frente", wind: "Vento", temp: "Temp.", rh: "HR",
    evacuated: "aldeias evacuadas", sources: "fontes", facts: "factos", from: "de", gusts: "Rajadas", status: "Estado", started: "Início",
    timeline: "Linha temporal", play: "Reproduzir", pause: "Pausa", automation: "Resposta automática",
    observed: "observado", simulated: "simulado", legendBurned: "Área ardida modelada (10/50/90 %)",
    legendForecast: "Previsão 6 h / 12 h", legendSmoke: "Fumo PM2.5", legendVillage: "Aldeia evacuada / ameaçada",
    mapFailed: "O mapa não carregou. Verifique a ligação.", close: "Fechar", model: "modelo",
    sourcesTitle: "Fontes", sourcesNote: "Cada item só aparece a partir da hora em que foi público.",
    steps: { reported: "Alerta", dispatched: "Despacho", first_alert_dispatch: "1.º alerta", active: "Em curso", corroborated_by_news: "Confirmado pela imprensa", forecast: "Previsão calculada", drones_launched: "Drones lançados" },
  },
  en: {
    live: "Live", replay: "Arganil 2025", replayLong: "Historical replay · real data", threeD: "3D", twoD: "2D",
    noFires: "No active fires right now", activeFires: (n: number) => `${n} active fire${n === 1 ? "" : "s"}`,
    checkFailed: "Could not confirm the current situation", partial: "Check incomplete",
    checked: "Checked", via: "via", snapshot: "Live service offline: showing the last check",
    loadingLive: "Checking sources…", watch: "Watch the Arganil 2025 fire",
    area: "Modelled area", spread: "Head spread", wind: "Wind", temp: "Temp.", rh: "RH",
    evacuated: "villages evacuated", sources: "sources", facts: "facts", from: "from", gusts: "Gusts", status: "Status", started: "Start",
    timeline: "Timeline", play: "Play", pause: "Pause", automation: "Automatic response",
    observed: "observed", simulated: "simulated", legendBurned: "Modelled burned area (10/50/90 %)",
    legendForecast: "Forecast 6 h / 12 h", legendSmoke: "PM2.5 smoke", legendVillage: "Village evacuated / threatened",
    mapFailed: "The map did not load. Check the connection.", close: "Close", model: "model",
    sourcesTitle: "Sources", sourcesNote: "Each item appears only from the time it was public.",
    steps: { reported: "Alert", dispatched: "Dispatch", first_alert_dispatch: "1st alert", active: "Active", corroborated_by_news: "Confirmed by press", forecast: "Forecast computed", drones_launched: "Drones launched" },
  },
};

export default function OpsConsole() {
  const [locale, setLocale] = useState<Locale>("pt");
  const [mode, setMode] = useState<Mode>("live");
  const [threeD, set3D] = useState(true);
  const [ready, setReady] = useState(false);
  const [mapFailed, setMapFailed] = useState(false);
  const [live, setLive] = useState<LiveStatus | "failed" | null>(null);
  const [index, setIndex] = useState<ReplayIndex | null>(null);
  const [frame, setFrame] = useState<ReplayFrame | null>(null);
  const [t, setT] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [drawer, setDrawer] = useState(false);
  const [toast, setToast] = useState<{ at: string; text: string; publisher: string } | null>(null);
  const [now, setNow] = useState<number | null>(null);
  const container = useRef<HTMLDivElement>(null);
  const map = useRef<LibreMap | null>(null);
  const frames = useRef(new Map<string, ReplayFrame>());
  const lastT = useRef(0);
  const lastShown = useRef<string | null>(null);
  const toastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const L = T[locale];

  useEffect(() => { document.documentElement.lang = locale; }, [locale]);
  useEffect(() => {
    const tick = () => setNow(Date.now());
    const first = setTimeout(tick, 0), id = setInterval(tick, 15_000);
    return () => { clearTimeout(first); clearInterval(id); };
  }, []);

  // Map: satellite on 3D terrain from the first frame, no button needed.
  useEffect(() => {
    let cancelled = false, instance: LibreMap | null = null;
    (async () => {
      const [maplibre, style] = await Promise.all([loadMapLibre(), loadOpsStyle()]);
      if (cancelled || !container.current) return;
      instance = new maplibre.Map({
        container: container.current, style, ...PORTUGAL, maxPitch: 80,
        attributionControl: false, fadeDuration: 150,
      });
      map.current = instance;
      instance.addControl(new maplibre.AttributionControl({ compact: true }), "bottom-left");
      instance.addControl(new maplibre.NavigationControl({ visualizePitch: true }), "bottom-right");
      instance.on("load", () => {
        if (cancelled || !instance) return;
        addReplayLayers(instance);
        setReplayVisible(instance, false);
        // Keep the credits collapsed behind the (i) button; they stay one click away.
        instance.getContainer().querySelector(".maplibregl-ctrl-attrib")?.classList.remove("maplibregl-compact-show");
        setReady(true);
      });
      instance.on("error", (e) => {
        // Missing tiles must not blank the screen; only a style failure does.
        if (!(e as { sourceId?: string }).sourceId && !instance?.isStyleLoaded()) setMapFailed(true);
      });
    })().catch(() => { if (!cancelled) setMapFailed(true); });
    return () => { cancelled = true; instance?.remove(); map.current = null; };
  }, []);

  // Live status: the service endpoint when configured, otherwise the last real capture.
  useEffect(() => {
    let cancelled = false;
    const base = process.env.NEXT_PUBLIC_API_BASE_URL;
    (async () => {
      if (base) {
        try {
          const r = await fetch(base.replace(/\/$/, "") + "/api/live/status", { cache: "no-store" });
          if (r.ok) { const v = (await r.json()) as LiveStatus; if (!cancelled) setLive({ ...v, origin: "service" }); return; }
        } catch { /* fall through to the snapshot */ }
      }
      const r = await fetch("/fixtures/v2/live-status.json", { cache: "no-store" });
      if (!r.ok) throw new Error("no live status");
      const v = (await r.json()) as LiveStatus;
      if (!cancelled) setLive({ ...v, origin: "snapshot" });
    })().catch(() => { if (!cancelled) setLive("failed"); });
    return () => { cancelled = true; };
  }, []);

  const loadFrame = useCallback(async (file: string) => {
    const hit = frames.current.get(file);
    if (hit) return hit;
    const r = await fetch(REPLAY_BASE + file);
    if (!r.ok) throw new Error("frame");
    const f = (await r.json()) as ReplayFrame;
    frames.current.set(file, f);
    return f;
  }, []);

  const enterReplay = useCallback(async () => {
    setMode("replay");
    setDrawer(false);
    let idx = index;
    if (!idx) {
      const r = await fetch(REPLAY_BASE + "index.json");
      idx = (await r.json()) as ReplayIndex;
      setIndex(idx);
    }
    const start = Date.parse(idx.window.from);
    lastT.current = start;
    lastShown.current = null;
    setT(start);
    setPlaying(true);
    idx.frames.forEach((f) => void loadFrame(f.file).catch(() => undefined));
  }, [index, loadFrame]);

  // Camera and layer visibility follow the mode.
  useEffect(() => {
    const m = map.current;
    if (!ready || !m) return;
    setReplayVisible(m, mode === "replay");
    if (mode === "replay" && index) {
      m.flyTo({ center: [index.ignition.position[0] - 0.012, index.ignition.position[1] - 0.006], zoom: 11.6,
        pitch: threeD ? 64 : 0, bearing: threeD ? -28 : 0, duration: 3200, essential: true });
    } else if (mode === "live") {
      m.flyTo({ ...PORTUGAL, pitch: threeD ? PORTUGAL.pitch : 0, bearing: threeD ? PORTUGAL.bearing : 0, duration: 2200, essential: true });
    }
  }, [mode, ready, index]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => { if (ready && map.current) setThreeD(map.current, threeD); }, [threeD, ready]);

  // Frame on screen = latest frame not after t.
  useEffect(() => {
    if (!index || mode !== "replay") return;
    const k = frameAt(index, t), file = index.frames[k].file;
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

  // Key moments: official statuses and the first report of each kind of impact.
  const moments = useMemo(() => {
    if (!index) return [];
    const out: { at: string; text: string; publisher: string; kind: string }[] = index.timeline
      .filter((s) => ["Início", "Despacho de 1º Alerta", "Em Curso"].includes(s.status))
      .map((s) => ({ at: s.at, text: `Fogos.pt: ${s.status}`, publisher: "Fogos.pt", kind: "official" }));
    for (const cat of ["ignition_cause", "evacuation", "shelter", "road_closure", "event_gathering"]) {
      const e = index.evidence.filter((x) => x.category === cat && x.quality === "verified").sort((a, b) => a.known_at.localeCompare(b.known_at))[0];
      if (e && e.known_at <= index.window.to) out.push({ at: e.known_at, text: locale === "pt" ? e.quote : e.statement_en, publisher: e.publisher, kind: cat });
    }
    return out.sort((a, b) => a.at.localeCompare(b.at));
  }, [index, locale]);

  useEffect(() => {
    if (!playing) return;
    const hit = [...moments].reverse().find((m) => Date.parse(m.at) <= t);
    if (!hit || hit.at === lastShown.current) return;
    lastShown.current = hit.at;
    setToast(hit);
    // The hide timer lives in a ref: this effect re-runs every animation frame.
    if (toastTimer.current) clearTimeout(toastTimer.current);
    toastTimer.current = setTimeout(() => setToast(null), 6500);
  }, [t, moments, playing]);
  useEffect(() => () => { if (toastTimer.current) clearTimeout(toastTimer.current); }, []);

  const minute = Math.floor(t / 60_000);
  const knownEvidence = useMemo(() => {
    if (!index) return [];
    const cut = minute * 60_000;
    return index.evidence.filter((e) => e.quality === "verified" && Date.parse(e.known_at) <= cut).sort((a, b) => b.known_at.localeCompare(a.known_at));
  }, [index, minute]);
  const villages = useMemo(() => (index ? villagesAt(index, minute * 60_000) : []), [index, minute]);
  const publishers = new Set(knownEvidence.map((e) => e.page)).size;
  const evacuated = villages.filter((v) => v.evacuated).length;
  useEffect(() => { if (ready && map.current) showVillages(map.current, villages); }, [villages, ready]);
  const reportedAt = index?.timeline[0]?.at;
  const beforeReport = !!reportedAt && t < Date.parse(reportedAt);

  return (
    <main className={s.shell}>
      <div ref={container} className={s.map} aria-label={locale === "pt" ? "Mapa 3D de Portugal" : "3D map of Portugal"} />
      <div className={s.vignette} aria-hidden />

      <header className={s.topLeft}>
        <div className={s.brand}>
          <span className={s.brandMark} aria-hidden>
            <svg viewBox="0 0 24 24" width="18" height="18"><path fill="currentColor" d="M12 2.5c1.2 5.2-4.4 5.6-3.4 10 .3 1.2 1.2 2 2.3 2.2-1-3.3 3.1-4.6 3.2-7.9 4.1 4 5.3 7.4 4.2 10.6A6.6 6.6 0 0 1 5.6 16c-.6-5 3.6-7.6 6.4-13.5Z" /></svg>
          </span>
          <span className={s.brandName}>EmberWatch</span>
        </div>
        <span className={mode === "live" ? s.pillLive : s.pillReplay}>
          <i />{mode === "live" ? L.live : L.replayLong}
        </span>
        <span className={s.clock} suppressHydrationWarning>
          {mode === "replay" && index ? lisbon(new Date(t).toISOString(), locale, true) : now ? lisbon(new Date(now).toISOString(), locale, true) : ""}
          <small>Lisboa</small>
        </span>
      </header>

      <nav className={s.topRight} aria-label={locale === "pt" ? "Controlos" : "Controls"}>
        <div className={s.segmented} role="group">
          <button aria-pressed={mode === "live"} onClick={() => { setPlaying(false); setMode("live"); }}><i className={s.dotLive} />{L.live}</button>
          <button aria-pressed={mode === "replay"} onClick={() => void enterReplay()}><i className={s.dotReplay} />{L.replay}</button>
        </div>
        <div className={s.segmented} role="group" aria-label="3D / 2D">
          <button aria-pressed={threeD} onClick={() => set3D(true)}>{L.threeD}</button>
          <button aria-pressed={!threeD} onClick={() => set3D(false)}>{L.twoD}</button>
        </div>
        <div className={s.segmented} role="group" aria-label="Idioma / Language">
          <button aria-pressed={locale === "pt"} onClick={() => setLocale("pt")}>PT</button>
          <button aria-pressed={locale === "en"} onClick={() => setLocale("en")}>EN</button>
        </div>
      </nav>

      {mode === "live" && (
        <section className={s.liveCard} aria-live="polite">
          {live === null ? (
            <p className={s.muted}>{L.loadingLive}</p>
          ) : live === "failed" || live.state === "check_failed" ? (
            <>
              <div className={s.stateRow}><span className={s.stateWarn} aria-hidden>!</span><h1>{L.checkFailed}</h1></div>
            </>
          ) : (
            <>
              <div className={s.stateRow}>
                <span className={live.state === "no_active_fires" ? s.stateOk : s.stateWarn} aria-hidden>
                  {live.state === "no_active_fires" ? <svg viewBox="0 0 24 24" width="18" height="18"><path fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" d="m5 12.5 4.2 4.2L19 7" /></svg> : "!"}
                </span>
                <h1>{live.state === "no_active_fires" ? L.noFires : live.state === "active_fires" ? L.activeFires(live.active.length) : L.partial}</h1>
              </div>
              {live.statement && (
                <p className={s.quote}>“{live.statement}”{locale === "en" && live.statement_en ? <span> ({live.statement_en})</span> : null}</p>
              )}
              <p className={s.meta}>
                {L.checked} {lisbon(live.checked_at, locale, true)} ·{" "}
                {live.sources.map((src, i) => (
                  <a key={src.url} href={src.url} target="_blank" rel="noreferrer">{i ? ", " : ""}{src.name.replace(" public incident list", "")}</a>
                ))}{" "}
                {L.via} Context.dev
              </p>
              {live.origin === "snapshot" && <p className={s.snapshot}>{L.snapshot}</p>}
            </>
          )}
          <button className={s.cta} onClick={() => void enterReplay()}>
            {L.watch}
            <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden><path fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" d="M5 12h13m-5-6 6 6-6 6" /></svg>
          </button>
        </section>
      )}

      {mode === "replay" && index && frame && (
        <section className={s.situation}>
          <div className={s.sitHead}>
            <div>
              <h1>Piódão · Arganil</h1>
              <p className={s.muted}>{L.started} {lisbon(index.timeline[0].at, locale, true)} · {index.ignition.nature}</p>
            </div>
            {frame.fogos_status && !beforeReport && <span className={s.status}>{frame.fogos_status.status}</span>}
          </div>
          {beforeReport ? (
            <p className={s.preReport}>{locale === "pt" ? "Ainda sem alerta. A aguardar a primeira ocorrência…" : "No alert yet. Waiting for the first report…"}</p>
          ) : (
            <>
              <div className={s.stats}>
                <div><span>{L.area}</span><strong>{fmt(frame.model.area_ha_p50, locale)}<small> ha</small></strong><em>{fmt(frame.model.area_ha_p10, locale)}–{fmt(frame.model.area_ha_p90, locale)}</em></div>
                <div><span>{L.spread}</span><strong>{frame.model.head_ros_m_min.toFixed(1)}<small> m/min</small></strong><em>{L.model}</em></div>
                {frame.weather && (
                  <div>
                    <span>{L.wind}</span>
                    <strong>
                      <svg className={s.windArrow} style={{ transform: `rotate(${frame.weather.wind_from_deg + 180}deg)` }} viewBox="0 0 24 24" width="15" height="15" aria-hidden><path fill="currentColor" d="M12 3 18 19l-6-3.5L6 19Z" /></svg>
                      {Math.round(frame.weather.wind_mps * 3.6)}<small> km/h</small>
                    </strong>
                    <em>{L.from} {compassPoint(frame.weather.wind_from_deg, locale)}</em>
                  </div>
                )}
              </div>
              {frame.weather && (
                <p className={s.conditions}>
                  {L.temp} <strong>{Math.round(frame.weather.temp_c)} °C</strong>
                  <i />{L.rh} <strong>{Math.round(frame.weather.rh_pct)} %</strong>
                  {frame.weather.gust_mps != null && <><i />{L.gusts} <strong>{Math.round(frame.weather.gust_mps * 3.6)} km/h</strong></>}
                  <span>ERA5</span>
                </p>
              )}
              <ol className={s.steps} aria-label={L.automation}>
                {frame.automation.filter((a) => ["reported", "forecast", "first_alert_dispatch", "corroborated_by_news"].includes(a.step)).slice(0, 4).map((a) => (
                  <li key={a.step + a.at}>
                    <time>{lisbon(a.at, locale)}</time>
                    <span>{L.steps[a.step as keyof typeof L.steps] ?? a.step}</span>
                    <em className={a.basis === "simulated" ? s.tagSim : s.tagObs}>{a.basis === "simulated" ? L.simulated : L.observed}</em>
                  </li>
                ))}
              </ol>
              <div className={s.sitFoot}>
                <span><strong>{evacuated}</strong> {L.evacuated}</span>
                <button onClick={() => setDrawer(true)}><strong>{knownEvidence.length}</strong> {L.facts} · <strong>{publishers}</strong> {L.sources} →</button>
              </div>
            </>
          )}
        </section>
      )}

      {mode === "replay" && toast && (
        <div className={s.toast} role="status">
          <time>{lisbon(toast.at, locale, true)}</time>
          <p>{toast.text}</p>
          <span>{toast.publisher}</span>
        </div>
      )}

      {mode === "replay" && drawer && index && (
        <aside className={s.drawer} aria-label={L.sourcesTitle}>
          <div className={s.drawerHead}>
            <div><h2>{L.sourcesTitle}</h2><p className={s.muted}>{L.sourcesNote}</p></div>
            <button onClick={() => setDrawer(false)} aria-label={L.close}>×</button>
          </div>
          <ol className={s.evidence}>
            {knownEvidence.slice(0, 60).map((e) => (
              <li key={e.id}>
                <div className={s.evMeta}><time>{lisbon(e.known_at, locale, true)}</time><a href={e.url} target="_blank" rel="noreferrer">{e.publisher} ↗</a></div>
                <p>{locale === "pt" ? `“${e.quote}”` : e.statement_en}</p>
              </li>
            ))}
          </ol>
        </aside>
      )}

      {mode === "replay" && index && (
        <footer className={s.timeline}>
          <button className={s.play} onClick={() => {
            if (!playing && t >= Date.parse(index.window.to)) { lastT.current = Date.parse(index.window.from); setT(lastT.current); }
            setPlaying(!playing);
          }} aria-label={playing ? L.pause : L.play}>
            {playing
              ? <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden><path fill="currentColor" d="M7 5h3.5v14H7zM13.5 5H17v14h-3.5z" /></svg>
              : <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden><path fill="currentColor" d="M8 5.5v13l10.5-6.5Z" /></svg>}
          </button>
          <div className={s.track}>
            <div className={s.ticks} aria-hidden>
              {moments.map((m) => (
                <i key={m.at + m.kind} data-kind={m.kind} title={m.text}
                  style={{ left: pct(Date.parse(m.at), index) + "%" }} />
              ))}
            </div>
            <input type="range" aria-label={L.timeline}
              min={Date.parse(index.window.from)} max={Date.parse(index.window.to)} step={60_000} value={t}
              style={{ "--p": pct(t, index) + "%" } as React.CSSProperties}
              onChange={(e) => { setPlaying(false); lastT.current = Number(e.target.value); setT(lastT.current); }} />
            <div className={s.trackLabels}>
              <span>{lisbon(index.window.from, locale, true)}</span>
              <span>{lisbon(index.window.to, locale, true)}</span>
            </div>
          </div>
        </footer>
      )}

      {mode === "replay" && (
        <div className={s.legend}>
          <span><i style={{ background: `linear-gradient(90deg, ${FIRE.p10}, ${FIRE.p50}, ${FIRE.p90})` }} />{L.legendBurned}</span>
          <span><i className={s.legendDash} />{L.legendForecast}</span>
          <span><i style={{ background: FIRE.smoke, opacity: 0.6 }} />{L.legendSmoke}</span>
          <span><i className={s.legendDot} />{L.legendVillage}</span>
        </div>
      )}

      {mapFailed && <div className={s.mapFailed} role="alert">{L.mapFailed}</div>}
    </main>
  );
}

const fmt = (n: number, locale: Locale) => new Intl.NumberFormat(locale === "pt" ? "pt-PT" : "en-GB", { maximumFractionDigits: 0 }).format(n);
const pct = (t: number, index: ReplayIndex) => {
  const a = Date.parse(index.window.from), b = Date.parse(index.window.to);
  return Math.max(0, Math.min(100, ((t - a) / (b - a)) * 100));
};
