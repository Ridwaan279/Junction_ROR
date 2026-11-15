"use client";
import { useEffect, useState } from "react";
import Link from "next/link";
import { useLocale } from "@/components/ops/SiteNav";
import { cameraState, staleAt, type ReconTask } from "@/lib/recon";
import s from "./recon.module.css";

const API = "/api/recon-backend";
const RUN = "synthetic-portugal";
const QUERY = "?run_id=" + RUN;
type Snapshot = { scenario: { epoch: number; mode: string }; incidents: ReconTask["incident"][]; evidence: { id: string; provenance: ReconTask["reports"][number] }[] };
const STATES = ["assigned", "travelling", "observing", "returning", "complete"] as const;
const stateLabels = {
  pt: { assigned:"Atribuída", travelling:"Em viagem", observing:"A observar", returning:"Em regresso", complete:"Concluída", failed:"Falhou" },
  en: { assigned:"Assigned", travelling:"Travelling", observing:"Observing", returning:"Returning", complete:"Complete", failed:"Failed" },
};
async function read(path: string, body?: object) {
  const response = await fetch(API + path, { cache:"no-store", ...(body ? { method:"POST", headers:{"Content-Type":"application/json"}, body:JSON.stringify(body) } : {}), signal:AbortSignal.timeout(6000) });
  if (!response.ok) throw new Error("Backend " + response.status);
  return response.json();
}
export default function ReconConsole() {
  const [locale] = useLocale();
  const pt = locale === "pt";
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [tasks, setTasks] = useState<ReconTask[]>([]);
  const [error, setError] = useState(false);
  const [busy, setBusy] = useState(false);
  const [now, setNow] = useState(0);
  const [brokenImage, setBrokenImage] = useState<string | null>(null);
  useEffect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const [snap, recon] = await Promise.all([read("/api/snapshot" + QUERY), read("/api/recon/tasks" + QUERY)]);
        if (snap.scenario.mode !== "synthetic") throw new Error("Recon demo requires synthetic mode");
        if (!stopped) { setSnapshot(snap); setTasks(recon.tasks.filter((t: ReconTask) => t.epoch === snap.scenario.epoch).sort((a: ReconTask, b: ReconTask) => a.assigned_at.localeCompare(b.assigned_at))); setError(false); }
      } catch { if (!stopped) setError(true); }
      if (!stopped) { setNow(Date.now()); timer = setTimeout(poll, 1500); }
    }
    void poll();
    return () => { stopped = true; clearTimeout(timer); };
  }, []);
  const task = tasks.at(-1);
  const incident = task?.incident || snapshot?.incidents[0];
  const reports = task?.reports || snapshot?.evidence.filter(e => incident?.evidence_ids?.includes(e.id)).map(e => e.provenance) || [];
  const observation = task?.observation;
  const state = observation?.state || "assigned";
  const camera = task ? cameraState(task, now) : "no_camera";
  const frame = task?.frame;
  const imageUrl = frame ? API + frame.image_url : "";
  const at = (value: string | null | undefined) => value ? new Date(value).toLocaleString(pt ? "pt-PT" : "en-GB", { timeZone:"Europe/Lisbon", hour12:false }) + " · Lisboa" : (pt ? "Desconhecido" : "Unknown");
  async function assign() {
    if (!incident) return;
    setBusy(true);
    try { const t = await read("/api/recon/tasks" + QUERY, { incident_id:incident.id, drone_id:"ember_drone" }); setTasks(previous => [...previous.filter(p => p.id !== t.id), t]); }
    catch { setError(true); } finally { setBusy(false); }
  }
  async function recall() {
    if (!task) return;
    setBusy(true);
    try { await read(`/api/recon/tasks/${task.id}/return` + QUERY, {}); }
    catch { setError(true); } finally { setBusy(false); }
  }
  return <main className={s.page}>
    <header className={s.header}><Link href="/">← EmberWatch</Link><span className={s.badge}>{pt ? "RECONHECIMENTO SIMULADO" : "SIMULATED RECONNAISSANCE"}</span></header>
    <h1>{pt ? "Uma vista sobre o incêndio" : "A view of the incident"}</h1>
    <p className={s.muted}>{pt ? "Cenário sintético · drone cinemático Gazebo · sem aeronaves reais ou despacho operacional" : "Synthetic scenario · Gazebo kinematic drone · no real aircraft or operational dispatch"}</p>
    {error && <p role="alert" className={s.alert}>{pt ? "Backend indisponível. Os últimos dados podem estar desatualizados." : "Backend unavailable. Last received data may be stale."}</p>}
    <div className={s.grid}><section>
      <div className={s.card}><div className={s.row}><h2>{pt ? "Câmara do drone" : "Drone camera"}</h2><span className={s.badge}>GAZEBO · {task?.drone_id || "ember_drone"}</span></div>
        {frame && brokenImage !== imageUrl ? <>
          {/* Gazebo frames are served by the local backend with no caching. */}
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img className={s.camera} src={imageUrl} alt={pt ? "Imagem renderizada pela câmara do drone simulado" : "Frame rendered by the simulated drone camera"} onError={() => setBrokenImage(imageUrl)} />
          <p>{pt ? "Imagem renderizada no Gazebo" : "Gazebo rendered camera frame"} · {at(frame.captured_at)}</p>
          <p className={s.muted}>{pt ? "Posição na captura (E/N/Z local, m)" : "Position at capture (local E/N/Z, m)"}: {frame.drone_position_enu_m?.map(v => v.toFixed(1)).join(" / ") || (pt ? "Desconhecida" : "Unknown")} · t={frame.sim_time_s?.toFixed(2) ?? "?"} s</p>
        </> : <div className={s.empty}>{pt ? "Sem imagem da câmara" : "No camera frame"}<p>{pt ? "A aguardar uma imagem real do renderizador Gazebo." : "Waiting for an actual frame from the Gazebo renderer."}</p></div>}
        {(camera !== "available" || brokenImage === imageUrl) && <p role="status" className={s.alert}>{camera === "stale" ? (pt ? "Imagem desatualizada — não é a vista atual." : "Stale frame — this is not the current view.") : (pt ? "Câmara indisponível. Não é apresentada uma imagem fictícia." : "Camera unavailable. No mock image is substituted.")}</p>}
        <p className={s.muted}>{pt ? "Cenário visual representativo, sem georreferenciação. Fumo visual não é uma medição de perigo." : "Representative visual scene, not georeferenced. Visual smoke is not a sensed hazard."}</p>
      </div>
    </section><section>
      <article className={s.card}><h2>{incident?.title || (pt ? "A aguardar evento" : "Waiting for event")}</h2><span className={s.badge}>{pt ? "EVENTO SINTÉTICO" : "SYNTHETIC EVENT"}</span>
        <dl><dt>{pt ? "Local reportado (lon, lat)" : "Reported location (lon, lat)"}</dt><dd>{incident?.position?.join(", ") || "?"}</dd><dt>{pt ? "Hora reportada" : "Reported time"}</dt><dd>{at(incident?.first_event_at)}</dd><dt>{pt ? "Atualização do evento" : "Event update"}</dt><dd>{at(incident?.updated_at)}</dd><dt>{pt ? "Extensão / intensidade / vento no local" : "Extent / intensity / wind on scene"}</dt><dd>{pt ? "Desconhecidos" : "Unknown"}</dd></dl>
        <p className={s.muted}>{pt ? "A imagem mostra um incêndio ilustrativo numa cena local. Não mede a extensão ou gravidade do evento reportado." : "The image shows an illustrative fire in a local scene. It does not measure the reported event’s extent or severity."}</p>
        {reports.map((r, i) => <p className={s.muted} key={i}><a href={/^https?:\/\//.test(r.source_url) ? r.source_url : undefined} target="_blank" rel="noreferrer">{r.source_id}</a> · {at(r.captured_at)}</p>)}
        {task?.event_stale && <p className={s.alert}>{pt ? "Evento desatualizado. Rever antes de voar." : "Stale event. Review before flying."}</p>}
        <button disabled={busy || error || !incident || (!!task && !["complete", "failed"].includes(state))} onClick={assign}>{pt ? "Atribuir reconhecimento simulado" : "Assign simulated reconnaissance"}</button>
      </article>
      {task && <article className={s.card}><div className={s.row}><h2>{task.drone_id}</h2><strong data-testid="mission-state">{stateLabels[locale][state]}</strong></div>
        <ol className={s.states}>{STATES.map(step => <li key={step} aria-current={state === step ? "step" : undefined}>{stateLabels[locale][step]}</li>)}</ol>
        {(staleAt(observation?.observed_at, now) || error) && <p role="status" className={s.alert}>{pt ? "Telemetria ausente ou desatualizada" : "Telemetry missing or stale"}</p>}
        {task.requested_action === "return" && state !== "complete" && <p>{pt ? "Regresso solicitado; a aguardar confirmação do simulador." : "Return requested; awaiting simulator confirmation."}</p>}
        <p>{observation?.note || (pt ? "A aguardar o simulador" : "Waiting for simulator")}</p>
        <p className={s.muted}>{pt ? "Observação" : "Observation"}: {at(observation?.observed_at)}</p>
        <button onClick={recall} disabled={busy || error || ["complete", "failed"].includes(state)}>{pt ? "Solicitar regresso" : "Request return"}</button>
        <details className={s.details}><summary>{pt ? "Origem e qualidade" : "Provenance and quality"}</summary><p>{pt ? "Estado do simulador. Não há deteção baseada em imagens." : "Simulator-provided state. No image-based detection."}</p>{task.reports.map((r, i) => <div key={i}><a href={/^https?:\/\//.test(r.source_url) ? r.source_url : undefined} target="_blank" rel="noreferrer">{r.source_id}</a><p className={s.muted}>{r.origin_group} · {at(r.captured_at)}<br />{r.quality_flags.join(", ")}</p></div>)}</details>
      </article>}
    </section></div>
  </main>;
}
