//! Configured demonstration priority proposal, separate from incident status.
use crate::ComputeError;
use emberwatch_domain::{RunContext, Weather};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeverityLevel {
    Unknown,
    Review,
    Elevated,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SeverityProposal {
    pub level: SeverityLevel,
    pub reasons: Vec<String>,
    pub input_weather_ids: Vec<String>,
    pub heuristic: bool,
}

/// Callers supply all thresholds; none are claimed to be calibrated danger levels.
#[derive(Clone, Copy, Debug)]
pub struct SeverityThresholds {
    pub wind_mps: f64,
    pub dry_humidity_pct: f64,
    pub max_weather_age_seconds: i64,
}

pub fn propose_severity(
    context: &RunContext,
    weather: &[Weather],
    thresholds: SeverityThresholds,
) -> Result<SeverityProposal, ComputeError> {
    if !thresholds.wind_mps.is_finite()
        || !(0.0..=100.0).contains(&thresholds.wind_mps)
        || !thresholds.dry_humidity_pct.is_finite()
        || !(0.0..=100.0).contains(&thresholds.dry_humidity_pct)
        || thresholds.max_weather_age_seconds < 0
    {
        return Err(ComputeError::InvalidInput(
            "invalid demonstration severity thresholds".into(),
        ));
    }
    let now = context.clock.now();
    if weather
        .iter()
        .any(|w| w.run_id != context.run_id || w.provenance.available_at > now)
    {
        return Err(ComputeError::InvalidInput(
            "severity input belongs to another run or future time".into(),
        ));
    }
    let mut reasons = vec!["Heuristic review priority using caller-configured thresholds; not official status or a probability.".into()];
    let mut inputs = Vec::new();
    let mut elevated = false;
    for record in weather {
        let age = now.signed_duration_since(record.valid_at).num_seconds();
        if age < 0 || age > thresholds.max_weather_age_seconds {
            continue;
        }
        let (Some(wind), Some(humidity)) = (record.wind_speed_mps, record.relative_humidity_pct)
        else {
            continue;
        };
        if !wind.is_finite()
            || !(0.0..=100.0).contains(&wind)
            || !humidity.is_finite()
            || !(0.0..=100.0).contains(&humidity)
        {
            return Err(ComputeError::InvalidInput(
                "invalid wind or humidity".into(),
            ));
        }
        inputs.push(record.id.clone());
        if wind >= thresholds.wind_mps && humidity <= thresholds.dry_humidity_pct {
            elevated = true;
            reasons.push(format!("{}: supplied wind {wind:.1} m/s and humidity {humidity:.0}% cross configured review thresholds.",record.id));
        }
    }
    let level = if inputs.is_empty() {
        reasons.push("No fresh complete wind/humidity input; priority is unknown.".into());
        SeverityLevel::Unknown
    } else if elevated {
        SeverityLevel::Elevated
    } else {
        SeverityLevel::Review
    };
    inputs.sort();
    inputs.dedup();
    Ok(SeverityProposal {
        level,
        reasons,
        input_weather_ids: inputs,
        heuristic: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use emberwatch_domain::{DataMode, FixedClock, Provenance};
    use std::sync::Arc;
    #[test]
    fn missing_weather_stays_unknown_and_explicit_thresholds_propose_review_only() {
        let now = "2026-11-14T12:00:00Z".parse().unwrap();
        let ctx = RunContext {
            run_id: "training".into(),
            scenario_id: Some("test".into()),
            mode: DataMode::Synthetic,
            epoch: 0,
            bbox: emberwatch_domain::PORTUGAL_BBOX,
            window_start: now,
            window_end: now,
            clock: Arc::new(FixedClock(now)),
        };
        let config = SeverityThresholds {
            wind_mps: 10.0,
            dry_humidity_pct: 30.0,
            max_weather_age_seconds: 3600,
        };
        assert_eq!(
            propose_severity(&ctx, &[], config).unwrap().level,
            SeverityLevel::Unknown
        );
        let weather = Weather {
            id: "weather-demo".into(),
            run_id: "training".into(),
            station_id: None,
            position: None,
            valid_at: now,
            forecast: false,
            temperature_c: None,
            relative_humidity_pct: Some(20.0),
            wind_speed_mps: Some(12.0),
            wind_from_deg: Some(270.0),
            gust_mps: None,
            fire_risk: None,
            provenance: Provenance {
                source_id: "synthetic".into(),
                source_url: "https://example.org/weather".into(),
                origin_group: "synthetic".into(),
                external_id: None,
                event_at: Some(now),
                published_at: None,
                captured_at: now,
                available_at: now,
                availability_basis: "synthetic".into(),
                evidence_hash: "example".into(),
                quality_flags: vec!["synthetic".into()],
            },
        };
        let result = propose_severity(&ctx, &[weather], config).unwrap();
        assert_eq!(result.level, SeverityLevel::Elevated);
        assert_eq!(result.input_weather_ids, vec!["weather-demo"]);
        assert!(result.heuristic);
    }
}
