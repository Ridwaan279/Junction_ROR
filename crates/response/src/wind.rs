//! Wind vectors point toward travel; meteorological bearings name the origin.
use crate::grid::ComputeError;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindVector {
    pub east_mps: f64,
    pub north_mps: f64,
}

impl WindVector {
    pub fn from_meteorological(speed_mps: f64, from_degrees: f64) -> Result<Self, ComputeError> {
        if !speed_mps.is_finite()
            || !(0.0..=100.0).contains(&speed_mps)
            || !from_degrees.is_finite()
            || !(0.0..360.0).contains(&from_degrees)
        {
            return Err(ComputeError::InvalidInput(
                "invalid meteorological wind".into(),
            ));
        }
        let angle = from_degrees.to_radians();
        Ok(Self {
            east_mps: -speed_mps * angle.sin(),
            north_mps: -speed_mps * angle.cos(),
        })
    }
    pub fn validate(self) -> Result<(), ComputeError> {
        if self.east_mps.is_finite() && self.north_mps.is_finite() && self.speed_mps() <= 100.0 {
            Ok(())
        } else {
            Err(ComputeError::InvalidInput(
                "wind must be finite and <= 100 m/s".into(),
            ))
        }
    }
    pub fn speed_mps(self) -> f64 {
        self.east_mps.hypot(self.north_mps)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WindSample {
    pub wind: WindVector,
    pub age_seconds: u64,
    pub distance_m: f64,
}

pub fn blend_wind(
    samples: &[WindSample],
    max_age_seconds: u64,
) -> Result<Option<WindVector>, ComputeError> {
    let mut weighted = WindVector::default();
    let mut total = 0.0;
    for sample in samples {
        if sample.age_seconds > max_age_seconds {
            continue;
        }
        sample.wind.validate()?;
        if !sample.distance_m.is_finite() || sample.distance_m < 0.0 {
            return Err(ComputeError::InvalidInput(
                "invalid station distance".into(),
            ));
        }
        let weight =
            1.0 / ((1.0 + sample.distance_m / 1000.0) * (1.0 + sample.age_seconds as f64 / 3600.0));
        weighted.east_mps += sample.wind.east_mps * weight;
        weighted.north_mps += sample.wind.north_mps * weight;
        total += weight;
    }
    Ok((total > 0.0).then(|| WindVector {
        east_mps: weighted.east_mps / total,
        north_mps: weighted.north_mps / total,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn west_wind_points_east_and_north_wind_points_south() {
        let west = WindVector::from_meteorological(10.0, 270.0).unwrap();
        assert!((west.east_mps - 10.0).abs() < 1e-9);
        assert!(west.north_mps.abs() < 1e-9);
        let north = WindVector::from_meteorological(5.0, 0.0).unwrap();
        assert!((north.north_mps + 5.0).abs() < 1e-9);
    }
    #[test]
    fn blend_vectors_across_north_instead_of_averaging_bearings() {
        let samples = [
            WindSample {
                wind: WindVector {
                    east_mps: -1.0,
                    north_mps: -9.0,
                },
                age_seconds: 0,
                distance_m: 0.0,
            },
            WindSample {
                wind: WindVector {
                    east_mps: 1.0,
                    north_mps: -9.0,
                },
                age_seconds: 0,
                distance_m: 0.0,
            },
        ];
        assert_eq!(
            blend_wind(&samples, 3600).unwrap(),
            Some(WindVector {
                east_mps: 0.0,
                north_mps: -9.0
            })
        );
    }
    #[test]
    fn missing_or_stale_wind_stays_unknown() {
        assert_eq!(blend_wind(&[], 3600).unwrap(), None);
        let stale = WindSample {
            wind: WindVector::default(),
            age_seconds: 3601,
            distance_m: 0.0,
        };
        assert_eq!(blend_wind(&[stale], 3600).unwrap(), None);
        assert!(WindVector::from_meteorological(-1.0, 270.0).is_err());
    }
}
