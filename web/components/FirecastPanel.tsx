"use client";
import type { Forecast } from "@/lib/contracts";
import type { Locale } from "@/lib/i18n";
import { compass, summaryOf } from "@/lib/firecast";
import { displayTime, safeUrl } from "@/lib/model";
import Icon from "./Icon";

const L = {
  pt: {
    title: "Previsão de propagação e fumo",
    physics: "Modelo físico · não operacional",
    run: "Calcular previsão",
    rerun: "Recalcular",
    running: "A calcular…",
    none: "Ainda sem previsão física para este incidente.",
    noneNote: "O serviço calcula automaticamente quando um incidente é detetado.",
    offline: "A previsão requer ligação ao serviço.",
    head: "Cabeça do incêndio",
    ros: "Velocidade",
    toward: "Direção",
    intensity: "Intensidade",
    flame: "Chama",
    fireType: { surface: "superfície", "passive crown (torching)": "copas passivo (archotes)", "active crown": "copas ativo" } as Record<string, string>,
    area: "Área prevista (mediana, 10–90 %)",
    now: "Agora",
    conditions: "Condições no ponto de ignição",
    wind10: "Vento 10 m",
    wind120: "Vento 120 m",
    gust: "rajada",
    tempRh: "Temp. / HR",
    fuelMoisture: "Humidade do combustível morto (1 h)",
    live: "Vivo",
    fuel: "Combustível",
    slope: "Declive",
    smoke: "Fumo (PM2.5)",
    smokeTo: "Dirige-se para",
    reach: "Alcance ≥ 35 µg/m³",
    stability: "Estabilidade",
    plume: "Coluna",
    web: "Evidência da web (Context.dev)",
    agreement: "Modelo vs. relatos",
    reported: "relatado",
    modelled: "modelo",
    regional: "Contexto regional",
    drought: "Seca",
    vegetation: "Vegetação",
    places: "Locais ameaçados",
    barriers: "Barreiras / linhas de contenção",
    noIntel: "Sem chamadas Context.dev nesta execução.",
    warnings: "Avisos IPMA",
    inputs: "Dados de entrada",
    limits: "Limitações",
    refs: "Referências científicas",
    basis: { live: "Dados ao vivo", historical: "Histórico", what_if: "Cenário hipotético: meteorologia real atual", synthetic: "Entradas sintéticas" },
    edge: "O incêndio atingiu o limite do domínio do modelo.",
    members: "membros do ensemble",
  },
  en: {
    title: "Spread and smoke forecast",
    physics: "Physics model · not operational",
    run: "Run forecast",
    rerun: "Re-run",
    running: "Computing…",
    none: "No physics forecast for this incident yet.",
    noneNote: "The service forecasts automatically when an incident is detected.",
    offline: "Forecasting requires a connection to the service.",
    head: "Head fire",
    ros: "Rate of spread",
    toward: "Heading",
    intensity: "Intensity",
    flame: "Flame",
    fireType: { surface: "surface", "passive crown (torching)": "passive crown (torching)", "active crown": "active crown" } as Record<string, string>,
    area: "Predicted area (median, 10–90 %)",
    now: "Now",
    conditions: "Conditions at the ignition point",
    wind10: "10 m wind",
    wind120: "120 m wind",
    gust: "gust",
    tempRh: "Temp. / RH",
    fuelMoisture: "Dead fuel moisture (1 h)",
    live: "Live",
    fuel: "Fuel",
    slope: "Slope",
    smoke: "Smoke (PM2.5)",
    smokeTo: "Drifting toward",
    reach: "Reach ≥ 35 µg/m³",
    stability: "Stability",
    plume: "Plume",
    web: "Web evidence (Context.dev)",
    agreement: "Model vs reports",
    reported: "reported",
    modelled: "model",
    regional: "Regional context",
    drought: "Drought",
    vegetation: "Vegetation",
    places: "Places threatened",
    barriers: "Barriers / containment lines",
    noIntel: "No Context.dev calls in this run.",
    warnings: "IPMA warnings",
    inputs: "Inputs",
    limits: "Limitations",
    refs: "Scientific references",
    basis: { live: "Live data", historical: "Historical", what_if: "What-if: today's real weather", synthetic: "Synthetic inputs" },
    edge: "Fire reached the model domain edge.",
    members: "ensemble members",
  },
};

const SUPPRESSION_PT: Record<string, string> = {
  "hand crews can attack the head": "equipas apeadas podem atacar a cabeça",
  "too intense for hand crews; machinery and aircraft needed": "demasiado intenso para equipas apeadas; necessárias máquinas e meios aéreos",
  "control efforts at the head likely ineffective": "combate à cabeça provavelmente ineficaz",
  "crowning, spotting and major runs likely; head attack ineffective": "fogo de copas, projeções e grandes corridas prováveis; ataque à cabeça ineficaz",
};

