//! Historical fires: a sourced dataset and a multi-day replay.
//!
//! The dataset bundles what Context.dev and the weather/terrain adapters would
//! have gathered while the fire burned: dated burned-area perimeters (GWIS via
//! GDACS), hourly ERA5 reanalysis weather at nine points, a terrain snapshot, and
//! dated web reports with their URLs and extracted facts.
//!
//! The replay forecasts from the ignition point to the first observed perimeter,
//! then re-initialises from every observed perimeter and forecasts the next one
//! (perimeter re-initialisation, a simple form of data assimilation). Each forecast
//! is scored against the next observation with the Sørensen–Dice coefficient on
//! total and newly burned area. A simulated drone fleet is sized and routed on
//! every frame from the modelled fire and smoke.
use crate::FirecastError;
use crate::forecast::{line, multipolygon, round, wind_grid_for};
use crate::fuel::LandCover;
use crate::geo::{LocalFrame, bearing_of, from_to_toward_deg};
use crate::moisture::{MoistureInputs, estimate};
use crate::smoke::{
    PM25_EMISSION_FACTOR_G_PER_KG, PlumeMet, PlumeSource, concentration, stability_class, summarise,
};
use crate::spread::{
    EnsembleConfig, EnsembleResult, HourConditions, Landscape, beds_for, cell_spread, run_ensemble,
};
use crate::terrain::Dem;
use crate::weather::WeatherField;
use crate::{solar, sources::HttpSources};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use emberwatch_response::{Cell, CellPosition, Grid, MissionRequest, WindVector, plan_mission};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceRef {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub licence: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Perimeter {
    pub time: DateTime<Utc>,
    pub area_ha: f64,
    pub source: String,
    pub url: String,
    pub geometry: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub time: DateTime<Utc>,
    /// How precise `time` is: "reported_time", "article_date", "evening_described"…
    pub time_basis: String,
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub source: String,
    pub url: String,
    /// Structured facts as Context.dev JSON extraction would return them.
    #[serde(default)]
    pub facts: Value,
    /// "web_research" (verified article) or "gdacs_emm" (media monitor feed).
    pub origin: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AreaFact {
    pub area_ha: f64,
    pub source: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Domain {
    pub centre: [f64; 2],
    pub cell_m: f64,
    pub size: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VegetationZones {
    pub note: String,
    /// (minimum elevation m, class), highest first.
    pub zones: Vec<(f64, LandCover)>,
    pub sources: Vec<SourceRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dataset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub glide: Option<String>,
    pub ignition_time: DateTime<Utc>,
    pub ignition_position: [f64; 2],
    pub ignition_basis: String,
    pub detection_time: DateTime<Utc>,
    pub resolved_time: Option<DateTime<Utc>>,
    pub final_area: Vec<AreaFact>,
    pub domain: Domain,
    /// Row-major from the south-west corner, metres.
    pub elevation_m: Vec<i16>,
    pub elevation_source: SourceRef,
    pub vegetation: VegetationZones,
    pub perimeters: Vec<Perimeter>,
    pub weather: WeatherField,
    pub weather_source: SourceRef,
    pub events: Vec<Event>,
    pub sources: Vec<SourceRef>,
}

impl Dataset {
    pub fn frame(&self) -> LocalFrame {
        LocalFrame::centred(
            self.domain.centre,
            self.domain.size as f64 * self.domain.cell_m / 2.0,
        )
    }
    pub fn dem(&self) -> Dem {
        Dem {
            width: self.domain.size,
            height: self.domain.size,
            cell_m: self.domain.cell_m,
            elevation_m: self.elevation_m.iter().map(|v| f32::from(*v)).collect(),
            frame: self.frame(),
        }
    }
    pub fn land_cover(&self) -> Vec<LandCover> {
        self.elevation_m
            .iter()
            .map(|z| {
                let z = f64::from(*z);
                if z <= 0.5 {
                    return LandCover::NonBurnable;
                }
                self.vegetation
                    .zones
                    .iter()
                    .find(|(min, _)| z >= *min)
                    .map_or(LandCover::Shrubland, |(_, c)| *c)
            })
            .collect()
    }
    pub fn validate(&self) -> Result<(), FirecastError> {
        let n = self.domain.size * self.domain.size;
        if self.elevation_m.len() != n
            || self.perimeters.windows(2).any(|w| w[0].time > w[1].time)
            || self.weather.is_empty()
        {
            return Err(FirecastError::Parse(
                "dataset shape (elevation, perimeter order or weather)".into(),
            ));
        }
        Ok(())
    }
}

/// Even-odd raster of a GeoJSON (Multi)Polygon onto the local grid (cell centres).
pub fn rasterise(geometry: &Value, frame: &LocalFrame, size: usize, cell_m: f64) -> Vec<bool> {
    let mut mask = vec![false; size * size];
    let polygons: Vec<&Value> = match geometry.get("type").and_then(Value::as_str) {
        Some("Polygon") => vec![&geometry["coordinates"]],
        Some("MultiPolygon") => geometry["coordinates"]
            .as_array()
            .map(|a| a.iter().collect())
            .unwrap_or_default(),
        _ => vec![],
    };
    for polygon in polygons {
        let rings: Vec<Vec<(f64, f64)>> = polygon
            .as_array()
            .map(|rings| {
                rings
                    .iter()
                    .filter_map(|r| {
                        r.as_array().map(|pts| {
                            pts.iter()
                                .filter_map(|p| {
                                    Some(frame.to_local([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?]))
                                })
                                .collect()
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        for y in 0..size {
            let cy = (y as f64 + 0.5) * cell_m;
            let mut xs = Vec::new();
            for ring in &rings {
                for i in 0..ring.len() {
                    let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                    if (a.1 <= cy && b.1 > cy) || (b.1 <= cy && a.1 > cy) {
                        xs.push(a.0 + (cy - a.1) / (b.1 - a.1) * (b.0 - a.0));
                    }
                }
            }
            xs.sort_by(f64::total_cmp);
            for pair in xs.chunks(2) {
                if pair.len() < 2 {
                    continue;
                }
                let x0 = ((pair[0] / cell_m - 0.5).ceil().max(0.0)) as usize;
                let x1 = (pair[1] / cell_m - 0.5).floor();
                if x1 < 0.0 {
                    continue;
                }
                for x in x0..=(x1 as usize).min(size - 1) {
                    mask[y * size + x] ^= true;
                }
            }
        }
    }
    mask
}

pub fn dice(a: &[bool], b: &[bool]) -> Option<f64> {
    let (na, nb) = (
        a.iter().filter(|v| **v).count(),
        b.iter().filter(|v| **v).count(),
    );
    if na + nb == 0 {
        return None;
    }
    let both = a.iter().zip(b).filter(|(x, y)| **x && **y).count();
    Some(2.0 * both as f64 / (na + nb) as f64)
}

// ---------------------------------------------------------------- dataset build

/// Parse one GDACS wildfire episode geometry response into a dated perimeter.
pub fn parse_gdacs_perimeter(value: &Value, url: &str) -> Option<Perimeter> {
    let feature =
        value.get("features")?.as_array()?.iter().find(|f| {
            f.pointer("/properties/Class").and_then(Value::as_str) == Some("Poly_area")
        })?;
    let time = feature.pointer("/properties/polygondate")?.as_str()?;
    let time = chrono::NaiveDateTime::parse_from_str(time, "%Y-%m-%dT%H:%M:%S")
        .ok()?
        .and_utc();
    let geometry = feature.get("geometry")?.clone();
    let area_ha = geodesic_area_ha(&geometry);
    Some(Perimeter {
        time,
        area_ha: round(area_ha, 0),
        source: "GWIS burned area via GDACS".into(),
        url: url.into(),
        geometry,
    })
}

/// Planar area of a lon/lat (Multi)Polygon using a local equirectangular projection.
pub fn geodesic_area_ha(geometry: &Value) -> f64 {
    let ring_area = |ring: &Value| -> f64 {
        let pts: Vec<(f64, f64)> = ring
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?)))
                    .collect()
            })
            .unwrap_or_default();
        if pts.len() < 3 {
            return 0.0;
        }
        let lat0 = pts.iter().map(|p| p.1).sum::<f64>() / pts.len() as f64;
        let k = lat0.to_radians().cos() * crate::geo::EARTH_RADIUS_M;
        let r = crate::geo::EARTH_RADIUS_M;
        let xy: Vec<(f64, f64)> = pts
            .iter()
            .map(|p| (p.0.to_radians() * k, p.1.to_radians() * r))
            .collect();
        (0..xy.len())
            .map(|i| xy[i].0 * xy[(i + 1) % xy.len()].1 - xy[(i + 1) % xy.len()].0 * xy[i].1)
            .sum::<f64>()
            .abs()
            / 2.0
    };
    let polygons: Vec<&Value> = match geometry.get("type").and_then(Value::as_str) {
        Some("Polygon") => vec![&geometry["coordinates"]],
        Some("MultiPolygon") => geometry["coordinates"]
            .as_array()
            .map(|a| a.iter().collect())
            .unwrap_or_default(),
        _ => vec![],
    };
    polygons
        .iter()
        .map(|p| {
            let rings = p.as_array().cloned().unwrap_or_default();
            rings.first().map_or(0.0, ring_area) - rings.iter().skip(1).map(ring_area).sum::<f64>()
        })
        .sum::<f64>()
        / 10_000.0
}

/// GDACS media-monitor items mentioning Portugal, as dated web signals.
pub fn parse_gdacs_news(value: &Value, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<Event> {
    let keywords = [
        "portug", "portog", "arganil", "piódão", "piodao", "covilh", "fundão", "seia",
    ];
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|n| {
                    let title = n.get("title")?.as_str()?.to_owned();
                    let lower = title.to_lowercase();
                    if !keywords.iter().any(|k| lower.contains(k)) {
                        return None;
                    }
                    let time = chrono::NaiveDateTime::parse_from_str(
                        n.get("pubdate")?.as_str()?,
                        "%Y-%m-%dT%H:%M:%S",
                    )
                    .ok()?
                    .and_utc();
                    (time >= from && time <= to).then(|| Event {
                        time,
                        time_basis: "publication_time".into(),
                        kind: "media".into(),
                        title,
                        detail: String::new(),
                        source: n
                            .get("source")
                            .and_then(Value::as_str)
                            .unwrap_or("media")
                            .into(),
                        url: n.get("link").and_then(Value::as_str).unwrap_or("").into(),
                        facts: Value::Null,
                        origin: "gdacs_emm".into(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn t(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, min, 0)
        .single()
        .unwrap_or_default()
}

/// Dated, sourced facts about the Arganil / Piódão fire (13–24 August 2025) from
/// Portuguese reporting. Times are UTC; Portugal used WEST (UTC+1).
pub fn arganil_2025_events() -> Vec<Event> {
    let e = |time,
             basis: &str,
             kind: &str,
             title: &str,
             detail: &str,
             source: &str,
             url: &str,
             facts: Value| Event {
        time,
        time_basis: basis.into(),
        kind: kind.into(),
        title: title.into(),
        detail: detail.into(),
        source: source.into(),
        url: url.into(),
        facts,
        origin: "web_research".into(),
    };
    vec![
        e(
            t(2025, 8, 13, 4, 0),
            "reported_time",
            "ignition",
            "Fogo começa na encosta acima do Piódão (cerca das 05:00)",
            "Local officials attribute the fire to a dry-lightning strike during the night; strong wind spread embers.",
            "Renascença / Lusa",
            "https://rr.pt/noticia/pais/2025/08/13/trovoada-seca-provocou-fogo-em-arganil-junto-a-aldeia-do-piodao/436045/",
            json!({"cause":"trovoada seca (dry lightning), per local officials","wind_description":"vento forte espalhou projeções","vegetation":["mato"]}),
        ),
        e(
            t(2025, 8, 13, 4, 8),
            "reported_time",
            "detection",
            "Alerta às 05:08 (ANEPC)",
            "Civil protection records the alert at 05:08 local time.",
            "JN",
            "https://www.jn.pt/pais/artigo/fogo-vindo-do-piodao-com-uma-frente-ativa-em-seia/17858864",
            json!({"alert_local_time":"05:08"}),
        ),
        e(
            t(2025, 8, 13, 12, 0),
            "article_date",
            "report",
            "Trovoada seca provocou fogo em Arganil junto à aldeia do Piódão",
            "More than 300 personnel deployed on the first day.",
            "Observador",
            "https://observador.pt/2025/08/13/trovoada-seca-provocou-fogo-em-arganil-junto-a-aldeia-do-piodao/",
            json!({"ground_personnel":300}),
        ),
        e(
            t(2025, 8, 14, 12, 0),
            "article_date",
            "forecast",
            "Análise: vento de sudeste empurra o fogo para noroeste",
            "Portal Bombeiros.pt analysis (IPMA/ANEPC data): south-easterly wind pushing the fire north-west, turning west/north-west late afternoon, gusts above 40 km/h.",
            "Bombeiros.pt",
            "https://www.bombeiros.pt/incendio-em-arganil-cenario-para-as-proximas-horas/",
            json!({"wind_from_direction":"sudeste","reported_spread_direction":"noroeste","wind_gust_kmh":40}),
        ),
        e(
            t(2025, 8, 14, 15, 0),
            "article_date",
            "evacuation",
            "890 operacionais; Piódão cercado pelas chamas",
            "Flames spread to neighbouring municipalities; evacuations including Foz d'Égua and Chãs d'Égua.",
            "Africanews / AP",
            "https://www.africanews.com/2025/08/14/firefighters-struggle-to-contain-blazes-in-northern-and-central-portugal/",
            json!({"ground_personnel":890,"evacuations":["Foz d'Égua","Chãs d'Égua"]}),
        ),
        e(
            t(2025, 8, 17, 11, 0),
            "reported_time",
            "front",
            "Frente ativa em Seia, no Parque Natural da Serra da Estrela",
            "Active front by the stream between Teixeira and Teixeira de Baixo; evacuated residents allowed back from 12:00.",
            "JN",
            "https://www.jn.pt/pais/artigo/fogo-vindo-do-piodao-com-uma-frente-ativa-em-seia/17858864",
            json!({"threatened_places":["Teixeira","Teixeira de Baixo"],"reported_spread_direction":"norte"}),
        ),
        e(
            t(2025, 8, 17, 19, 0),
            "evening_described",
            "escalation",
            "Fogo descontrolado: ventos variáveis e rajadas desde a tarde de domingo",
            "Arganil mayor: fire completely out of control with shifting winds and gusts from late afternoon and night of Sunday 17 August; reaches the Serra da Estrela park boundary.",
            "NiT",
            "https://www.nit.pt/fora-de-casa/na-cidade/chamas-ameacam-o-parque-natural-da-serra-da-estrela",
            json!({"wind_description":"ventos variáveis, rajadas"}),
        ),
        e(
            t(2025, 8, 18, 12, 0),
            "article_date",
            "front",
            "Covilhã: \"incêndio fora de controlo\"; três frentes no Fundão",
            "Fronts at Silvares, around Lavacolhos, and the most devastating one heading to Enxabarda; Covilhã asks residents to stay in safe places.",
            "Diário de Notícias",
            "https://www.dn.pt/sociedade/inc%C3%AAndio-est%C3%A1-fora-de-controlo-covilh%C3%A3-lan%C3%A7a-novo-alerta-%C3%A0-popula%C3%A7%C3%A3o-devido-%C3%A0-gravidade-da-situa%C3%A7%C3%A3o",
            json!({"active_fronts":3,"threatened_places":["Silvares","Lavacolhos","Enxabarda"],"reported_spread_direction":"sudeste"}),
        ),
        e(
            t(2025, 8, 19, 12, 0),
            "article_date",
            "front",
            "Fogo entra no concelho de Castelo Branco vindo do Fundão",
            "Covilhã considers the fire out of control because of wind intensity, with gusts reaching 40 km/h.",
            "Diário de Notícias",
            "https://www.dn.pt/sociedade/inc%C3%AAndio-com-origem-em-arganil-alastrou-se-ao-munic%C3%ADpio-de-castelo-branco",
            json!({"wind_gust_kmh":40,"threatened_places":["Castelo Branco"],"reported_spread_direction":"sul"}),
        ),
        e(
            t(2025, 8, 24, 12, 0),
            "article_date",
            "containment",
            "Incêndio em resolução (ANEPC)",
            "Declared in resolution almost twelve days after it started.",
            "Rádio Cova da Beira",
            "https://rcb-radiocovadabeira.pt/incendio-em-resolucao/",
            Value::Null,
        ),
        e(
            t(2025, 8, 25, 12, 0),
            "article_date",
            "assessment",
            "Maior área ardida de sempre em Portugal: 64 451 ha (ICNF provisório)",
            "Provisional ICNF national report.",
            "Observador / Lusa",
            "https://observador.pt/2025/08/25/fogo-que-comecou-em-arganil-apresenta-a-maior-area-ardida-de-sempre-em-portugal/",
            json!({"burned_area_ha":64451}),
        ),
    ]
}

pub const ARGANIL_GDACS_EVENT: u64 = 1_024_588;

/// Download and assemble the Arganil / Piódão 2025 dataset (network).
pub async fn build_arganil_2025(sources: &HttpSources) -> Result<Dataset, FirecastError> {
    let domain = Domain {
        centre: [-7.758, 40.175],
        cell_m: 200.0,
        size: 211,
    };
    let frame = LocalFrame::centred(domain.centre, domain.size as f64 * domain.cell_m / 2.0);
    let mut perimeters = Vec::new();
    for episode in 1..=12 {
        let url = format!(
            "https://www.gdacs.org/gdacsapi/api/polygons/getgeometry?eventtype=WF&eventid={ARGANIL_GDACS_EVENT}&episodeid={episode}"
        );
        let value = sources.get_json(&url, 8 * 1024 * 1024).await?;
        perimeters.extend(parse_gdacs_perimeter(&value, &url));
    }
    perimeters.sort_by_key(|p| p.time);
    perimeters.dedup_by(|a, b| a.time == b.time);
    let points: Vec<[f64; 2]> = (0..9)
        .map(|k| {
            frame.to_lon_lat(
                (0.12 + 0.38 * (k % 3) as f64) * domain.size as f64 * domain.cell_m,
                (0.12 + 0.38 * (k / 3) as f64) * domain.size as f64 * domain.cell_m,
            )
        })
        .collect();
    let (weather, weather_url) = sources
        .weather_archive(
            &points,
            NaiveDate::from_ymd_opt(2025, 8, 12).unwrap_or_default(),
            NaiveDate::from_ymd_opt(2025, 8, 21).unwrap_or_default(),
        )
        .await?;
    let dem = sources
        .dem_tiles(frame, domain.size, domain.size, domain.cell_m)
        .await?;
    let news_url = format!(
        "https://www.gdacs.org/gdacsapi/api/emm/getemmnewsbykey?eventtype=WF&eventid={ARGANIL_GDACS_EVENT}"
    );
    let mut events = arganil_2025_events();
    if let Ok(news) = sources.get_json(&news_url, 4 * 1024 * 1024).await {
        events.extend(parse_gdacs_news(
            &news,
            t(2025, 8, 13, 0, 0),
            t(2025, 8, 21, 0, 0),
        ));
    }
    events.sort_by_key(|e| e.time);
    let gdacs = |p: &str| SourceRef {
        name: format!("GDACS {p}"),
        url: format!(
            "https://www.gdacs.org/report.aspx?eventid={ARGANIL_GDACS_EVENT}&eventtype=WF"
        ),
        licence: Some("GDACS terms; GWIS/EFFIS data © European Union".into()),
    };
    Ok(Dataset {
        id: "arganil-piodao-2025".into(),
        name: "Arganil / Piódão wildfire, 13–24 August 2025".into(),
        description: "Largest burned area on record in Portugal. Started by dry lightning above Piódão (Arganil) and spread into Seia, Covilhã, Fundão and Castelo Branco.".into(),
        glide: Some("WF-2025-000140-PRT".into()),
        ignition_time: t(2025, 8, 13, 4, 0),
        ignition_position: [-7.824, 40.230],
        ignition_basis: "Piódão village coordinates; reports place the start on the slope above the village around 05:00 local (approximate).".into(),
        detection_time: t(2025, 8, 13, 4, 8),
        resolved_time: Some(t(2025, 8, 24, 12, 0)),
        final_area: vec![
            AreaFact { area_ha: perimeters.last().map_or(0.0, |p| p.area_ha), source: "GWIS last perimeter (GDACS episode 12)".into(), url: perimeters.last().map(|p| p.url.clone()).unwrap_or_default() },
            AreaFact { area_ha: 64_451.0, source: "ICNF provisional report (via Observador/Lusa)".into(), url: "https://observador.pt/2025/08/25/fogo-que-comecou-em-arganil-apresenta-a-maior-area-ardida-de-sempre-em-portugal/".into() },
        ],
        domain: domain.clone(),
        elevation_m: dem.elevation_m.iter().map(|z| z.round().clamp(-500.0, 4000.0) as i16).collect(),
        elevation_source: SourceRef { name: "Terrain Tiles (Mapzen / AWS Open Data), zoom 11, bilinear to 200 m".into(), url: crate::sources::TERRARIUM.into(), licence: Some("Open data; attribution to Mapzen and sources (SRTM, EU-DEM, …)".into()) },
        vegetation: VegetationZones {
            note: "Altitudinal zonation assumption for the Serra do Açor / Estrela slopes (OSM land cover unavailable): high pasture and rock above 1500 m, shrubland (mato, much of it regrowth after the 2017 fires) 800–1500 m, maritime pine 400–800 m, eucalyptus below 400 m. Replace with COS/OSM land cover when available.".into(),
            zones: vec![(1500.0, LandCover::Grassland), (800.0, LandCover::Shrubland), (400.0, LandCover::MaritimePine), (0.5, LandCover::Eucalyptus)],
            sources: vec![SourceRef { name: "Reported vegetation: mato (Renascença/Lusa)".into(), url: "https://rr.pt/noticia/pais/2025/08/13/trovoada-seca-provocou-fogo-em-arganil-junto-a-aldeia-do-piodao/436045/".into(), licence: None }],
        },
        perimeters,
        weather,
        weather_source: SourceRef { name: "ERA5 reanalysis via Open-Meteo historical weather API (9 points)".into(), url: weather_url, licence: Some("CC BY 4.0 (Open-Meteo); ERA5 © ECMWF / Copernicus".into()) },
        events,
        sources: vec![gdacs("wildfire event 1024588 (GWIS burned-area episodes)"), SourceRef { name: "GDACS media monitor (EMM)".into(), url: news_url, licence: None }],
    })
}

// ---------------------------------------------------------------------- replay

#[derive(Clone, Debug)]
pub struct ReplayConfig {
    pub members: u32,
    pub seed: u64,
    pub frame_step_h: i64,
}

impl Default for ReplayConfig {
    fn default() -> Self {
        Self {
            members: 12,
            seed: 2025,
            frame_step_h: 3,
        }
    }
}

struct Cycle {
    start: DateTime<Utc>,
    end_obs: Option<usize>,
    hours: Vec<HourConditions>,
    ensemble: EnsembleResult,
    initial: Option<Vec<bool>>,
}

const SMOKE_THRESHOLDS: [f64; 3] = [35.0, 150.0, 500.0];

fn run_cycle(
    ds: &Dataset,
    landscape: &Landscape,
    start: DateTime<Utc>,
    hours_needed: i64,
    initial: Option<Vec<bool>>,
    cfg: &ReplayConfig,
    salt: u64,
) -> Result<Cycle, FirecastError> {
    let frame = ds.frame();
    let n = ds.domain.size;
    let mut hours = Vec::new();
    for k in 0..=hours_needed {
        let at = start + Duration::hours(k);
        let wx = ds
            .weather
            .sample(ds.domain.centre, at)
            .ok_or_else(|| FirecastError::Compute("no weather".into()))?;
        let m = estimate(&MoistureInputs {
            time: at,
            temp_c: wx.temp_c,
            rh_pct: wx.rh_pct,
            precip_last_3h_mm: ds.weather.precip_before(ds.domain.centre, at, 3),
            vpd_kpa: wx.vpd_kpa,
            drought_live_reduction_pct: 0.0,
            drought_label: None,
        });
        hours.push(HourConditions {
            wind: wind_grid_for(&ds.weather, &frame, n, n, ds.domain.cell_m, at),
            moisture: m.moisture,
        });
    }
    let ignition = frame.to_local(ds.ignition_position);
    let mut ec = EnsembleConfig::new(
        ignition,
        100.0,
        hours_needed as f64 * 3600.0,
        cfg.members,
        cfg.seed ^ salt,
    );
    ec.initial_burned = initial.clone();
    let ensemble = run_ensemble(landscape, &hours, &ec).map_err(FirecastError::Compute)?;
    Ok(Cycle {
        start,
        end_obs: None,
        hours,
        ensemble,
        initial,
    })
}

fn probability_at(e: &EnsembleResult, t: f64) -> Vec<f64> {
    e.burn_probability(t)
}

/// Run the full replay and return the JSON consumed by the web UI.
pub fn replay(ds: &Dataset, cfg: &ReplayConfig) -> Result<Value, FirecastError> {
    ds.validate()?;
    if cfg.frame_step_h <= 0 || cfg.members == 0 {
        return Err(FirecastError::Config(
            "positive replay step and ensemble size required".into(),
        ));
    }
    let started = std::time::Instant::now();
    let frame = ds.frame();
    let (n, cell) = (ds.domain.size, ds.domain.cell_m);
    let dem = ds.dem();
    let landscape = Landscape {
        width: n,
        height: n,
        cell_m: cell,
        cover: ds.land_cover(),
        slope: (0..n * n).map(|k| dem.slope(k % n, k / n)).collect(),
    };
    let observed: Vec<Vec<bool>> = ds
        .perimeters
        .iter()
        .map(|p| rasterise(&p.geometry, &frame, n, cell))
        .collect();

    // Cycle 0 from the ignition point; cycle k from observed perimeter k − 1.
    let mut cycles = Vec::new();
    let last_t = ds
        .perimeters
        .last()
        .map_or(ds.ignition_time + Duration::hours(48), |p| p.time)
        + Duration::hours(6);
    let mut starts: Vec<(DateTime<Utc>, Option<usize>)> = vec![(ds.ignition_time, None)];
    starts.extend(
        ds.perimeters
            .iter()
            .enumerate()
            .map(|(i, p)| (p.time, Some(i))),
    );
    for (k, (start, obs)) in starts.iter().enumerate() {
        let next = starts.get(k + 1).map_or(last_t, |s| s.0);
        let hours_needed = ((next - *start).num_minutes() as f64 / 60.0).ceil() as i64 + 12;
        let mut c = run_cycle(
            ds,
            &landscape,
            *start,
            hours_needed.clamp(1, 96),
            obs.map(|i| observed[i].clone()),
            cfg,
            k as u64 * 7919,
        )?;
        c.end_obs = starts.get(k + 1).and_then(|s| s.1);
        cycles.push(c);
    }

    // Validation against the next observed perimeter.
    let cell_ha = cell * cell / 10_000.0;
    let mut validation = Vec::new();
    for (k, c) in cycles.iter().enumerate() {
        let Some(obs_i) = c.end_obs else { continue };
        let t_obs = (ds.perimeters[obs_i].time - c.start).num_seconds() as f64;
        let p = probability_at(&c.ensemble, t_obs);
        let predicted: Vec<bool> = p.iter().map(|v| *v >= 0.5).collect();
        let truth = &observed[obs_i];
        let base = c.initial.clone().unwrap_or_else(|| vec![false; n * n]);
        let g_obs: Vec<bool> = truth.iter().zip(&base).map(|(o, b)| *o && !*b).collect();
        let g_pred: Vec<bool> = predicted
            .iter()
            .zip(&base)
            .map(|(o, b)| *o && !*b)
            .collect();
        let covered = g_obs
            .iter()
            .zip(&p)
            .filter(|(g, v)| **g && **v >= 0.1)
            .count();
        let g_obs_n = g_obs.iter().filter(|v| **v).count();
        let centroid = |m: &[bool]| -> Option<(f64, f64)> {
            let idx: Vec<usize> = (0..m.len()).filter(|i| m[*i]).collect();
            (!idx.is_empty()).then(|| {
                (
                    idx.iter().map(|i| (i % n) as f64).sum::<f64>() / idx.len() as f64,
                    idx.iter().map(|i| (i / n) as f64).sum::<f64>() / idx.len() as f64,
                )
            })
        };
        let origin = centroid(&base).unwrap_or_else(|| {
            let (x, y) = frame.to_local(ds.ignition_position);
            (x / cell, y / cell)
        });
        let bearing = |m: &[bool]| {
            centroid(m).map(|(x, y)| round(bearing_of(x - origin.0, y - origin.1).to_degrees(), 0))
        };
        let areas = c.ensemble.areas_ha(t_obs);
        let q = |f: f64| areas[((areas.len() - 1) as f64 * f).round() as usize];
        validation.push(json!({
            "cycle": k, "from": c.start, "to": ds.perimeters[obs_i].time, "hours": round(t_obs / 3600.0, 1),
            "started_from": if c.initial.is_some() { "observed perimeter" } else { "ignition point" },
            "observed_area_ha": round(truth.iter().filter(|v| **v).count() as f64 * cell_ha, 0),
            "observed_growth_ha": round(g_obs_n as f64 * cell_ha, 0),
            "predicted_area_ha_p10": round(q(0.1), 0), "predicted_area_ha_p50": round(q(0.5), 0), "predicted_area_ha_p90": round(q(0.9), 0),
            "dice_total": dice(&predicted, truth).map(|v| round(v, 3)),
            "dice_growth": dice(&g_pred, &g_obs).map(|v| round(v, 3)),
            "growth_within_10pct_band": (g_obs_n > 0).then(|| round(covered as f64 / g_obs_n as f64, 3)),
            "observed_growth_bearing_deg": bearing(&g_obs), "predicted_growth_bearing_deg": bearing(&g_pred),
        }));
    }

    // Frames.
    let first = ds.ignition_time;
    let mut frames = Vec::new();
    let mut at = first;
    while at <= last_t {
        let ci = cycles.iter().rposition(|c| c.start <= at).unwrap_or(0);
        let c = &cycles[ci];
        let dt = (at - c.start).num_seconds() as f64;
        let mut frame_json = json!({"t": at, "cycle": ci});
        let obs_index = ds.perimeters.iter().rposition(|p| p.time <= at);
        frame_json["observed_index"] = json!(obs_index);
        // Modelled extent now and forecast fronts ahead.
        let mut predicted = json!({});
        for (key, prob) in [("p10", 0.1), ("p50", 0.5), ("p90", 0.9)] {
            predicted[key] = multipolygon(&frame, cell, &probability_at(&c.ensemble, dt), n, prob)
                .unwrap_or(Value::Null);
        }
        frame_json["predicted"] = predicted;
        let mut forecast = json!({});
        for h in [6, 12] {
            let tt = dt + f64::from(h) * 3600.0;
            if tt <= c.ensemble.horizon_s {
                forecast[format!("h{h}")] =
                    multipolygon(&frame, cell, &probability_at(&c.ensemble, tt), n, 0.5)
                        .unwrap_or(Value::Null);
            }
        }
        frame_json["forecast"] = forecast;
        let areas = c.ensemble.areas_ha(dt);
        let q = |f: f64| areas[((areas.len() - 1) as f64 * f).round() as usize];
        // Central-member fire state for smoke, drones and fire behaviour.
        let central = &c.ensemble.members[0];
        let burned: Vec<usize> = (0..n * n)
            .filter(|k| f64::from(central.arrival[*k]) <= dt)
            .collect();
        let window = 1800.0f64.min(dt.max(600.0));
        let active: Vec<usize> = burned
            .iter()
            .copied()
            .filter(|k| f64::from(central.arrival[*k]) > dt - window)
            .collect();
        let is_burned = |k: usize| f64::from(central.arrival[k]) <= dt;
        let perimeter_cells: Vec<usize> = burned
            .iter()
            .copied()
            .filter(|k| f64::from(central.arrival[*k]) > dt - 3.0 * 3600.0)
            .filter(|k| {
                let (x, y) = ((k % n) as isize, (k / n) as isize);
                [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(dx, dy)| {
                        let (nx, ny) = (x + dx, y + dy);
                        nx >= 0
                            && ny >= 0
                            && (nx as usize) < n
                            && (ny as usize) < n
                            && !is_burned(ny as usize * n + nx as usize)
                    })
            })
            .collect();
        let front_km = perimeter_cells.len() as f64 * cell / 1000.0;
        let growth_ha_h = active.len() as f64 * cell_ha / window * 3600.0;
        let (cx, cy) = if burned.is_empty() {
            frame.to_local(ds.ignition_position)
        } else {
            let nb = burned.len() as f64;
            (
                burned
                    .iter()
                    .map(|k| ((k % n) as f64 + 0.5) * cell)
                    .sum::<f64>()
                    / nb,
                burned
                    .iter()
                    .map(|k| ((k / n) as f64 + 0.5) * cell)
                    .sum::<f64>()
                    / nb,
            )
        };
        let centre_ll = frame.to_lon_lat(cx, cy);
        let wx = ds.weather.sample(centre_ll, at);
        let hour = ((dt / 3600.0) as usize).min(c.hours.len() - 1);
        let beds = beds_for(&c.hours, &central.perturbation);
        let (mut hpa, mut consumed, mut head_ros, mut head_int, mut crown) =
            (0.0, 0.0, 0.0f64, 0.0f64, 0.0f64);
        for k in &active {
            if let Some(bed) = beds[hour][usize::from(landscape.cover[*k].index())] {
                hpa += bed.heat_per_area * 11.3565;
                consumed += bed.consumed_kg_m2;
            }
            if let Some(s) =
                cell_spread(&landscape, &c.hours, &beds, &central.perturbation, hour, *k)
                && s.head_ros_mps > head_ros
            {
                head_ros = s.head_ros_mps;
                head_int = s.intensity_kw_m;
                crown = s.crown_fraction;
            }
        }
        let na = active.len().max(1) as f64;
        let (hpa, consumed) = (hpa / na, consumed / na);
        let area_m2 = burned.len() as f64 * cell * cell;
        let growth_m2_s = active.len() as f64 * cell * cell / window;
        let mut smoke_json = Vec::new();
        let mut transport = (0.0, 0.0);
        let mut smoke_field: Option<(LocalFrame, usize, f64, Vec<f64>)> = None;
        let mut planning_plume = Value::Null;
        if let Some(wx) = wx {
            let (elev, _) = solar::position(centre_ll, at);
            let stability = stability_class(wx.wind_mps, elev, wx.cloud_pct, wx.shortwave_wm2);
            let (tmps, tfrom) = match (wx.wind_upper_mps, wx.wind_upper_from_deg) {
                (Some(s), Some(d)) => (s, d),
                _ => (
                    wx.wind_mps * 10f64.powf(stability.wind_exponent()),
                    wx.wind_from_deg,
                ),
            };
            let toward = from_to_toward_deg(tfrom);
            transport = (toward, tmps);
            if growth_m2_s > 0.0 {
                let source = PlumeSource {
                    x: cx,
                    y: cy,
                    radius_m: (area_m2 / std::f64::consts::PI).sqrt().max(cell / 2.0),
                    emission_g_s: growth_m2_s * consumed * PM25_EMISSION_FACTOR_G_PER_KG,
                    heat_release_w: hpa * 1000.0 * growth_m2_s,
                    age_s: dt.max(3600.0),
                };
                let met = PlumeMet {
                    wind10_mps: wx.wind_mps,
                    transport_mps: tmps,
                    transport_toward: toward.to_radians(),
                    stability,
                    mixing_height_m: stability.default_mixing_height_m(),
                };
                let plume = summarise(&source, &met);
                planning_plume = json!({"source":source,"met":met});
                let (size, scell) = (121usize, 700.0);
                let extent = size as f64 * scell;
                let centre = (
                    cx + 0.35 * extent * met.transport_toward.sin(),
                    cy + 0.35 * extent * met.transport_toward.cos(),
                );
                let sframe =
                    LocalFrame::centred(frame.to_lon_lat(centre.0, centre.1), extent / 2.0);
                let (sx, sy) = sframe.to_local(centre_ll);
                let local = PlumeSource {
                    x: sx,
                    y: sy,
                    ..source
                };
                let values: Vec<f64> = (0..size * size)
                    .map(|k| {
                        concentration(
                            &local,
                            &met,
                            &plume,
                            ((k % size) as f64 + 0.5) * scell,
                            ((k / size) as f64 + 0.5) * scell,
                        )
                    })
                    .collect();
                for th in SMOKE_THRESHOLDS {
                    if let Some(g) = multipolygon(&sframe, scell, &values, size, th) {
                        smoke_json.push(json!({"threshold_ug_m3": th, "geometry": g}));
                    }
                }
                let axis = plume.max_length_m.min(0.6 * extent);
                frame_json["smoke_axis"] = line(
                    &frame,
                    (cx, cy),
                    (
                        cx + axis * met.transport_toward.sin(),
                        cy + axis * met.transport_toward.cos(),
                    ),
                );
                frame_json["plume"] = json!({"stability": stability.label(), "rise_m": round(plume.effective_height_m, 0), "heat_mw": round(source.heat_release_w / 1e6, 0), "pm25_t_h": round(source.emission_g_s * 3.6 / 1000.0, 1)});
                smoke_field = Some((sframe, size, scell, values));
            }
            frame_json["weather"] = json!({"temp_c": round(wx.temp_c, 1), "rh_pct": round(wx.rh_pct, 0), "wind_mps": round(wx.wind_mps, 1), "wind_from_deg": round(wx.wind_from_deg, 0), "gust_mps": wx.gust_mps.map(|g| round(g, 1)), "wind_100m_mps": wx.wind_upper_mps.map(|v| round(v, 1)), "wind_100m_from_deg": wx.wind_upper_from_deg.map(|v| round(v, 0))});
        }
        frame_json["smoke"] = json!(smoke_json);
        // Wind arrows on a 6 × 6 lattice.
        let span = n as f64 * cell;
        let mut arrows = Vec::new();
        for j in 0..6 {
            for i in 0..6 {
                let (x, y) = ((i as f64 + 0.5) / 6.0 * span, (j as f64 + 0.5) / 6.0 * span);
                if let Some(w) = ds.weather.sample(frame.to_lon_lat(x, y), at) {
                    let (e, nn) = w.wind_vector();
                    let len = 160.0 * w.wind_mps.clamp(2.0, 20.0);
                    let norm = w.wind_mps.max(1e-6);
                    arrows.push(json!({"path": line(&frame, (x, y), (x + e / norm * len, y + nn / norm * len))["coordinates"], "speed_mps": round(w.wind_mps, 1)}));
                }
            }
        }
        frame_json["wind"] = json!(arrows);
        let detected = at >= ds.detection_time;
        let horizon = (dt + 3600.).min(c.ensemble.horizon_s);
        let burn_1h = probability_at(&c.ensemble, horizon);
        let burn_6h = probability_at(&c.ensemble, (dt + 21600.).min(c.ensemble.horizon_s));
        let burn_mission = probability_at(
            &c.ensemble,
            (dt + crate::fleet::Airframe::default().endurance_s).min(c.ensemble.horizon_s),
        );
        let mut edge_m = 0.;
        let mut head_sectors = std::collections::BTreeSet::new();
        for k in 0..n * n {
            if burn_1h[k] < 0.5 {
                continue;
            }
            let (x, y) = (k % n, k / n);
            let mut edge = false;
            for (dx, dy) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
                let (xx, yy) = (x as isize + dx, y as isize + dy);
                if xx < 0
                    || yy < 0
                    || xx >= n as isize
                    || yy >= n as isize
                    || burn_1h[yy as usize * n + xx as usize] < 0.5
                {
                    edge_m += cell;
                    edge = true;
                }
            }
            if edge
                && let Some(s) =
                    cell_spread(&landscape, &c.hours, &beds, &central.perturbation, hour, k)
            {
                let az = bearing_of((x as f64 + 0.5) * cell - cx, (y as f64 + 0.5) * cell - cy);
                let delta = (az - s.head_bearing)
                    .sin()
                    .atan2((az - s.head_bearing).cos())
                    .abs();
                if s.head_ros_mps >= 0.1 && delta <= std::f64::consts::FRAC_PI_4 {
                    head_sectors.insert(
                        (az.rem_euclid(std::f64::consts::TAU) / std::f64::consts::FRAC_PI_4).floor()
                            as u8,
                    );
                }
            }
        }
        let sizing = crate::fleet::size(
            edge_m,
            head_sectors.len(),
            0,
            0.,
            &crate::fleet::Airframe::default(),
        )?;
        let drones = if detected { sizing.count } else { 0 };
        // Intermediate offline planning grids; the split public frames omit these
        // arrays and retain only derived outputs. Outside this domain is unknown.
        frame_json["planning"] = json!({"origin_lon_lat":frame.origin_lon_lat,"size":n,"cell_m":cell,
            "burn_1h":burn_1h,"burn_6h":burn_6h,"burn_mission":burn_mission,
            "front_1h_m":edge_m,"head_sectors":head_sectors.len().max(1),
            "head_ros_threshold_mps":0.1,"plume":planning_plume});
        frame_json["model"] = json!({"area_ha_p10": round(q(0.1), 0), "area_ha_p50": round(q(0.5), 0), "area_ha_p90": round(q(0.9), 0),
            "front_km": round(front_km, 1), "growth_ha_h": round(growth_ha_h, 0), "head_ros_m_min": round(head_ros * 60.0, 1),
            "head_intensity_kw_m": round(head_int, 0), "fire_type": crate::rothermel::fire_type(crown)});
        frame_json["drones"] = plan_fleet(
            &frame,
            n,
            cell,
            &dem,
            c,
            dt,
            (cx, cy),
            transport,
            &perimeter_cells,
            smoke_field.as_ref(),
            drones,
        );
        frames.push(frame_json);
        at += if at - first < Duration::hours(3) {
            Duration::minutes(30)
        } else {
            Duration::hours(cfg.frame_step_h)
        };
    }

    Ok(json!({
        "meta": {
            "id": ds.id, "name": ds.name, "description": ds.description, "glide": ds.glide,
            "ignition": {"time": ds.ignition_time, "position": ds.ignition_position, "basis": ds.ignition_basis},
            "detection_time": ds.detection_time, "resolved_time": ds.resolved_time, "final_area": ds.final_area,
            "domain": ds.domain, "vegetation": ds.vegetation, "weather_source": ds.weather_source, "elevation_source": ds.elevation_source,
            "sources": ds.sources, "members": cfg.members, "frame_step_h": cfg.frame_step_h, "runtime_ms": started.elapsed().as_millis() as u64,
            "model": crate::forecast::MODEL_VERSION,
            "method": "Forecast from the ignition point to the first observed perimeter, then re-initialised from each observed perimeter (GWIS) and scored against the next one with the Sørensen–Dice coefficient (1 = perfect overlap).",
            "early_frame_step_minutes":30,
            "fleet_rule": "Deterministic perimeter/head/exposure demand divided by duty cycle, with configured reserve (fleet::size). This intermediate replay keeps legacy local route graphics; the split generated demo uses sourced fixed hypothetical stations and smoke-blocking regional planning. Aircraft parameters are demonstration values.",
        },
        "perimeters": ds.perimeters.iter().map(|p| json!({"time": p.time, "area_ha": p.area_ha, "source": p.source, "url": p.url, "geometry": p.geometry})).collect::<Vec<_>>(),
        "events": ds.events,
        "validation": validation,
        "frames": frames,
    }))
}

#[allow(clippy::too_many_arguments)]
fn plan_fleet(
    frame: &LocalFrame,
    n: usize,
    cell: f64,
    dem: &Dem,
    c: &Cycle,
    dt: f64,
    centre: (f64, f64),
    transport: (f64, f64),
    perimeter_cells: &[usize],
    smoke: Option<&(LocalFrame, usize, f64, Vec<f64>)>,
    count: usize,
) -> Value {
    if count == 0 {
        return json!({"count": 0, "bases": [], "routes": []});
    }
    const PLAN_M: f64 = 400.0;
    let span = n as f64 * cell;
    let m = (span / PLAN_M) as usize;
    let ratio = PLAN_M / cell;
    let soon = c.ensemble.burn_probability(dt + 3600.0);
    let old = c.ensemble.burn_probability((dt - 6.0 * 3600.0).max(-1.0));
    let mut cells = Vec::with_capacity(m * m);
    for y in 0..m {
        for x in 0..m {
            let (fx, fy) = (
                ((x as f64 + 0.5) * ratio) as usize,
                ((y as f64 + 0.5) * ratio) as usize,
            );
            let k = fy.min(n - 1) * n + fx.min(n - 1);
            let ll = frame.to_lon_lat((x as f64 + 0.5) * PLAN_M, (y as f64 + 0.5) * PLAN_M);
            let conc = smoke.map_or(0.0, |(sf, size, sc, v)| {
                let (sx, sy) = sf.to_local(ll);
                let (i, j) = ((sx / sc).floor(), (sy / sc).floor());
                if i < 0.0 || j < 0.0 || i >= *size as f64 || j >= *size as f64 {
                    0.0
                } else {
                    v[j as usize * size + i as usize]
                }
            });
            cells.push(Cell {
                fuel: 0.5,
                humidity: 0.0,
                elevation_m: f64::from(dem.elevation_m[k]).clamp(-400.0, 8000.0),
                blocked: soon[k] >= 0.3 && old[k] < 0.9,
                // Planner multiplies travel time by (1 + penalty): 35 µg/m³ ≈ +23 %, ≥ 300 µg/m³ = ×3.
                smoke_penalty: (conc / 150.0).clamp(0.0, 2.0),
            });
        }
    }
    let Ok(mut grid) = Grid::new(m, m, PLAN_M, cells) else {
        return json!({"count": 0, "bases": [], "routes": []});
    };
    let to_cell = |x: f64, y: f64| CellPosition {
        x: ((x / PLAN_M).max(0.0) as usize).min(m - 1),
        y: ((y / PLAN_M).max(0.0) as usize).min(m - 1),
    };
    let (toward, speed) = transport;
    // Eight candidate staging bases, each 3 km beyond the modelled fire edge along
    // its bearing; bases in smoke (penalty > 0.2, about 30 µg/m³) are dropped.
    let central_arrival = &c.ensemble.members[0].arrival;
    let fine_at = |x: f64, y: f64| -> Option<usize> {
        (x >= 0.0 && y >= 0.0 && x < span && y < span)
            .then(|| ((y / cell) as usize).min(n - 1) * n + ((x / cell) as usize).min(n - 1))
    };
    let candidates: Vec<(f64, (f64, f64))> = (0..8)
        .filter_map(|k| {
            let b = f64::from(k) * std::f64::consts::FRAC_PI_4;
            // Outermost burning (or soon burning) cell along the whole ray.
            let mut last = 0.0;
            let mut d = 0.0;
            while d < span {
                let (x, y) = (centre.0 + d * b.sin(), centre.1 + d * b.cos());
                match fine_at(x, y) {
                    Some(i) if f64::from(central_arrival[i]) <= dt + 3600.0 || soon[i] >= 0.1 => {
                        last = d
                    }
                    Some(_) => {}
                    None => break,
                }
                d += PLAN_M;
            }
            let d = last + 3_000.0;
            let p = (
                (centre.0 + d * b.sin()).clamp(PLAN_M, span - PLAN_M),
                (centre.1 + d * b.cos()).clamp(PLAN_M, span - PLAN_M),
            );
            let pc = to_cell(p.0, p.1);
            grid.cell(pc)
                .filter(|c| !c.blocked)
                .map(|c| (c.smoke_penalty, p))
        })
        .collect::<Vec<(f64, (f64, f64))>>();
    // Prefer clean-air bases; when smoke surrounds the fire keep the three cleanest.
    let mut ranked = candidates;
    ranked.sort_by(|a, b| a.0.total_cmp(&b.0));
    let clean = ranked
        .iter()
        .filter(|(pen, _)| *pen <= 0.2)
        .count()
        .max(3.min(ranked.len()));
    let bases: Vec<(f64, f64)> = ranked.into_iter().take(clean).map(|(_, p)| p).collect();
    let _ = toward;
    for b in &bases {
        let _ = grid.set_blocked(to_cell(b.0, b.1), false);
    }
    // Task points: active front cells sorted by angle, excluding the head sector.
    let head = toward.to_radians();
    let mut candidates: Vec<(f64, f64, f64)> = perimeter_cells
        .iter()
        .map(|k| (((k % n) as f64 + 0.5) * cell, ((k / n) as f64 + 0.5) * cell))
        .filter_map(|(x, y)| {
            let ang = (x - centre.0).atan2(y - centre.1);
            let rel = (ang - head).sin().atan2((ang - head).cos()).abs();
            (rel > 35f64.to_radians()).then_some((ang, x, y))
        })
        .collect();
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    let targets: Vec<(f64, f64)> = if candidates.is_empty() {
        vec![
            (
                centre.0 + 800.0 * (head + 1.6).sin(),
                centre.1 + 800.0 * (head + 1.6).cos()
            );
            count.min(2)
        ]
    } else {
        (0..count)
            .map(|i| candidates[i * candidates.len() / count])
            .map(|(_, x, y)| (x, y))
            .collect()
    };
    let wind = WindVector {
        east_mps: speed * toward.to_radians().sin(),
        north_mps: speed * toward.to_radians().cos(),
    };
    let mut routes = Vec::new();
    let mut held: std::collections::BTreeMap<String, usize> = Default::default();
    for (i, (tx, ty)) in targets.iter().enumerate() {
        let mut target = to_cell(*tx, *ty);
        // Step outward until outside the no-fly area.
        let (dx, dy) = (tx - centre.0, ty - centre.1);
        let norm = dx.hypot(dy).max(1.0);
        for step in 0..12 {
            if !grid.cell(target).is_some_and(|c| c.blocked) {
                break;
            }
            target = to_cell(
                tx + dx / norm * PLAN_M * step as f64,
                ty + dy / norm * PLAN_M * step as f64,
            );
        }
        let mut last_error: Option<String> = None;
        let mut routed = false;
        let mut order: Vec<usize> = (0..bases.len()).collect();
        order.sort_by(|a, b| {
            (bases[*a].0 - tx)
                .hypot(bases[*a].1 - ty)
                .total_cmp(&(bases[*b].0 - tx).hypot(bases[*b].1 - ty))
        });
        for bi in order {
            let start = to_cell(bases[bi].0, bases[bi].1);
            if start == target {
                continue;
            }
            // Demonstration airframes: multirotor scouts (15 m/s, 60 min) and fixed-wing
            // VTOL perimeter drones (18 m/s, 90 min); 20 % reserve, 5-minute task.
            let (cruise, endurance) = if i < 2 {
                (15.0, 3600.0)
            } else {
                (18.0, 5400.0)
            };
            let request = MissionRequest {
                start,
                target,
                wind,
                cruise_mps: cruise,
                max_gust_mps: 25.0,
                gust_mps: None,
                battery_capacity_seconds: endurance,
                battery_pct: 100.0,
                reserve_pct: 20.0,
                task_seconds: 300.0,
                climb_mps: 3.0,
            };
            let planned = plan_mission(&grid, &request);
            if let Err(e) = &planned {
                last_error = Some(format!("{e}"));
            }
            if let Ok(mission) = planned {
                let path: Vec<[f64; 2]> = mission
                    .outbound
                    .cells
                    .iter()
                    .map(|p| {
                        let ll = frame
                            .to_lon_lat((p.x as f64 + 0.5) * PLAN_M, (p.y as f64 + 0.5) * PLAN_M);
                        [round(ll[0], 5), round(ll[1], 5)]
                    })
                    .collect();
                routes.push(json!({"path": path, "base": bi, "role": if i < 2 { "scout" } else { "perimeter" }, "eta_min": round(mission.outbound.duration_seconds / 60.0, 0)}));
                routed = true;
                break;
            }
        }
        if !routed {
            *held
                .entry(last_error.unwrap_or_else(|| "no base".into()))
                .or_default() += 1;
        }
    }
    json!({
        "count": count,
        "bases": bases.iter().map(|b| frame.to_lon_lat(b.0, b.1)).collect::<Vec<_>>(),
        "routes": routes,
        "held": held,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rasterise_square_and_hole_with_even_odd() {
        let frame = LocalFrame {
            origin_lon_lat: [-7.9, 40.1],
        };
        let ring = |a: f64, b: f64| {
            let pts = [(a, a), (b, a), (b, b), (a, b), (a, a)];
            json!(
                pts.iter()
                    .map(|(x, y)| frame.to_lon_lat(*x, *y))
                    .collect::<Vec<_>>()
            )
        };
        let geom = json!({"type":"Polygon","coordinates":[ring(0.0, 1000.0), ring(400.0, 600.0)]});
        let mask = rasterise(&geom, &frame, 10, 100.0);
        assert!(mask[0] && mask[9 * 10 + 9] && !mask[5 * 10 + 4]);
        assert_eq!(mask.iter().filter(|v| **v).count(), 96);
        let area = geodesic_area_ha(&geom);
        assert!((area - 96.0).abs() < 1.0, "{area}");
    }

    fn synthetic_dataset() -> Dataset {
        let domain = Domain {
            centre: [-7.8, 40.2],
            cell_m: 200.0,
            size: 61,
        };
        let frame = LocalFrame::centred(domain.centre, 61.0 * 200.0 / 2.0);
        let square = |half: f64| {
            let c = 6100.0;
            let pts = [
                (c - half, c - half),
                (c + half, c - half),
                (c + half, c + half),
                (c - half, c + half),
                (c - half, c - half),
            ];
            json!({"type":"Polygon","coordinates":[pts.iter().map(|(x, y)| frame.to_lon_lat(*x, *y)).collect::<Vec<_>>()]})
        };
        let start = t(2025, 8, 13, 10, 0);
        let weather = crate::environment::synthetic_weather(domain.centre, start, 40);
        let perimeter = |h: i64, half: f64| Perimeter {
            time: start + Duration::hours(h),
            area_ha: (2.0 * half).powi(2) / 1e4,
            source: "test".into(),
            url: "https://example.org".into(),
            geometry: square(half),
        };
        Dataset {
            id: "test".into(),
            name: "Synthetic test fire".into(),
            description: String::new(),
            glide: None,
            ignition_time: start,
            ignition_position: domain.centre,
            ignition_basis: "test".into(),
            detection_time: start + Duration::minutes(10),
            resolved_time: None,
            final_area: vec![],
            elevation_m: (0..61 * 61).map(|k| 300 + (k % 61) as i16 * 3).collect(),
            elevation_source: SourceRef {
                name: "synthetic".into(),
                url: "https://example.org".into(),
                licence: None,
            },
            vegetation: VegetationZones {
                note: "test".into(),
                zones: vec![(0.5, LandCover::Shrubland)],
                sources: vec![],
            },
            perimeters: vec![perimeter(6, 600.0), perimeter(12, 1200.0)],
            weather,
            weather_source: SourceRef {
                name: "synthetic".into(),
                url: "https://example.org".into(),
                licence: None,
            },
            events: arganil_2025_events().into_iter().take(2).collect(),
            sources: vec![],
            domain,
        }
    }

    #[test]
    fn replay_validates_against_perimeters_and_scales_the_fleet() {
        let ds = synthetic_dataset();
        let out = replay(
            &ds,
            &ReplayConfig {
                members: 4,
                seed: 1,
                frame_step_h: 2,
            },
        )
        .unwrap();
        let validation = out["validation"].as_array().unwrap();
        assert_eq!(validation.len(), 2, "ignition→P1 and P1→P2");
        assert_eq!(validation[0]["started_from"], "ignition point");
        assert_eq!(validation[1]["started_from"], "observed perimeter");
        for v in validation {
            let d = v["dice_total"].as_f64().unwrap();
            assert!((0.0..=1.0).contains(&d));
        }
        let frames = out["frames"].as_array().unwrap();
        assert!(frames.len() >= 9);
        assert_eq!(
            frames[0]["drones"]["count"], 0,
            "no drones before detection"
        );
        assert!(
            frames
                .iter()
                .skip(1)
                .all(|f| f["drones"]["count"].as_u64().unwrap() >= 2)
        );
        // Re-initialised frames carry the observed perimeter index.
        assert!(frames.iter().any(|f| f["observed_index"] == 1));
        assert!(frames.iter().any(|f| f["predicted"]["p50"].is_object()));
    }

    #[test]
    fn dice_and_fleet_rule() {
        assert_eq!(
            dice(&[true, true, false], &[true, false, false]),
            Some(2.0 / 3.0)
        );
        assert_eq!(dice(&[false], &[false]), None);
        let cfg = crate::fleet::Airframe::default();
        assert_eq!(crate::fleet::size(0., 1, 0, 0., &cfg).unwrap().count, 2);
        let small = crate::fleet::size(40_000., 1, 0, 0., &cfg).unwrap().count;
        assert!(crate::fleet::size(400_000., 1, 0, 0., &cfg).unwrap().count > small);
    }

    #[test]
    fn gdacs_parsers_extract_dated_perimeters_and_portuguese_news() {
        let geo = json!({"features":[{"properties":{"Class":"Point_Centroid"},"geometry":{"type":"Point","coordinates":[-7.7,40.2]}},
            {"properties":{"Class":"Poly_area","polygondate":"2025-08-16T01:04:55"},"geometry":{"type":"Polygon","coordinates":[[[-7.8,40.2],[-7.7,40.2],[-7.7,40.3],[-7.8,40.3],[-7.8,40.2]]]}}]});
        let p = parse_gdacs_perimeter(&geo, "https://www.gdacs.org/x").unwrap();
        assert_eq!(p.time, t(2025, 8, 16, 1, 4) + Duration::seconds(55));
        assert!(p.area_ha > 9000.0 && p.area_ha < 10_000.0, "{}", p.area_ha);
        let news = json!([{"pubdate":"2025-08-20T12:47:00","source":"ansa","link":"https://a","title":"Emergenza incendi, tre morti in Portogallo"},
            {"pubdate":"2025-08-20T12:00:00","source":"x","link":"https://b","title":"Incendios en España"}]);
        let events = parse_gdacs_news(&news, t(2025, 8, 13, 0, 0), t(2025, 8, 21, 0, 0));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].origin, "gdacs_emm");
        assert!(
            arganil_2025_events()
                .iter()
                .all(|e| e.url.starts_with("https://"))
        );
    }
}
