"use client";
import { useCallback, useEffect, useState } from "react";
import Link from "next/link";
import MapView from "./MapView";
import FirecastPanel from "./FirecastPanel";
import HistoricalView from "./HistoricalView";
import { firecastFor, isFirecast } from "@/lib/firecast";
import Icon from "./Icon";
import { useWorkspace } from "@/lib/use-workspace";
import { copy, healthLabel, label, type Locale } from "@/lib/i18n";
import { coverageMessage, displayTime, safeUrl } from "@/lib/model";
import type { Evidence, Incident, Snapshot } from "@/lib/contracts";
type Tab = "overview" | "sources" | "fleet" | "inbox";
export default function Workspace() {
  const { snapshot, connection, error, busy, command, resync, dispatch, forecast } =
    useWorkspace();
  const [locale] = useState<Locale>("en"),
    [tab, setTab] = useState<Tab>("overview"),
    [selectedId, setSelectedId] = useState<string | null>(null),
    [view, setView] = useState<"workspace" | "historical">("workspace");
  const select = useCallback((id: string) => {
    setSelectedId(id);
    setTab("overview");
  }, []);
  const t = copy[locale];
  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);
  if (!snapshot)
    return (
      <main className="loading-screen">
        <div className="brand">
          <span className="brand-mark">
            <Icon name="flame" size={28} />
          </span>
          <span>Brasa Scout</span>
        </div>
        <p>{error ? t.notLoaded : t.loading}</p>
        {error && (
          <button
            className="primary-button"
            onClick={() => window.location.reload()}
          >
            {t.retry}
          </button>
        )}
      </main>
    );
  const scenario = snapshot.scenario,
    selected =
      snapshot.incidents.find((i) => i.id === selectedId) ??
      snapshot.incidents[0];
  const coverage = coverageMessage(snapshot);
  const progress =
    ((Date.parse(scenario.current_at) - Date.parse(scenario.start_at)) /
      (Date.parse(scenario.end_at) - Date.parse(scenario.start_at))) *
    100;
  const mode = t[scenario.mode],
    connected = connection === "connected" || connection === "local";
  const tabs = [
    ["overview", "map"],
    ["sources", "layers"],
    ["fleet", "plane"],
    ["inbox", "message"],
  ] as const;
  return (
    <main className="workspace">
      <header className="topbar">
        <Link className="brand" href="/" aria-label="Brasa Scout">
          <span className="brand-mark">
            <Icon name="flame" size={27} />
          </span>
          <span>
            Brasa Scout<small>{t.subtitle}</small>
          </span>
        </Link>
        <nav
          className="main-nav"
          aria-label={
            locale === "pt" ? "Navegação principal" : "Main navigation"
          }
        >
          {tabs.map(([key, icon]) => (
            <button
              key={key}
              onClick={() => setTab(key)}
              className={tab === key ? "active" : ""}
              aria-current={tab === key ? "page" : undefined}
            >
              <Icon name={icon} size={17} />
              {t[key]}
              {key === "sources" && (
                <span className="nav-count">{snapshot.sources.length}</span>
              )}
            </button>
          ))}
        </nav>
        <div className="topbar-end">
          <button
            className={"history-switch" + (view === "historical" ? " active" : "")}
            aria-pressed={view === "historical"}
            onClick={() => setView(view === "historical" ? "workspace" : "historical")}
          >
            <Icon name="reset" size={14} />
            {locale === "pt" ? "Arganil 2025 (histórico)" : "Arganil 2025 (historical)"}
          </button>
          <Link href="/terrain" className="terrain-workspace-link">
            <Icon name="layers" size={16} />
            {locale === "pt" ? "Terreno 3D" : "3D terrain"}
          </Link>
          <div
            className={"connection " + (!connected ? "connection-warning" : "")}
          >
            <i />
            {t[connection]}
          </div>

        </div>
      </header>
      {view === "historical" ? (
        <HistoricalView locale={locale} onExit={() => setView("workspace")} />
      ) : (
      <>
      <div className={"mode-banner mode-" + scenario.mode}>
        <span className="mode-pill">
          <i />
          {mode}
        </span>
        <span>
          {scenario.mode === "synthetic"
            ? t.syntheticNote
            : scenario.mode === "historical"
              ? t.historicalNote
              : t.liveNote}
        </span>
        <span className="mode-time">
          {displayTime(scenario.current_at, locale, true)}{" "}
          <small>Europe/Lisbon</small>
        </span>
      </div>
      {error && (
        <div className="error-banner" role="alert">
          <Icon name="warning" size={16} />
          {error === "command" ? t.commandError : t.connectionError}
          <button onClick={() => void resync()}>{t.retry}</button>
        </div>
      )}
      <section className="work-area">
        <aside className="incident-sidebar">
          <div className="sidebar-heading">
            <h2>{t.reports}</h2>
            <span className="count-label">{snapshot.incidents.length}</span>
          </div>
          <p className="sidebar-subtitle">{t.demoCount}</p>
          <div className="incident-list">
            {snapshot.incidents.length ? (
              snapshot.incidents.map((incident, index) => (
                <button
                  key={incident.id}
                  className={
                    "incident-item " +
                    (selected?.id === incident.id ? "selected" : "")
                  }
                  onClick={() => select(incident.id)}
                >
                  <span className="incident-symbol">
                    <Icon name="flame" size={19} />
                  </span>
                  <span className="incident-copy">
                    <span className="incident-number">
                      {t.report} {String(index + 1).padStart(2, "0")}
                    </span>
                    <strong>{incident.location_name ?? t.noLocation}</strong>
                    <span className="incident-state">
                      <i />
                      {label(incident.status, locale)}
                    </span>
                  </span>
                  <Icon name="arrow" size={15} />
                </button>
              ))
            ) : (
              <div className="empty-small">
                <Icon name="pin" />
                <p>{t.noIncident}</p>
              </div>
            )}
          </div>
          <div className="sidebar-bottom">
            <div className="coverage-label">
              <span>{t.coverage}</span>
              <strong className={"coverage-" + coverage}>{t[coverage]}</strong>
            </div>
            <p>{t.sourceNote}</p>
            <div className="source-mini-list">
              {snapshot.sources.map((source) => (
                <button key={source.id} onClick={() => setTab("sources")}>
                  <i className={"health-dot " + source.health} />
                  <span>
                    {source.name.replace(
                      " public-page fixture",
                      locale === "pt" ? " de teste" : " fixture",
                    )}
                  </span>
                  <Icon name="arrow" size={13} />
                </button>
              ))}
            </div>
          </div>
        </aside>
        <MapView
          snapshot={snapshot}
          selectedId={selectedId}
          onSelect={select}
          locale={locale}
        />
        <aside className="detail-panel">
          {tab === "overview" && (
            <IncidentDetails
              incident={selected}
              snapshot={snapshot}
              locale={locale}
              onForecast={
                forecast && selected ? () => void forecast(selected.id) : undefined
              }
              busy={busy}
            />
          )}
          {tab === "sources" && <Sources snapshot={snapshot} locale={locale} />}
          {tab === "fleet" && (
            <Response
              snapshot={snapshot}
              locale={locale}
              onDispatch={
                dispatch
                  ? () => {
                      if (selected) void dispatch(selected.id);
                    }
                  : undefined
              }
              busy={busy}
            />
          )}
          {tab === "inbox" && <Briefings snapshot={snapshot} locale={locale} />}
        </aside>
      </section>
      <footer className="replay-bar">
        <div className="replay-title">
          <span className="replay-icon">
            <Icon name="reset" size={20} />
          </span>
          <span>
            <strong>{t.replay}</strong>
            <small>{scenario.name}</small>
          </span>
          <span className="replay-mode">{mode}</span>
        </div>
        <div className="transport">
          <button
            className="play-button"
            disabled={busy || scenario.mode === "live"}
            aria-label={scenario.playing ? t.pause : t.play}
            onClick={() => void command(scenario.playing ? "pause" : "play")}
          >
            <Icon name={scenario.playing ? "pause" : "play"} size={20} />
            <span>{scenario.playing ? t.pause : t.play}</span>
          </button>
          <button
            className="reset-button"
            disabled={busy || scenario.mode === "live"}
            onClick={() => void command("reset")}
            aria-label={t.reset}
          >
            <Icon name="reset" size={18} />
          </button>
          <div className="time-slider">
            <div className="time-labels">
              <span>{displayTime(scenario.start_at, locale)}</span>
              <strong>{displayTime(scenario.current_at, locale, true)}</strong>
              <span>{displayTime(scenario.end_at, locale)}</span>
            </div>
            <input
              aria-label={t.seek}
              type="range"
              min="0"
              max="100"
              step="0.25"
              value={Number.isFinite(progress) ? progress : 0}
              disabled={busy || scenario.mode === "live"}
              onChange={(event) => {
                const time =
                  Date.parse(scenario.start_at) +
                  ((Date.parse(scenario.end_at) -
                    Date.parse(scenario.start_at)) *
                    Number(event.target.value)) /
                    100;
                void command("seek", new Date(time).toISOString());
              }}
              style={{ "--progress": progress + "%" } as React.CSSProperties}
            />
          </div>
          <div className="replay-speed">
            <strong>{scenario.speed}×</strong>
            <small>{t.clock}</small>
          </div>
        </div>
        <p className="replay-note">{t.replayNote}</p>
      </footer>
      </>
      )}
    </main>
  );
}
function SectionHeader({
  icon,
  title,
  small,
}: {
  icon: "flame" | "layers" | "plane" | "message";
  title: string;
  small?: string;
}) {
  return (
    <div className="panel-heading">
      <span className="panel-icon">
        <Icon name={icon} size={18} />
      </span>
      <div>
        <h2>{title}</h2>
        {small && <p>{small}</p>}
      </div>
    </div>
  );
}
function SourceLink({
  evidence,
  snapshot,
  locale,
}: {
  evidence: Evidence;
  snapshot: Snapshot;
  locale: Locale;
}) {
  const source = snapshot.sources.find(
      (s) => s.id === evidence.provenance.source_id,
    ),
    url = safeUrl(evidence.provenance.source_url),
    t = copy[locale];
  return (
    <a className="source-link" href={url} target="_blank" rel="noreferrer">
      {t.source}: {source?.name ?? evidence.provenance.source_id}
      <Icon name="external" size={13} />
      {source?.via && <small>via {source.via}</small>}
    </a>
  );
}
function IncidentDetails({
  incident,
  snapshot,
  locale,
  onForecast,
  busy,
}: {
  incident: Incident | undefined;
  snapshot: Snapshot;
  locale: Locale;
  onForecast?: () => void;
  busy: boolean;
}) {
  const t = copy[locale],
    evidence = snapshot.evidence.filter((e) =>
      incident?.evidence_ids.includes(e.id),
    ),
    weather = snapshot.weather[0];
  return (
    <>
      <SectionHeader
        icon="flame"
        title={t.detail}
        small={incident?.mode === "synthetic" ? t.synthetic : undefined}
      />
      {incident ? (
        <div className="incident-details">
          <div className="report-status">
            {label(incident.evidence_level, locale)}
          </div>
          <h2>{incident.location_name ?? incident.title}</h2>
          <p className="incident-description">{incident.title}</p>
          <div className="data-grid">
            <div>
              <span>{t.status}</span>
              <strong>{label(incident.status, locale)}</strong>
            </div>
            <div>
              <span>{t.origins}</span>
              <strong>{incident.origin_groups.length}</strong>
            </div>
            <div>
              <span>{t.uncertainty}</span>
              <strong>
                {incident.uncertainty_m !== null
                  ? "± " +
                    new Intl.NumberFormat(locale).format(
                      incident.uncertainty_m,
                    ) +
                    " m"
                  : t.unknown}
              </strong>
            </div>
            <div>
              <span>{t.eventTime}</span>
              <strong>{displayTime(incident.first_event_at, locale)}</strong>
            </div>
          </div>
          {evidence[0] && (
            <div className="attribution-block">
              <SourceLink
                evidence={evidence[0]}
                snapshot={snapshot}
                locale={locale}
              />
              {incident.mode === "synthetic" && (
                <span>{t.syntheticSource}</span>
              )}
            </div>
          )}
          <div className="weather-strip">
            <Icon name="wind" size={19} />
            {weather ? (
              <span>
                {t.wind}: {weather.wind_speed_mps ?? "—"} m/s{" "}
                <small>
                  {displayTime(weather.valid_at, locale)} ·{" "}
                  {weather.forecast
                    ? locale === "pt"
                      ? "previsão"
                      : "forecast"
                    : locale === "pt"
                      ? "observação"
                      : "observation"}
                </small>
              </span>
            ) : (
              <span>
                {t.noWeather}
                <small>
                  {locale === "pt"
                    ? "Os valores em falta permanecem desconhecidos."
                    : "Missing values remain unknown."}
                </small>
              </span>
            )}
          </div>
        </div>
      ) : null}
      {incident && (
        <FirecastPanel
          forecast={firecastFor(snapshot, incident.id)}
          locale={locale}
          onRun={onForecast}
          busy={busy}
        />
      )}
      {!incident && (
        <div className="empty-state">
          <Icon name="pin" size={28} />
          <h3>{t.noIncident}</h3>
          <p>{t.noIncidentNote}</p>
        </div>
      )}
      <div className="timeline-heading">
        <h3>{t.timeline}</h3>
        <span>{snapshot.evidence.length}</span>
      </div>
      <ol className="evidence-timeline">
        {[...snapshot.evidence]
          .sort(
            (a, b) =>
              Date.parse(a.provenance.available_at) -
              Date.parse(b.provenance.available_at),
          )
          .map((e) => (
            <li key={e.id}>
              <div className="timeline-time">
                <span>{displayTime(e.provenance.available_at, locale)}</span>
                <i />
              </div>
              <article>
                <span className="evidence-kind">
                  {e.mode === "synthetic"
                    ? t.contextFixture
                    : e.kind.replaceAll("_", " ")}
                </span>
                <h4>{e.title}</h4>
                {e.body && <p>{e.body}</p>}
                <SourceLink evidence={e} snapshot={snapshot} locale={locale} />
                <dl className="provenance-list">
                  <div>
                    <dt>{t.eventTime}</dt>
                    <dd>{displayTime(e.provenance.event_at, locale)}</dd>
                  </div>
                  <div>
                    <dt>{t.captured}</dt>
                    <dd>
                      {displayTime(e.provenance.captured_at, locale, true)}
                    </dd>
                  </div>
                  <div>
                    <dt>{t.lineage}</dt>
                    <dd>{e.provenance.origin_group}</dd>
                  </div>
                </dl>
                {e.provenance.quality_flags.length > 0 && (
                  <div className="quality-flags">
                    {e.provenance.quality_flags.map((flag) => (
                      <span key={flag}>{flag.replaceAll("_", " ")}</span>
                    ))}
                  </div>
                )}
              </article>
            </li>
          ))}
      </ol>
    </>
  );
}
function Sources({ snapshot, locale }: { snapshot: Snapshot; locale: Locale }) {
  const t = copy[locale];
  return (
    <>
      <SectionHeader icon="layers" title={t.sourceHealth} />
      <p className="panel-intro">{t.sourceNote}</p>
      <div className="sources-list">
        {snapshot.sources.map((source) => (
          <article key={source.id} className="source-card">
            <div className="source-card-heading">
              <h3>{source.name}</h3>
              <span className={"health-pill " + source.health}>
                <i />
                {healthLabel[source.health]?.[locale] ?? source.health}
              </span>
            </div>
            <a
              href={safeUrl(source.source_url)}
              target="_blank"
              rel="noreferrer"
              className="source-link"
            >
              {source.attribution}
              <Icon name="external" size={12} />
            </a>
            {source.via && <p className="source-via">via {source.via}</p>}
            <dl className="provenance-list">
              <div>
                <dt>{t.dataTime}</dt>
                <dd>{displayTime(source.data_at, locale, true)}</dd>
              </div>
              <div>
                <dt>{t.captured}</dt>
                <dd>{displayTime(source.captured_at, locale, true)}</dd>
              </div>
              <div>
                <dt>{t.coverage}</dt>
                <dd>{source.coverage.complete ? t.verified : t.partial}</dd>
              </div>
            </dl>
            <p className="coverage-scope">{source.coverage.scope}</p>
            {[...source.coverage.notes, ...source.notes].map((note, i) => (
              <p key={i} className="source-note">
                {note}
              </p>
            ))}
          </article>
        ))}
      </div>
    </>
  );
}
function Response({
  snapshot,
  locale,
  onDispatch,
  busy,
}: {
  snapshot: Snapshot;
  locale: Locale;
  onDispatch?: () => void;
  busy: boolean;
}) {
  const t = copy[locale];
  return (
    <>
      <SectionHeader icon="plane" title={t.fleet} small={t.simulation} />
      <div className="response-content">
        <button
          className="dispatch-button"
          disabled={
            !onDispatch ||
            busy ||
            !snapshot.incidents.length ||
            snapshot.scenario.mode !== "synthetic"
          }
          onClick={onDispatch}
        >
          <Icon name="plane" size={16} />
          {locale === "pt" ? "Simular missão" : "Simulate mission"}
        </button>
        {!onDispatch && (
          <p className="source-note">
            {locale === "pt"
              ? "A simulação de missões requer ligação ao serviço."
              : "Mission simulation requires a connection to the service."}
          </p>
        )}
        <h3>{t.spread}</h3>
        {snapshot.forecasts.some(isFirecast) && (
          <p className="source-note firecast-pointer">
            {locale === "pt"
              ? "A previsão física de propagação e fumo está no separador Visão geral. As missões evitam a área prevista do incêndio e o fumo."
              : "The physics spread and smoke forecast is in the Overview tab. Missions avoid the predicted fire area and smoke."}
          </p>
        )}
        {snapshot.forecasts.some((f) => !isFirecast(f)) ? (
          snapshot.forecasts.filter((f) => !isFirecast(f)).map((f) => (
            <article className="output-card" key={f.id}>
              <span className="simulation-label">{t.simulation}</span>
              <h4>
                {t.model}: {f.model_version}
              </h4>
              <p>
                {locale === "pt" ? "Horizonte" : "Horizon"}:{" "}
                {Math.round(f.horizon_seconds / 60)} min
              </p>
              {f.limitations.map((note) => (
                <p key={note} className="source-note">
                  {note}
                </p>
              ))}
            </article>
          ))
        ) : snapshot.forecasts.length ? null : (
          <div className="empty-output">
            <Icon name="layers" size={25} />
            <strong>{t.noForecast}</strong>
            <p>{t.noForecastNote}</p>
          </div>
        )}
        <h3>{t.drone}</h3>
        {snapshot.drone_plans.length ? (
          snapshot.drone_plans.map((p) => (
            <article className="output-card" key={p.id}>
              <span className="simulation-label">{t.simulation}</span>
              <dl className="provenance-list">
                <div>
                  <dt>{t.reserve}</dt>
                  <dd>{p.reserve_pct.toFixed(0)}%</dd>
                </div>
                <div>
                  <dt>{t.routeTime}</dt>
                  <dd>{Math.round(p.duration_seconds / 60)} min</dd>
                </div>
              </dl>
              {p.reasons.map((reason) => (
                <p key={reason}>{reason}</p>
              ))}
            </article>
          ))
        ) : (
          <div className="empty-output">
            <Icon name="plane" size={25} />
            <strong>{t.noDrones}</strong>
            <p>{t.noDronesNote}</p>
          </div>
        )}
      </div>
    </>
  );
}
function Briefings({
  snapshot,
  locale,
}: {
  snapshot: Snapshot;
  locale: Locale;
}) {
  const t = copy[locale];
  return (
    <>
      <SectionHeader icon="message" title={t.inbox} small={t.simulation} />
      {snapshot.briefings.length ? (
        <div className="briefings-list">
          {snapshot.briefings.map((b) => (
            <article className="briefing-card" key={b.id}>
              <span className="simulation-label">{t.simulation}</span>
              <time>{displayTime(b.generated_at, locale, true)}</time>
              <p>{locale === "pt" ? b.text_pt : b.text_en}</p>
              <h4>{t.citations}</h4>
              {b.citations.map((c) => (
                <a
                  key={c}
                  href={safeUrl(c)}
                  target="_blank"
                  rel="noreferrer"
                  className="source-link"
                >
                  {c}
                  <Icon name="external" size={12} />
                </a>
              ))}
            </article>
          ))}
        </div>
      ) : (
        <div className="empty-state">
          <Icon name="message" size={30} />
          <h3>{t.noBriefing}</h3>
          <p>{t.noBriefingNote}</p>
        </div>
      )}
    </>
  );
}
