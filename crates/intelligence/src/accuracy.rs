//! Deterministic checks over retained public evidence. A model's confidence is not evidence.
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Europe::Lisbon;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use unicode_normalization::UnicodeNormalization;

pub fn normalize(text: &str) -> String {
    text.nfc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn quote_verified(markdown: &str, quote: &str) -> bool {
    let quote = normalize(quote);
    !quote.is_empty() && normalize(markdown).contains(&quote)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quantity {
    pub value: f64,
    pub upper: Option<f64>,
    pub approximate: bool,
    pub lower_bound: bool,
}

fn numeric(token: &str) -> Option<f64> {
    let token = token.trim_matches(['.', ',']).replace(' ', "");
    let clean = if token.contains(',') {
        token.replace('.', "").replace(',', ".")
    } else if token.split('.').skip(1).all(|s| s.len() == 3) {
        token.replace('.', "")
    } else {
        token
    };
    clean
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
}
pub fn numeric_values(text: &str) -> Vec<f64> {
    let chars: Vec<_> = normalize(text).chars().collect();
    let mut values = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < chars.len() {
            if chars[i].is_ascii_digit() || matches!(chars[i], '.' | ',') {
                i += 1;
                continue;
            }
            if chars[i] == ' ' {
                let digits = chars
                    .iter()
                    .skip(i + 1)
                    .take_while(|c| c.is_ascii_digit())
                    .count();
                if digits == 3 {
                    i += 1;
                    continue;
                }
            }
            break;
        }
        if let Some(value) = numeric(&chars[start..i].iter().collect::<String>()) {
            values.push(value);
        }
    }
    values
}
fn word_value(word: &str) -> Option<f64> {
    Some(match word {
        "um" | "uma" => 1.,
        "dois" | "duas" => 2.,
        "três" | "tres" => 3.,
        "quatro" => 4.,
        "cinco" => 5.,
        "seis" => 6.,
        "sete" => 7.,
        "oito" => 8.,
        "nove" => 9.,
        "dez" => 10.,
        "onze" => 11.,
        "doze" => 12.,
        "treze" => 13.,
        "catorze" | "quatorze" => 14.,
        "quinze" => 15.,
        "dezasseis" => 16.,
        "dezassete" => 17.,
        "dezoito" => 18.,
        "dezanove" => 19.,
        "vinte" => 20.,
        "trinta" => 30.,
        "quarenta" => 40.,
        "cinquenta" => 50.,
        "sessenta" => 60.,
        "setenta" => 70.,
        "oitenta" => 80.,
        "noventa" => 90.,
        "cem" | "cento" => 100.,
        "duzentos" => 200.,
        "trezentos" => 300.,
        "quatrocentos" => 400.,
        "quinhentos" => 500.,
        "seiscentos" => 600.,
        "setecentos" => 700.,
        "oitocentos" => 800.,
        "novecentos" => 900.,
        _ => return None,
    })
}
pub fn quantities(text: &str) -> Vec<Quantity> {
    let lower = normalize(text).to_lowercase();
    let mut numbers = numeric_values(&lower);
    if numbers.is_empty() {
        let words: Vec<_> = lower
            .split(|c: char| !c.is_alphabetic())
            .filter(|s| !s.is_empty())
            .collect();
        let mut value = 0.;
        let mut active = false;
        for word in words.into_iter().chain([""]) {
            if let Some(n) = word_value(word) {
                value += n;
                active = true;
            } else if word == "mil" {
                value = value.max(1.) * 1000.;
                active = true;
            } else if word == "centenas" {
                value = value.max(1.) * 100.;
                active = true;
            } else if word != "e" && active {
                numbers.push(value);
                value = 0.;
                active = false;
            }
        }
    }
    let approximate = [
        "cerca de",
        "aproximadamente",
        "quase",
        "mais de",
        "menos de",
    ]
    .iter()
    .any(|s| lower.contains(s));
    let lower_bound = lower.contains("mais de") || lower.contains("pelo menos");
    if numbers.len() == 2
        && (lower.contains("entre ") || lower.contains(" a ") || lower.contains('-'))
    {
        return vec![Quantity {
            value: numbers[0],
            upper: Some(numbers[1]),
            approximate,
            lower_bound,
        }];
    }
    numbers
        .into_iter()
        .map(|value| Quantity {
            value,
            upper: None,
            approximate,
            lower_bound,
        })
        .collect()
}
pub fn number_supported(value: f64, quote: &str) -> bool {
    value.is_finite()
        && quantities(quote).iter().any(|q| {
            (q.value - value).abs() < 1e-9 || q.upper.is_some_and(|n| (n - value).abs() < 1e-9)
        })
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Personnel,
    Vehicles,
    Aircraft,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceObservation {
    pub kind: ResourceKind,
    pub quantity: Quantity,
    pub quote: String,
    pub event_time: Option<DateTime<Utc>>,
}
pub fn resource_observation(
    kind: ResourceKind,
    value: f64,
    quote: &str,
    markdown: &str,
    time_text: Option<&str>,
    published: Option<DateTime<Utc>>,
) -> Option<ResourceObservation> {
    if !quote_verified(markdown, quote) {
        return None;
    }
    let lower = normalize(quote).to_lowercase();
    let units: &[&str] = match kind {
        ResourceKind::Personnel => &["operacionais", "bombeiros"],
        ResourceKind::Vehicles => &["viaturas", "veículos", "meios terrestres"],
        ResourceKind::Aircraft => &["meios aéreos", "aeronaves", "helicópteros"],
    };
    for unit in units {
        for (index, _) in lower.match_indices(unit) {
            let prefix = &lower[..index];
            let words: Vec<_> = prefix
                .split_whitespace()
                .rev()
                .take_while(|s| {
                    numeric(s).is_some()
                        || word_value(s).is_some()
                        || matches!(
                            *s,
                            "e" | "mil"
                                | "centenas"
                                | "de"
                                | "mais"
                                | "cerca"
                                | "aproximadamente"
                                | "menos"
                                | "pelo"
                                | "pelo menos"
                        )
                })
                .collect();
            let expression = words.into_iter().rev().collect::<Vec<_>>().join(" ");
            for quantity in quantities(&expression) {
                if (quantity.value - value).abs() < 1e-9 {
                    let event_time = time_text.and_then(|t| {
                        relative_time(t, published).or_else(|| {
                            published.and_then(|p| {
                                lisbon_time(
                                    p.with_timezone(&Lisbon).date_naive(),
                                    *clock_times(t).first()?,
                                )
                            })
                        })
                    });
                    return Some(ResourceObservation {
                        kind,
                        quantity,
                        quote: quote.into(),
                        event_time,
                    });
                }
            }
        }
    }
    None
}

pub fn lisbon_time(date: NaiveDate, time: NaiveTime) -> Option<DateTime<Utc>> {
    // Ambiguous/nonexistent DST clock readings remain unresolved.
    Lisbon
        .from_local_datetime(&date.and_time(time))
        .single()
        .map(|t| t.with_timezone(&Utc))
}
pub fn clock_times(text: &str) -> Vec<NaiveTime> {
    let text = text.to_lowercase().replace(" h", "h");
    let c: Vec<_> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        if !c[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < c.len() && c[i].is_ascii_digit() {
            i += 1;
        }
        if i - start > 2 || i == c.len() || !matches!(c[i], ':' | 'h') {
            continue;
        }
        let hour = c[start..i].iter().collect::<String>().parse().ok();
        i += 1;
        while i < c.len() && c[i] == ' ' {
            i += 1;
        }
        let ms = i;
        while i < c.len() && c[i].is_ascii_digit() {
            i += 1;
        }
        let minute = if i == ms {
            Some(0)
        } else {
            c[ms..i].iter().collect::<String>().parse().ok()
        };
        if let (Some(h), Some(m)) = (hour, minute)
            && let Some(t) = NaiveTime::from_hms_opt(h, m, 0)
        {
            out.push(t);
        }
    }
    out
}
fn month(text: &str) -> Option<u32> {
    [
        "jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez",
    ]
    .iter()
    .position(|m| text.starts_with(m))
    .map(|i| i as u32 + 1)
}
pub fn printed_date(text: &str) -> Option<NaiveDate> {
    let lower = text.to_lowercase();
    let t: Vec<_> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    let yi = t
        .iter()
        .position(|s| s.len() == 4 && s.parse::<i32>().is_ok_and(|y| (1900..=2200).contains(&y)))?;
    let year = t[yi].parse().ok()?;
    if let Some((mi, mon)) = t
        .iter()
        .enumerate()
        .take(yi)
        .find_map(|(i, s)| month(s).map(|m| (i, m)))
    {
        let day = t[..mi]
            .iter()
            .rev()
            .chain(t[mi + 1..yi].iter())
            .find_map(|s| s.parse::<u32>().ok().filter(|d| (1..=31).contains(d)))?;
        NaiveDate::from_ymd_opt(year, mon, day)
    } else if yi >= 2 {
        NaiveDate::from_ymd_opt(year, t[yi - 1].parse().ok()?, t[yi - 2].parse().ok()?)
    } else {
        None
    }
}
pub fn printed_time(text: &str) -> Option<DateTime<Utc>> {
    lisbon_time(printed_date(text)?, *clock_times(text).last()?)
}
pub fn relative_time(text: &str, published: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    if let Some(t) = printed_time(text) {
        return Some(t);
    }
    let publication = published?;
    let local = publication.with_timezone(&Lisbon);
    let lower = text.to_lowercase();
    let date = if lower.contains("ontem") {
        local.date_naive().pred_opt()?
    } else if lower.contains("hoje") || lower.contains("esta ") || lower.contains("desta ") {
        local.date_naive()
    } else {
        return None;
    };
    // "esta madrugada" has no exact clock time, so it remains null.
    lisbon_time(date, *clock_times(text).first()?)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Availability {
    pub published_at: Option<DateTime<Utc>>,
    pub known_at: Option<DateTime<Utc>>,
    pub flags: Vec<String>,
}
pub fn availability(
    metadata_published: Option<&str>,
    metadata_modified: Option<&str>,
    printed: Option<&str>,
    printed_in_page: bool,
    quotes: &[&str],
) -> Availability {
    let parse = |s: &str| {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|t| t.with_timezone(&Utc))
    };
    let meta = metadata_published.and_then(parse);
    let modified = metadata_modified.and_then(parse);
    let raw = printed.filter(|_| printed_in_page);
    let parsed = raw.and_then(printed_time);
    let mut flags = Vec::new();
    if printed.is_some() && !printed_in_page {
        flags.push("printed_date_not_found_in_page".into());
    }
    if meta.zip(parsed).is_some_and(|(a, b)| a != b) {
        flags.push("printed_metadata_disagree_later_used".into());
    }
    let date = raw
        .and_then(printed_date)
        .or(meta.map(|t| t.with_timezone(&Lisbon).date_naive()));
    let publication = meta.into_iter().chain(parsed).max().or_else(|| {
        date.and_then(|d| {
            flags.push("date_only_end_of_local_day".into());
            lisbon_time(d, NaiveTime::from_hms_opt(23, 59, 59).unwrap())
        })
    });
    let mut known = publication.into_iter().chain(modified).max();
    if let Some(day) = date {
        for quote in quotes {
            for clock in clock_times(quote) {
                if let Some(time) = lisbon_time(day, clock)
                    && known.is_some_and(|t| time > t)
                {
                    known = Some(time);
                    flags.push("later_clock_in_content".into());
                }
            }
        }
    }
    Availability {
        published_at: publication,
        known_at: known,
        flags,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Place {
    pub name: String,
    pub municipality: String,
    pub position: [f64; 2],
    pub source_url: String,
}
pub fn resolve_place<'a>(
    name: &str,
    municipality: Option<&str>,
    places: &'a [Place],
) -> Option<&'a Place> {
    let name = normalize(name).to_lowercase();
    let mut matches = places.iter().filter(|p| {
        normalize(&p.name).to_lowercase() == name
            && municipality.is_none_or(|m| {
                normalize(&p.municipality).to_lowercase() == normalize(m).to_lowercase()
            })
            && (-9.6..=-6.1).contains(&p.position[0])
            && (36.9..=42.2).contains(&p.position[1])
    });
    let found = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(found)
    }
}
pub fn same_lineage(headline_a: &str, body_a: &str, headline_b: &str, body_b: &str) -> bool {
    let title = |s: &str| {
        normalize(s)
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
    };
    let a = title(headline_a);
    let b = title(headline_b);
    if !a.is_empty() && a == b {
        return true;
    }
    if !body_a.to_lowercase().contains("lusa") || !body_b.to_lowercase().contains("lusa") {
        return false;
    }
    let words = |s: &str| {
        normalize(s)
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    };
    let a = words(body_a);
    let b = words(body_b);
    let union = a.union(&b).count();
    union > 0 && a.intersection(&b).count() as f64 / union as f64 >= 0.9
}
pub fn named_event(quote: &str) -> bool {
    let t = quote.to_lowercase();
    [
        "festa de ",
        "festa do ",
        "festa da ",
        "feira de ",
        "romaria de ",
        "festival de ",
        "arraial de ",
    ]
    .iter()
    .any(|s| t.contains(s))
}
pub fn next_weekday(published: DateTime<Utc>, weekday: u32) -> Option<NaiveDate> {
    if weekday > 6 {
        return None;
    }
    let day = published.with_timezone(&Lisbon).date_naive();
    let ahead = (weekday + 7 - day.weekday().num_days_from_monday()) % 7;
    day.checked_add_signed(Duration::days(if ahead == 0 {
        7
    } else {
        i64::from(ahead)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resource_units_do_not_borrow_a_nearby_vehicle_count() {
        let quote = "367 bombeiros, apoiados por 129 viaturas e ainda cinco meios aéreos";
        assert!(
            resource_observation(ResourceKind::Personnel, 367., quote, quote, None, None).is_some()
        );
        assert!(
            resource_observation(ResourceKind::Aircraft, 5., quote, quote, None, None).is_some()
        );
        assert!(
            resource_observation(ResourceKind::Personnel, 129., quote, quote, None, None).is_none()
        );
        assert!(
            resource_observation(ResourceKind::Aircraft, 129., quote, quote, None, None).is_none()
        );
    }
    #[test]
    fn homonymous_places_require_municipality_context() {
        // Deliberately synthetic resolver fixtures, never demo/source data.
        let places = vec![
            Place {
                name: "Lugar sintético".into(),
                municipality: "A".into(),
                position: [-8., 40.],
                source_url: "https://example.org/synthetic-a".into(),
            },
            Place {
                name: "Lugar sintético".into(),
                municipality: "B".into(),
                position: [-8.1, 40.1],
                source_url: "https://example.org/synthetic-b".into(),
            },
        ];
        assert!(resolve_place("Lugar sintético", None, &places).is_none());
        assert_eq!(
            resolve_place("Lugar sintético", Some("B"), &places)
                .unwrap()
                .municipality,
            "B"
        );
    }
    #[test]
    fn unicode_and_whitespace_are_not_rephrasing() {
        assert!(quote_verified("Pio\u{301}dão\u{a0}arde", "Piódão arde"));
        assert!(!quote_verified("Não há evacuação", "Há evacuação"));
        assert!(!quote_verified("abc", ""));
    }
    #[test]
    fn portuguese_numbers_qualifiers_ranges() {
        assert_eq!(quantities("1.234,5 ha")[0].value, 1234.5);
        assert_eq!(quantities("1 234 ha")[0].value, 1234.);
        assert_eq!(quantities("mil hectares")[0].value, 1000.);
        assert_eq!(quantities("cinco centenas")[0].value, 500.);
        assert!(quantities("mais de 300 bombeiros")[0].lower_bound);
        assert!(quantities("cerca de 300")[0].approximate);
        assert_eq!(quantities("entre 10 e 20")[0].upper, Some(20.));
        assert!(!number_supported(50., "150 bombeiros"));
    }
    #[test]
    fn dst_and_relative_time() {
        assert_eq!(
            printed_time("13 agosto 2025 às 13h31")
                .unwrap()
                .to_rfc3339(),
            "2025-08-13T12:31:00+00:00"
        );
        assert_eq!(printed_time("13 janeiro 2025 às 13:31").unwrap().hour(), 13);
        assert!(relative_time("ontem às 12h", None).is_none());
        assert!(relative_time("esta madrugada", printed_time("13 agosto 2025 às 13h")).is_none());
        assert!(
            lisbon_time(
                NaiveDate::from_ymd_opt(2025, 3, 30).unwrap(),
                NaiveTime::from_hms_opt(1, 30, 0).unwrap()
            )
            .is_none()
        );
    }
    #[test]
    fn availability_never_backdates_later_updates() {
        let a = availability(
            Some("2025-08-13T06:59:00Z"),
            None,
            Some("13 agosto 2025 às 07h59"),
            true,
            &["Segundo ANEPC às 17h"],
        );
        assert_eq!(
            a.known_at.unwrap().to_rfc3339(),
            "2025-08-13T16:00:00+00:00"
        );
        let a = availability(None, None, Some("13 agosto 2025"), true, &[]);
        assert_eq!(
            a.known_at.unwrap().to_rfc3339(),
            "2025-08-13T22:59:59+00:00"
        );
    }
    #[test]
    fn agency_copies_and_events() {
        assert!(same_lineage(
            "Incêndio em Piódão",
            "Lusa",
            "Incêndio em Piódão",
            "Lusa"
        ));
        assert!(!same_lineage(
            "A",
            "Lusa texto diferente",
            "B",
            "Lusa outro artigo"
        ));
        assert!(!named_event("Moradores reunidos no largo"));
        assert!(named_event("Festa de Verão do Piódão cancelada"));
    }
    use chrono::Timelike;
}