const fmt = (v: number | null | undefined, digits = 0, unit = "") =>
  v === null || v === undefined || !Number.isFinite(v) ? "—" : v.toFixed(digits) + unit;

export default function FirecastPanel({
  forecast,
  locale,
  onRun,
  busy,
}: {
  forecast: Forecast | undefined;
  locale: Locale;
  onRun?: () => void;
  busy: boolean;
}) {
  const l = L[locale];
  const s = summaryOf(forecast);
  const action = (
    <button className="forecast-run" disabled={!onRun || busy} onClick={onRun}>
      <Icon name="target" size={14} />
      {busy ? l.running : s ? l.rerun : l.run}
    </button>
  );
  if (!s || !forecast)
    return (
      <section className="firecast-panel">
        <header className="firecast-header">
          <h3>{l.title}</h3>
          {action}
        </header>
        <p className="firecast-empty">{l.none}</p>
        <p className="source-note">{onRun ? l.noneNote : l.offline}</p>
      </section>
    );
  const smoke1 = s.smoke.find((x) => x.hour_offset === 1) ?? s.smoke[0];
  const reach35 = smoke1?.reach.find((r) => r.threshold_ug_m3 === 35)?.reach_km;
  const adj = s.intel.adjustments;
  const agreement = s.intel.agreement;
  return (
    <section className="firecast-panel">
      <header className="firecast-header">
        <div>
          <h3>{l.title}</h3>
          <span className={"basis-badge basis-" + s.basis}>{l.basis[s.basis]}</span>
        </div>
        {action}
      </header>
      <p className="firecast-meta">
        {l.physics} · {s.members} {l.members} · {displayTime(s.now, locale, true)}
      </p>

      {s.head && (
        <div className="firecast-head">
          <h4>
            {l.head}
            {s.head.fire_type && (
              <span className={"fire-type fire-type-" + (s.head.crown_fraction && s.head.crown_fraction > 0 ? "crown" : "surface")}>
                {l.fireType[s.head.fire_type] ?? s.head.fire_type}
              </span>
            )}
          </h4>
          <div className="metric-grid">
            <div>
              <span>{l.ros}</span>
              <strong>{fmt(s.head.ros_m_min, 1, " m/min")}</strong>
              <small>{fmt((s.head.ros_m_min * 60) / 1000, 2, " km/h")}</small>
            </div>
            <div>
              <span>{l.toward}</span>
              <strong>{compass(s.head.bearing_deg, locale)}</strong>
              <small>{fmt(s.head.bearing_deg, 0, "°")}</small>
            </div>
            <div>
              <span>{l.intensity}</span>
              <strong>{fmt(s.head.intensity_kw_m, 0, " kW/m")}</strong>
              <small>{locale === "pt" ? (SUPPRESSION_PT[s.head.suppression] ?? s.head.suppression) : s.head.suppression}</small>
            </div>
            <div>
              <span>{l.flame}</span>
              <strong>{fmt(s.head.flame_length_m, 1, " m")}</strong>
              <small>L/B {fmt(s.head.length_to_breadth, 2)}</small>
            </div>
          </div>
        </div>
      )}

      <h4>{l.area}</h4>
      <table className="horizon-table">
        <tbody>
          {s.horizons.map((h) => (
            <tr key={h.horizon_h}>
              <th>{h.horizon_h === 0 ? l.now : "+" + h.horizon_h + " h"}</th>
              <td>
                <strong>{fmt(h.area_ha_p50, 0, " ha")}</strong>
              </td>
              <td className="range">
                {fmt(h.area_ha_p10, 0)}–{fmt(h.area_ha_p90, 0, " ha")}
              </td>
              <td className="range">
                {h.run_bearing_deg !== null ? compass(h.run_bearing_deg, locale) + " · " + fmt(h.max_run_km_p50, 1, " km") : ""}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {s.edge_reached && <p className="source-note">{l.edge}</p>}

      <h4>{l.conditions}</h4>
      <dl className="provenance-list firecast-list">
        <div>
          <dt>{l.wind10}</dt>
          <dd>
            {fmt(s.weather.wind_mps, 1, " m/s")} {compass(s.weather.wind_from_deg, locale)}
            {s.weather.gust_mps !== null && " · " + l.gust + " " + fmt(s.weather.gust_mps, 1, " m/s")}
          </dd>
        </div>
        <div>
          <dt>{l.wind120}</dt>
          <dd>
            {fmt(s.weather.wind_120m_mps, 1, " m/s")} {compass(s.weather.wind_120m_from_deg, locale)}
          </dd>
        </div>
        <div>
          <dt>{l.tempRh}</dt>
          <dd>
            {fmt(s.weather.temp_c, 1, " °C")} / {fmt(s.weather.rh_pct, 0, " %")}
          </dd>
        </div>
        <div>
          <dt>{l.fuelMoisture}</dt>
          <dd>
            {fmt(s.moisture.dead_1h_pct, 1, " %")} · {l.live} {fmt(s.moisture.live_pct, 0, " %")}
          </dd>
        </div>
        <div>
          <dt>{l.fuel}</dt>
          <dd>
            {s.ignition.fuel} {s.ignition.fuel_model ? "(" + s.ignition.fuel_model + ")" : ""}
          </dd>
        </div>
        <div>
          <dt>{l.slope}</dt>
          <dd>
            {fmt(s.ignition.slope_deg, 1, "°")} · {fmt(s.ignition.elevation_m, 0, " m")}
          </dd>
        </div>
      </dl>

      {smoke1 && (
        <>
          <h4>{l.smoke}</h4>
          <dl className="provenance-list firecast-list">
            <div>
              <dt>{l.smokeTo}</dt>
              <dd>
                {compass(smoke1.transport_toward_deg, locale)} · {fmt(smoke1.transport_mps, 1, " m/s")}
              </dd>
            </div>
            <div>
              <dt>{l.reach}</dt>
              <dd>{fmt(reach35, 1, " km")}</dd>
            </div>
            <div>
              <dt>{l.stability}</dt>
              <dd>{smoke1.stability}</dd>
            </div>
            <div>
              <dt>{l.plume}</dt>
              <dd>
                {fmt(smoke1.effective_height_m, 0, " m")} · {fmt(smoke1.heat_release_mw, 0, " MW")}
              </dd>
            </div>
          </dl>
        </>
      )}

      <h4>{l.web}</h4>
      {agreement && (
        <div className={"agreement agreement-" + (agreement.delta_deg <= 30 ? "good" : agreement.delta_deg <= 60 ? "fair" : "poor")}>
          <strong>{l.agreement}</strong>
          <span>
            {l.reported} {compass(agreement.reported_bearing_deg, locale)} · {l.modelled}{" "}
            {compass(agreement.model_run_bearing_deg, locale)} · Δ {fmt(agreement.delta_deg, 0, "°")} — {agreement.verdict}
          </span>
          {agreement.evidence && <q>{agreement.evidence}</q>}
        </div>
      )}
      {s.intel.calls.length === 0 && <p className="source-note">{l.noIntel}</p>}
      {s.intel.reports.slice(0, 6).map((r) => (
        <a key={r.url} className="source-link" href={safeUrl(r.url)} target="_blank" rel="noreferrer">
          {r.title || r.url}
          <Icon name="external" size={12} />
        </a>
      ))}
      {s.intel.regional && (
        <dl className="provenance-list firecast-list">
          {s.intel.regional.drought_status && (
            <div>
              <dt>{l.drought}</dt>
              <dd>{s.intel.regional.drought_status}</dd>
            </div>
          )}
          {s.intel.regional.dominant_vegetation.length > 0 && (
            <div>
              <dt>{l.vegetation}</dt>
              <dd>{s.intel.regional.dominant_vegetation.join(", ")}</dd>
            </div>
          )}
        </dl>
      )}
      {adj.threatened_places.length > 0 && (
        <p className="intel-list">
          <strong>{l.places}:</strong> {adj.threatened_places.map((p) => p.value).join(", ")}
        </p>
      )}
      {adj.barriers.length > 0 && (
        <p className="intel-list">
          <strong>{l.barriers}:</strong> {adj.barriers.map((p) => p.value).join(", ")}
        </p>
      )}
      {s.warnings.length > 0 && (
        <p className="intel-list">
          <strong>{l.warnings}:</strong> {s.warnings.map((w) => w.kind + " (" + w.level + ")").join(", ")}
        </p>
      )}

      <details className="firecast-details">
        <summary>{l.inputs}</summary>
        <ul className="input-list">
          {s.inputs.map((i, k) => (
            <li key={k} className={"input-" + i.status}>
              <span className="input-status">{i.status}</span>
              <div>
                <strong>
                  {i.source}
                  {i.via ? " · via " + i.via : ""}
                </strong>
                <p>{i.detail}</p>
              </div>
            </li>
          ))}
        </ul>
        <ul className="basis-list">
          {s.moisture.basis.map((b) => (
            <li key={b}>{b}</li>
          ))}
        </ul>
      </details>
      <details className="firecast-details">
        <summary>{l.limits}</summary>
        {forecast.limitations.map((x) => (
          <p key={x} className="source-note">
            {x}
          </p>
        ))}
      </details>
      <details className="firecast-details">
        <summary>{l.refs}</summary>
        <ul className="basis-list">
          {s.references.map((r) => (
            <li key={r}>{r}</li>
          ))}
        </ul>
        <p className="source-note">{s.model}</p>
      </details>
    </section>
  );
}
