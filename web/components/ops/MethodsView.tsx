"use client";
import { FLEET } from "@/lib/fleet";
import SiteNav, { useLocale } from "./SiteNav";
import s from "./ops.module.css";

export default function MethodsView() {
  const [locale, setLocale] = useLocale();
  const t = (pt: string, en: string) => (locale === "pt" ? pt : en);
  const segment = (FLEET.cruise_kmh * (FLEET.revisit_min / 60)) / 2;
  const transit = (FLEET.staging_km / FLEET.cruise_kmh) * 60;
  const duty = (FLEET.endurance_min - 2 * transit) / (FLEET.endurance_min + FLEET.recharge_min);

  return (
    <main className={s.doc}>
      <SiteNav active="methods" locale={locale} />
      <button className={s.docLocale} onClick={() => setLocale(locale === "pt" ? "en" : "pt")}>{locale === "pt" ? "EN" : "PT"}</button>
      <article className={s.docBody}>
        <h1>{t("Método e tecnologia", "Methods and technology")}</h1>
        <p className={s.lead}>{t(
          "A física calcula todos os números. O Context.dev fornece as provas, cada uma com fonte e hora. Nada é inventado; os drones e o despacho são simulações.",
          "Physics produces every number. Context.dev provides the evidence, each item with its source and time. Nothing is invented; drones and dispatch are simulations.")}</p>

        <section>
          <h2>{t("1. Propagação do fogo", "1. Fire spread")}</h2>
          <p>{t("Velocidade de propagação à superfície (Rothermel 1972; Albini 1976):", "Surface rate of spread (Rothermel 1972; Albini 1976):")}</p>
          <pre className={s.formula}>R = I_R · ξ · (1 + φ_w + φ_s) / (ρ_b · ε · Q_ig)</pre>
          <p className={s.muted}>{t(
            "I_R intensidade de reação, ξ razão de fluxo propagante, φ_w e φ_s fatores de vento e declive, ρ_b densidade do leito, ε aquecimento efetivo, Q_ig calor de ignição. Combustíveis: 13 modelos de Anderson (1982) associados à cobertura do solo (aproximação, não os modelos calibrados para Portugal). Vento à altura da chama com fatores de ajuste sem abrigo (Albini e Baughman 1979; Andrews 2012).",
            "I_R reaction intensity, ξ propagating flux ratio, φ_w and φ_s wind and slope factors, ρ_b bulk density, ε effective heating, Q_ig heat of ignition. Fuels: Anderson's (1982) 13 models mapped from land cover (an approximation, not the calibrated Portuguese models). Midflame wind from unsheltered adjustment factors (Albini and Baughman 1979; Andrews 2012).")}</p>
          <pre className={s.formula}>{`L/B = 0.936·e^(0.2566·U) + 0.461·e^(−0.1548·U) − 0.397      (Anderson 1983, U ${t("em", "in")} mi/h)
I   = H · w · R                                              (Byram 1959, kW/m)
L_f = 0.45 · I^0.46                                          (Byram 1959, ft; I ${t("em", "in")} BTU/ft/s)`}</pre>
          <p>{t(
            "No terreno, o fogo cresce pelo caminho de menor tempo (Finney 2002) com Dijkstra numa grelha de 16 vizinhos. Incerteza: 32 simulações com vento ×lognormal(σ = 0,2), direção ±12° e humidade perturbada; a probabilidade de arder é a fração de simulações.",
            "On the landscape, fire grows along the minimum travel time (Finney 2002) using Dijkstra on a 16-neighbour grid. Uncertainty: 32 runs with wind ×lognormal(σ = 0.2), direction ±12° and perturbed moisture; burn probability is the fraction of runs.")}</p>
        </section>

        <section>
          <h2>{t("2. Fogo de copas e humidade", "2. Crown fire and moisture")}</h2>
          <pre className={s.formula}>{`I₀      = (0.010 · CBH · (460 + 25.9 · FMC))^1.5      (Van Wagner 1977)
${t("R_copa ", "R_crown")} = 3.34 · R₁₀                                    (Rothermel 1991)`}</pre>
          <p className={s.muted}>{t(
            "Fração de copa ardida segundo Scott e Reinhardt (2001). Humidade do combustível morto de 1 h pelo teor de equilíbrio (Simard 1968) a partir da temperatura e humidade relativa.",
            "Crown fraction burned after Scott and Reinhardt (2001). 1-h dead fuel moisture from equilibrium moisture content (Simard 1968) using temperature and relative humidity.")}</p>
        </section>

        <section>
          <h2>{t("3. Fumo", "3. Smoke")}</h2>
          <pre className={s.formula}>{`Q_PM2.5 = ${t("crescimento da área × combustível consumido", "area growth × fuel consumed")} × 17 g/kg
F       = g · Q_c / (π · c_p · ρ · T),   Q_c = 0.65 · Q      (${t("fluxo de flutuação", "buoyancy flux")})
Δh      = 21.425 · F^0.75 / u   (F < 55)
          38.71  · F^0.6  / u   (F ≥ 55)
          2.6 · (F / (u·s))^(1/3)   (${t("estável", "stable")})                 (Briggs)
C(x,y,z)= Q / (2π·u·σ_y·σ_z) · e^(−y²/2σ_y²) · [e^(−(z−H)²/2σ_z²) + e^(−(z+H)²/2σ_z²)]`}</pre>
          <p className={s.muted}>{t(
            "Pluma gaussiana com a emissão distribuída pela coluna de convecção (5 camadas); σ_y e σ_z de Briggs (1973) em campo aberto; classe de estabilidade de Pasquill–Turner pela posição do sol, vento e nuvens.",
            "Gaussian plume with emission spread through the convection column (5 layers); σ_y and σ_z from Briggs (1973) open country; Pasquill–Turner stability class from sun position, wind and cloud.")}</p>
        </section>

        <section>
          <h2>{t("4. Quantos drones", "4. How many drones")}</h2>
          <pre className={s.formula}>{locale === "pt" ? `frente   = ⌈ comprimento da frente / ${segment.toFixed(1)} km ⌉        ${FLEET.cruise_kmh} km/h × ${FLEET.revisit_min} min ÷ 2 (ida e volta)
cabeça   = 1 + ⌊ vento / ${FLEET.head_wind_step_kmh} km/h ⌋
locais   = ⌈ locais e ocorrências a < ${FLEET.site_radius_km} km / ${FLEET.sites_per_drone} ⌉
no ar    = frente + cabeça + locais
rotação  = (${FLEET.endurance_min} − 2 × ${transit.toFixed(1)}) / (${FLEET.endurance_min} + ${FLEET.recharge_min}) = ${duty.toFixed(2)}
frota    = ⌈ no ar / rotação × ${1 + FLEET.reserve} ⌉
em terra se rajadas > ${FLEET.gust_limit_kmh} km/h` : `front    = ⌈ front length / ${segment.toFixed(1)} km ⌉        ${FLEET.cruise_kmh} km/h × ${FLEET.revisit_min} min ÷ 2 (out and back)
head     = 1 + ⌊ wind / ${FLEET.head_wind_step_kmh} km/h ⌋
places   = ⌈ places and events within ${FLEET.site_radius_km} km / ${FLEET.sites_per_drone} ⌉
airborne = front + head + places
duty     = (${FLEET.endurance_min} − 2 × ${transit.toFixed(1)}) / (${FLEET.endurance_min} + ${FLEET.recharge_min}) = ${duty.toFixed(2)}
fleet    = ⌈ airborne / duty × ${1 + FLEET.reserve} ⌉
grounded if gusts > ${FLEET.gust_limit_kmh} km/h`}</pre>
          <p className={s.muted}>{t(
            "Usa apenas dados públicos no momento mostrado. A base fica 5 km a barlavento (ar limpo). Constantes de demonstração; as rotas definitivas, que evitam o fumo, vêm do planeador A* do serviço Rust.",
            "Uses only data public at the moment shown. The staging point is 5 km upwind (clean air). Demonstration constants; the final smoke-avoiding routes come from the Rust service's A* planner.")}</p>
        </section>

        <section>
          <h2>{t("5. Precisão", "5. Accuracy")}</h2>
          <pre className={s.formula}>Dice = 2 · |P ∩ O| / (|P| + |O|)</pre>
          <p className={s.muted}>{t(
            "P área prevista (p50), O área observada por satélite (GWIS via GDACS). Calculado sobre a área total e, mais exigente, só sobre o crescimento novo.",
            "P predicted area (p50), O satellite-observed area (GWIS via GDACS). Computed on the total area and, more strictly, on new growth only.")}</p>
        </section>

        <section>
          <h2>{t("6. Tecnologia", "6. Technology")}</h2>
          <table className={s.docTable}>
            <tbody>
              <tr><td>{t("Serviço", "Service")}</td><td>Rust 1.100 · Axum 0.8 · Tokio 1.48 · Serde · reqwest 0.12 (rustls) · SQLx 0.8 · PostgreSQL/PostGIS {t("(opcional)", "(optional)")}</td></tr>
              <tr><td>{t("Módulos Rust", "Rust crates")}</td><td>domain · platform · intelligence (Context.dev, Fogos.pt, IPMA, webhooks) · response (A*) · firecast ({t("física", "physics")}) · server</td></tr>
              <tr><td>{t("Interface", "Interface")}</td><td>Next.js 16.4 · React 19.3 · TypeScript 6.0 · MapLibre GL 6.18 {t("(terreno 3D)", "(3D terrain)")} · maplibre-contour</td></tr>
              <tr><td>{t("Testes", "Tests")}</td><td>cargo test · clippy · Vitest 5.1 · Playwright</td></tr>
              <tr><td>{t("Dados da web", "Web data")}</td><td>Context.dev ({t("pesquisa, leitura de páginas, extração JSON", "search, page reading, JSON extraction")}) · {t("páginas públicas do Fogos.pt via Context.dev", "Fogos.pt public pages via Context.dev")}</td></tr>
              <tr><td>{t("Meteorologia", "Weather")}</td><td>Open-Meteo ({t("previsão e arquivo ERA5", "forecast and ERA5 archive")}) · IPMA</td></tr>
              <tr><td>{t("Mapa", "Map")}</td><td>Sentinel-2 cloudless 2016 (EOX, CC BY 4.0) · {t("relevo", "elevation")} Mapterhorn / Terrarium · OpenFreeMap / OpenStreetMap</td></tr>
              <tr><td>{t("Validação e locais", "Validation and places")}</td><td>GWIS/EFFIS via GDACS · GeoNames</td></tr>
            </tbody>
          </table>
        </section>
      </article>
    </main>
  );
}
