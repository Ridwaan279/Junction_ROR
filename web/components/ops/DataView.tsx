"use client";
import { useEffect, useState } from "react";
import { lisbon, REPLAY_BASES, type ReplayIndex } from "@/lib/replay";
import SiteNav, { useLocale } from "./SiteNav";
import s from "./ops.module.css";

type Search = { name: string; endpoint: string; request_id: string | null; credits: number | null; error: unknown; results: number };
type Page = ReplayIndex["pages"][number] & { credits: number | null; request_id: string | null; time_flags: string[]; published_at: string | null };
type Index = Omit<ReplayIndex, "pages"> & { pages: Page[]; searches: Search[] };
type Live = { checked_at: string; state: string; statement: string | null; sources: { name: string; url: string; via: string; request_id: string | null; credits: number | null }[] };

const SEARCH_LABEL: Record<string, { pt: string; en: string }> = {
  news_incendio_arganil: { pt: "Notícias: incêndio Arganil Piódão (pt)", en: "News: Arganil Piódão fire (pt)" },
  news_domains: { pt: "Notícias: só domínios portugueses", en: "News: Portuguese domains only" },
  news_country: { pt: "Notícias: país = PT", en: "News: country = PT" },
  news_en: { pt: "Notícias: imprensa internacional (en)", en: "News: international press (en)" },
  web_incendio: { pt: "Web: incêndio e trovoada, 13 ago.", en: "Web: fire and lightning, 13 Aug" },
  web_evacuacao: { pt: "Web: evacuações de aldeias", en: "Web: village evacuations" },
  web_estradas: { pt: "Web: estradas cortadas", en: "Web: road closures" },
  web_festas: { pt: "Web: festas e eventos próximos", en: "Web: festivals and nearby events" },
  web_ipma: { pt: "Web: avisos IPMA e perigo de incêndio", en: "Web: IPMA warnings and fire danger" },
};
const MONITOR_LABEL: Record<string, { pt: string; en: string }> = {
  fogos: { pt: "Página oficial da ocorrência (Fogos.pt)", en: "Official incident page (Fogos.pt)" },
  news_index: { pt: "Índice de notícias", en: "News index" },
  web_news: { pt: "Notícias na web", en: "Web news" },
  events: { pt: "Eventos perto do fogo", en: "Events near the fire" },
  warnings: { pt: "Avisos e perigo de incêndio", en: "Warnings and fire danger" },
  weather: { pt: "Meteorologia", en: "Weather" },
  satellite: { pt: "Satélite (focos de calor)", en: "Satellite hotspots" },
  terrain: { pt: "Terreno e combustível", en: "Terrain and fuel" },
};
const STATUS = { pt: { ok: "ok", coverage_gap: "sem cobertura", not_run: "não executado", failed: "falhou" }, en: { ok: "ok", coverage_gap: "no coverage", not_run: "not run", failed: "failed" } } as const;

const T = {
  pt: {
    title: "O que o Context.dev recolhe",
    intro: "Todas as informações da web passam pelo Context.dev: pesquisa, leitura de páginas públicas e extração com um esquema JSON. Cada facto guarda o URL, a citação exata, a hora de publicação e a hora de captura. Os valores desconhecidos ficam vazios.",
    monitors: "O que é monitorizado", source: "Fonte", how: "Como", status: "Estado", items: "Itens",
    run: "Captura do incêndio de Arganil (13–14 ago. 2025)", searches: "pesquisas", pages: "páginas lidas", ok: "com sucesso", blocked: "bloqueadas pelo site", credits: "créditos", facts: "factos extraídos", verified: "citações confirmadas no texto",
    pageList: "Páginas lidas", published: "Visível desde", publisher: "Publicação", headline: "Título",
    searchList: "Pesquisas", query: "Pesquisa", endpoint: "Endpoint", results: "Resultados",
    live: "Verificação ao vivo", liveText: (st: string | null, at: string) => `Lista pública do Fogos.pt às ${at}: “${st ?? "—"}”.`,
    rules: "Regras de exatidão",
    ruleList: [
      "Uma citação só conta se aparecer palavra por palavra no texto da página.",
      "Cada facto aparece na reprodução apenas a partir da hora em que foi público (a mais tardia entre metadados, hora impressa e horas citadas no texto).",
      "Notícias da agência Lusa repetidas em vários jornais contam como uma única fonte.",
      "Uma captura falhada ou parcial nunca significa “sem incêndios”.",
      "O texto completo das notícias não é publicado; guardamos apenas factos, citações curtas e ligações.",
    ],
    failed: "falhou",
  },
  en: {
    title: "What Context.dev pulls",
    intro: "All web information goes through Context.dev: search, reading public pages and extraction with a JSON schema. Every fact keeps its URL, the exact quote, publication time and capture time. Unknown values stay empty.",
    monitors: "What is monitored", source: "Source", how: "How", status: "Status", items: "Items",
    run: "Arganil fire capture (13–14 Aug 2025)", searches: "searches", pages: "pages read", ok: "succeeded", blocked: "blocked by the site", credits: "credits", facts: "facts extracted", verified: "quotes confirmed in page text",
    pageList: "Pages read", published: "Visible from", publisher: "Publisher", headline: "Headline",
    searchList: "Searches", query: "Search", endpoint: "Endpoint", results: "Results",
    live: "Live check", liveText: (st: string | null, at: string) => `Fogos.pt public list at ${at}: “${st ?? "—"}”.`,
    rules: "Accuracy rules",
    ruleList: [
      "A quote counts only if it appears word for word in the page text.",
      "Each fact appears in the replay only from the time it was public (the latest of metadata, printed time and clock times quoted in the text).",
      "Lusa agency stories repeated by several papers count as one source.",
      "A failed or partial capture never means “no fires”.",
      "Full article text is not published; only facts, short quotes and links are kept.",
    ],
    failed: "failed",
  },
};

