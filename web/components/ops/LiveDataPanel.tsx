"use client";
import { useLiveStatus } from "@/lib/use-live-status";
import { lisbon } from "@/lib/replay";
import type { Locale } from "./SiteNav";
import s from "./ops.module.css";

const COPY = {
  en: {
    title: "Live incoming data", connected: "Service connected", unavailable: "Connection unavailable", checking: "Connecting…",
    refresh: "Refresh now", refreshing: "Refreshing…", cadence: "Refreshes every 60 seconds. Source capture times are shown separately from dashboard refreshes.",
    noFires: "The source positively reported no active incidents.", active: "Active incidents returned by the source", partial: "Partial capture: the current situation is not confirmed.",
    failed: "The source check did not succeed. An empty result does not mean there are no fires.", offline: "The live service is unavailable. Recorded replay data is not shown as live data.",
    checked: "Source checked", received: "Dashboard received", sources: "Source captures", captured: "Captured", lastGood: "Last successful capture", result: "Result", credits: "Credits", request: "Request ID",
    success: "Capture succeeded", captureFailed: "Capture failed", partialCapture: "Partial capture", unknown: "Unknown", empty: "Positive empty result", raw: "Latest normalized service response", incident: "Incident", place: "Place", status: "Status",
    stale: "Last received result; connection is currently unavailable.",
  },
  pt: {
    title: "Dados recebidos ao vivo", connected: "Serviço ligado", unavailable: "Ligação indisponível", checking: "A ligar…",
    refresh: "Atualizar agora", refreshing: "A atualizar…", cadence: "Atualização a cada 60 segundos. A hora de captura da fonte é distinta da atualização do painel.",
    noFires: "A fonte indicou explicitamente que não há ocorrências ativas.", active: "Ocorrências ativas devolvidas pela fonte", partial: "Captura parcial: a situação atual não está confirmada.",
    failed: "A verificação da fonte falhou. Um resultado vazio não significa que não há incêndios.", offline: "O serviço ao vivo está indisponível. Os dados da reprodução não são apresentados como dados ao vivo.",
    checked: "Fonte verificada", received: "Recebido no painel", sources: "Capturas das fontes", captured: "Capturado", lastGood: "Última captura bem-sucedida", result: "Resultado", credits: "Créditos", request: "ID do pedido",
    success: "Captura bem-sucedida", captureFailed: "Captura falhada", partialCapture: "Captura parcial", unknown: "Desconhecido", empty: "Resultado vazio confirmado", raw: "Última resposta normalizada do serviço", incident: "Ocorrência", place: "Local", status: "Estado",
    stale: "Último resultado recebido; a ligação está indisponível.",
  },
};

export default function LiveDataPanel({ locale }: { locale: Locale }) {
  const { data, error, loading, receivedAt, refresh } = useLiveStatus();
  const L = COPY[locale];
  const time = (value: string | null | undefined) => value && Number.isFinite(Date.parse(value)) ? lisbon(value, locale, true) : L.unknown;
  const confirmedEmpty = data?.state === "no_active_fires" && data.sources.some((source) => source.capture_succeeded && source.positively_empty && !source.is_partial);
  return <section className={s.livePanel} aria-label={L.title}>
    <div className={s.livePanelHead}>
      <div><h2>{L.title}</h2><span className={error ? s.connectionBad : data ? s.connectionGood : s.connectionPending} role="status">
        {error ? L.unavailable : data ? L.connected : L.checking}
      </span></div>
      <button className={s.refreshData} disabled={loading} onClick={() => void refresh()}>{loading ? L.refreshing : L.refresh}</button>
    </div>
    <p className={s.muted}>{L.cadence}</p>
    {error && <p className={s.liveWarning} role="alert">{L.offline}</p>}
    {data && <>
      {error && <p className={s.liveWarning}>{L.stale}</p>}
      <p className={data.state === "check_failed" || data.state === "partial" ? s.liveWarning : s.liveStatement}>
        {data.state === "partial" ? L.partial : data.state === "check_failed" ? L.failed
          : confirmedEmpty ? L.noFires : data.state === "active_fires" ? `${L.active}: ${data.active.length}` : L.partial}
      </p>
      {data.statement && <blockquote className={s.liveQuote}>{data.statement}</blockquote>}
      <dl className={s.captureTimes}>
        <div><dt>{L.checked}</dt><dd><time dateTime={data.checked_at}>{time(data.checked_at)} · Lisboa</time></dd></div>
        <div><dt>{L.received}</dt><dd><time dateTime={receivedAt ?? undefined}>{time(receivedAt)} · Lisboa</time></dd></div>
      </dl>
      <h3>{L.sources}</h3>
      <div className={s.liveSources}>{data.sources.map((source) => <article className={s.liveSource} key={source.url}>
        <div className={s.liveSourceHead}><a href={source.url} target="_blank" rel="noreferrer">{source.name} ↗</a>
          <strong className={source.capture_succeeded && !source.is_partial ? s.connectionGood : s.connectionBad}>
            {source.is_partial ? L.partialCapture : source.capture_succeeded ? L.success : L.captureFailed}
          </strong>
        </div>
        <p>{source.via}</p>
        <dl><div><dt>{L.captured}</dt><dd>{time(source.captured_at)}</dd></div>
          {!source.capture_succeeded && <div><dt>{L.lastGood}</dt><dd>{time(source.last_successful_capture_at)}</dd></div>}
          <div><dt>{L.credits}</dt><dd>{source.credits ?? L.unknown}</dd></div>
          <div><dt>{L.request}</dt><dd><code>{source.request_id ?? L.unknown}</code></dd></div>
          {source.error_code && <div><dt>{L.result}</dt><dd>{source.error_code}</dd></div>}
        </dl>
        {source.capture_succeeded && source.positively_empty && !source.is_partial && <span className={s.connectionGood}>{L.empty}</span>}
      </article>)}</div>
      {data.active.length > 0 && <div className={s.liveRecords}><table className={s.docTable}>
        <thead><tr><th>{L.incident}</th><th>{L.place}</th><th>{L.status}</th></tr></thead>
        <tbody>{data.active.map((incident) => <tr key={incident.id}><td><a href={incident.url} target="_blank" rel="noreferrer">{incident.id} ↗</a></td><td>{incident.place}</td><td>{incident.status}</td></tr>)}</tbody>
      </table></div>}
      <details className={s.liveRaw}><summary>{L.raw}</summary><pre>{JSON.stringify(data, null, 2)}</pre></details>
    </>}
  </section>;
}
