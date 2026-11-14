use chrono::{DateTime, NaiveDateTime, Utc};
use emberwatch_domain::platform::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

pub const MAX_UPLOAD_BYTES: usize = 2 * 1024 * 1024;
const MAX_UPLOAD_ROWS: usize = 10_000;

pub fn parse_upload(
    format: &str,
    text: &str,
    source_id: &str,
    captured_at: DateTime<Utc>,
) -> Result<SourceBatch, String> {
    if text.len() > MAX_UPLOAD_BYTES {
        return Err("upload exceeds 2 MiB".into());
    }
    if !source_id.starts_with("upload:") || source_id.len() > 128 {
        return Err("upload source_id must start with upload: and be at most 128 bytes".into());
    }
    let mut evidence = match format {
        "jsonl" => {
            let mut rows = Vec::new();
            for (index, line) in text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .enumerate()
            {
                if index >= MAX_UPLOAD_ROWS {
                    return Err("upload exceeds 10000 records".into());
                }
                let row: Evidence =
                    serde_json::from_str(line).map_err(|e| format!("row {}: {e}", index + 1))?;
                rows.push(row);
            }
            rows
        }
        "csv" => parse_firms_csv(text, source_id, captured_at)?,
        "geojson" => {
            let document: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
            if document["type"] != "FeatureCollection" {
                return Err("GeoJSON must be a FeatureCollection".into());
            }
            let features = document["features"]
                .as_array()
                .ok_or("missing features array")?;
            if features.len() > MAX_UPLOAD_ROWS {
                return Err("upload exceeds 10000 records".into());
            }
            let mut rows = Vec::new();
            for (index, feature) in features.iter().enumerate() {
                if feature["geometry"]["type"] != "Point" {
                    return Err(format!(
                        "feature {index}: this importer accepts Point geometry only"
                    ));
                }
                let coordinates = feature["geometry"]["coordinates"]
                    .as_array()
                    .ok_or("missing coordinates")?;
                if coordinates.len() != 2 {
                    return Err("Point must have longitude and latitude".into());
                }
                let mut row: Evidence = serde_json::from_value(feature["properties"].clone())
                    .map_err(|e| format!("feature {index} properties: {e}"))?;
                row.point = Some(Point {
                    longitude: coordinates[0].as_f64().ok_or("invalid longitude")?,
                    latitude: coordinates[1].as_f64().ok_or("invalid latitude")?,
                });
                rows.push(row);
            }
            rows
        }
        _ => return Err("format must be csv, geojson or jsonl".into()),
    };
    for row in &mut evidence {
        row.source_id = source_id.into();
        row.id = Uuid::nil();
        row.validate()?;
    }
    let empty = evidence.is_empty();
    Ok(SourceBatch {
        source_id: source_id.into(),
        evidence,
        weather: vec![],
        coverage: Coverage {
            complete: true,
            explicit_empty: empty,
            note: Some(
                "User-supplied dataset; rights/provenance are the uploader's responsibility".into(),
            ),
        },
        captured_at,
    })
}

pub fn parse_firms_csv(
    text: &str,
    source_id: &str,
    captured_at: DateTime<Utc>,
) -> Result<Vec<Evidence>, String> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(text.as_bytes());
    let headers = reader.headers().map_err(|e| e.to_string())?.clone();
    for name in ["latitude", "longitude", "acq_date", "acq_time"] {
        if !headers.iter().any(|header| header == name) {
            return Err(format!("missing FIRMS column {name}"));
        }
    }
    let mut evidence = Vec::new();
    for (index, record) in reader.records().enumerate() {
        if index >= MAX_UPLOAD_ROWS {
            return Err("CSV exceeds 10000 records".into());
        }
        let record = record.map_err(|e| format!("CSV row {}: {e}", index + 2))?;
        let get = |name: &str| -> &str {
            headers
                .iter()
                .position(|h| h == name)
                .and_then(|i| record.get(i))
                .unwrap_or("")
        };
        let parse = |name: &str| {
            get(name)
                .parse::<f64>()
                .map_err(|_| format!("row {}: invalid {name}", index + 2))
        };
        let point = Point {
            latitude: parse("latitude")?,
            longitude: parse("longitude")?,
        };
        point.validate()?;
        let time = format!("{:0>4}", get("acq_time"));
        let naive =
            NaiveDateTime::parse_from_str(&format!("{} {time}", get("acq_date")), "%Y-%m-%d %H%M")
                .map_err(|_| format!("row {}: invalid acquisition date/time", index + 2))?;
        let observed_at = naive.and_utc();
        let mut attrs = BTreeMap::new();
        attrs.insert("satellite".into(), json!(get("satellite")));
        attrs.insert("confidence".into(), json!(get("confidence")));
        if let Ok(frp) = get("frp").parse::<f64>()
            && frp.is_finite()
            && frp >= 0.0
        {
            attrs.insert("frp_mw".into(), json!(frp));
        }
        let row = Evidence {
            id: Uuid::nil(),
            source_id: source_id.into(),
            external_id: format!(
                "{}:{}:{}:{}",
                get("satellite"),
                observed_at,
                point.longitude,
                point.latitude
            ),
            origin_group: "nasa-firms".into(),
            kind: EvidenceKind::Satellite,
            url: "https://firms.modaps.eosdis.nasa.gov/".into(),
            title: "FIRMS thermal observation".into(),
            place_name: "Satellite observation".into(),
            municipality: None,
            point: Some(point),
            radius_m: 500.0,
            observed_at: Some(observed_at),
            published_at: None,
            available_at: captured_at,
            availability_basis: AvailabilityBasis::Captured,
            captured_at,
            status: OperationalStatus::Unknown,
            raw_status: None,
            is_official: false,
            attrs,
        };
        row.validate()?;
        evidence.push(row);
    }
    Ok(evidence)
}