export default function DataView() {
  const [locale, setLocale] = useLocale();
  const [index, setIndex] = useState<Index | null>(null);
  const [live, setLive] = useState<Live | null>(null);
  const L = T[locale];
  useEffect(() => {
    void (async () => {
      for (const b of REPLAY_BASES) {
        const r = await fetch(b + "index.json").catch(() => null);
        if (r?.ok) { setIndex(await r.json()); return; }
      }
    })();
    void fetch("/fixtures/v2/live-status.json").then((r) => r.json()).then(setLive).catch(() => undefined);
  }, []);

  const pages = index?.pages ?? [];
  const okPages = pages.filter((p) => p.status === "ok");
  const credits = pages.reduce((a, p) => a + (p.credits ?? 0), 0) + (index?.searches ?? []).reduce((a, x) => a + (x.credits ?? 0), 0);
  const facts = index?.evidence.length ?? 0;
  const verified = index?.evidence.filter((e) => e.quality === "verified").length ?? 0;

  return (
    <main className={s.doc}>
      <SiteNav active="data" locale={locale} />
      <button className={s.docLocale} onClick={() => setLocale(locale === "pt" ? "en" : "pt")}>{locale === "pt" ? "EN" : "PT"}</button>
      <article className={s.docBody}>
        <h1>{L.title}</h1>
        <p className={s.lead}>{L.intro}</p>

        {index && (
          <>
            <section>
              <h2>{L.run}</h2>
              <dl className={s.figures}>
                <div><dd>{index.searches.length}</dd><dt>{L.searches}</dt></div>
                <div><dd>{pages.length}</dd><dt>{L.pages}</dt></div>
                <div><dd>{okPages.length}</dd><dt>{L.ok}</dt></div>
                <div><dd>{pages.filter((p) => p.error_code === "WEBSITE_BLOCKED").length}</dd><dt>{L.blocked}</dt></div>
                <div><dd>{verified}/{facts}</dd><dt>{L.verified}</dt></div>
                <div><dd>{credits}</dd><dt>{L.credits}</dt></div>
              </dl>
            </section>

            <section>
              <h2>{L.monitors}</h2>
              <table className={s.docTable}>
                <thead><tr><th>{L.source}</th><th>{L.how}</th><th>{L.status}</th><th>{L.items}</th></tr></thead>
                <tbody>
                  {index.monitors.map((m) => (
                    <tr key={m.id}>
                      <td>{MONITOR_LABEL[m.id]?.[locale] ?? m.category}</td>
                      <td className={s.muted}>{m.provider}</td>
                      <td>{STATUS[locale][m.status as keyof (typeof STATUS)["pt"]] ?? m.status}</td>
                      <td className={s.num}>{m.items}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>

            <section>
              <h2>{L.pageList}</h2>
              <table className={s.docTable}>
                <thead><tr><th>{L.published}</th><th>{L.publisher}</th><th>{L.headline}</th></tr></thead>
                <tbody>
                  {[...pages].sort((a, b) => (a.known_at ?? "9").localeCompare(b.known_at ?? "9")).map((p) => (
                    <tr key={p.id}>
                      <td className={s.muted}>{p.known_at ? lisbon(p.known_at, locale, true) : "—"}</td>
                      <td>{p.publisher}</td>
                      <td>
                        {p.status === "ok"
                          ? <a href={p.url} target="_blank" rel="noreferrer">{(p.headline ?? p.url).replace(/^\[[^\]]*\]\s*/, "")}</a>
                          : <span className={s.muted}>{L.failed} · {p.error_code}</span>}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>

            <section>
              <h2>{L.searchList}</h2>
              <table className={s.docTable}>
                <thead><tr><th>{L.query}</th><th>{L.endpoint}</th><th className={s.num}>{L.results}</th></tr></thead>
                <tbody>
                  {index.searches.map((x) => (
                    <tr key={x.name}>
                      <td>{SEARCH_LABEL[x.name]?.[locale] ?? x.name}</td>
                      <td className={s.muted}><code>{x.endpoint.replace("POST ", "")}</code></td>
                      <td className={s.num}>{x.error ? L.failed : x.results}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          </>
        )}

        {live && (
          <section>
            <h2>{L.live}</h2>
            <p>{L.liveText(live.statement, lisbon(live.checked_at, locale, true))}{" "}
              {live.sources.map((src) => <a key={src.url} href={src.url} target="_blank" rel="noreferrer">fogos.pt/pt/lista</a>)}
              <span className={s.muted}> · {live.sources[0]?.via}</span>
            </p>
          </section>
        )}

        <section>
          <h2>{L.rules}</h2>
          <ul className={s.rules}>{L.ruleList.map((r) => <li key={r}>{r}</li>)}</ul>
        </section>
      </article>
    </main>
  );
}
