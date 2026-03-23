use std::fmt;
use std::str::FromStr;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::error::StintlabError;

/// A Formula 1 race (Grand Prix).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Race {
    pub id: Option<i64>,
    pub season: u16,
    pub round: u8,
    pub name: String,
    pub circuit_key: String,
    pub date: NaiveDate,
    pub laps_total: u16,
}

/// A single lap recorded for a driver in a race.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lap {
    pub race_id: i64,
    pub driver: String,
    pub lap_number: u16,
    pub lap_time_ms: Option<u32>,
    pub sector1_ms: Option<u32>,
    pub sector2_ms: Option<u32>,
    pub sector3_ms: Option<u32>,
    pub compound: Compound,
    pub tire_age: u16,
    pub position: u8,
    pub pit_in: bool,
    pub pit_out: bool,
}

/// Tire compound type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Compound {
    Soft,
    Medium,
    Hard,
    Intermediate,
    Wet,
}

impl fmt::Display for Compound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Soft => write!(f, "Soft"),
            Self::Medium => write!(f, "Medium"),
            Self::Hard => write!(f, "Hard"),
            Self::Intermediate => write!(f, "Intermediate"),
            Self::Wet => write!(f, "Wet"),
        }
    }
}

impl FromStr for Compound {
    type Err = StintlabError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "soft" | "s" => Ok(Self::Soft),
            "medium" | "m" => Ok(Self::Medium),
            "hard" | "h" => Ok(Self::Hard),
            "intermediate" | "inter" | "i" => Ok(Self::Intermediate),
            "wet" | "w" => Ok(Self::Wet),
            other => Err(StintlabError::InvalidInput(format!(
                "unknown compound: {other}"
            ))),
        }
    }
}

/// A stint (consecutive laps on the same set of tires).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stint {
    pub race_id: i64,
    pub driver: String,
    pub stint_number: u8,
    pub compound: Compound,
    pub start_lap: u16,
    pub end_lap: u16,
}

/// Statistical degradation model fitted per (compound, circuit) pair.
///
/// Model: `lap_time = base + slope * tire_age - fuel_correction * (laps_total - lap_number)`
///
/// Computed in Rust at ingest time via OLS linear regression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradationModel {
    pub compound: Compound,
    pub circuit_key: String,
    pub base_lap_time_ms: u32,
    pub degradation_slope_ms: f64,
    pub fuel_correction_ms: f64,
    pub r_squared: f64,
    pub sample_count: u32,
}

impl DegradationModel {
    /// Validate that fitted parameters are plausible.
    ///
    /// # Errors
    ///
    /// Returns `InvalidInput` if `base_lap_time_ms` is zero or `r_squared` is outside `[0, 1]`.
    pub fn validate(&self) -> Result<(), StintlabError> {
        if self.base_lap_time_ms == 0 {
            return Err(StintlabError::InvalidInput(
                "base_lap_time_ms must be > 0".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.r_squared) {
            return Err(StintlabError::InvalidInput(
                "r_squared must be in [0.0, 1.0]".into(),
            ));
        }
        Ok(())
    }
}

/// Prediction result from the pit window calculator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PitWindowPrediction {
    pub optimal_lap: u16,
    pub window_start: u16,
    pub window_end: u16,
    pub predicted_time_loss_ms: f64,
    pub confidence: f64,
    pub next_compound: Compound,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compound_display_roundtrip() {
        let compounds = [
            Compound::Soft,
            Compound::Medium,
            Compound::Hard,
            Compound::Intermediate,
            Compound::Wet,
        ];
        for c in compounds {
            let s = c.to_string();
            let parsed: Compound = s.parse().expect("roundtrip should succeed");
            assert_eq!(c, parsed);
        }
    }

    #[test]
    fn compound_from_str_abbreviations() {
        assert_eq!("s".parse::<Compound>().unwrap(), Compound::Soft);
        assert_eq!("M".parse::<Compound>().unwrap(), Compound::Medium);
        assert_eq!("H".parse::<Compound>().unwrap(), Compound::Hard);
        assert_eq!("inter".parse::<Compound>().unwrap(), Compound::Intermediate);
        assert_eq!("W".parse::<Compound>().unwrap(), Compound::Wet);
    }

    #[test]
    fn compound_from_str_invalid() {
        assert!("supersoft".parse::<Compound>().is_err());
    }

    #[test]
    fn degradation_model_validate_ok() {
        let model = DegradationModel {
            compound: Compound::Medium,
            circuit_key: "bahrain".into(),
            base_lap_time_ms: 93_000,
            degradation_slope_ms: 50.0,
            fuel_correction_ms: 30.0,
            r_squared: 0.85,
            sample_count: 100,
        };
        assert!(model.validate().is_ok());
    }

    #[test]
    fn degradation_model_validate_bad_base() {
        let model = DegradationModel {
            compound: Compound::Soft,
            circuit_key: "test".into(),
            base_lap_time_ms: 0,
            degradation_slope_ms: 50.0,
            fuel_correction_ms: 30.0,
            r_squared: 0.85,
            sample_count: 100,
        };
        assert!(model.validate().is_err());
    }

    #[test]
    fn degradation_model_validate_bad_r_squared() {
        let model = DegradationModel {
            compound: Compound::Hard,
            circuit_key: "test".into(),
            base_lap_time_ms: 90_000,
            degradation_slope_ms: 50.0,
            fuel_correction_ms: 30.0,
            r_squared: 1.5,
            sample_count: 100,
        };
        assert!(model.validate().is_err());
    }
}
