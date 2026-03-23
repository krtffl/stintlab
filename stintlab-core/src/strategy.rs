use std::collections::HashMap;

use crate::degradation::compute_stint_time;
use crate::error::StintlabError;
use crate::models::{Compound, DegradationModel, PitWindowPrediction};

/// Default pit stop time loss in milliseconds (22 seconds).
const DEFAULT_PIT_STOP_LOSS_MS: u32 = 22_000;

/// Minimum laps before the end of the race to consider pitting.
const MIN_LAPS_AFTER_PIT: u16 = 5;

/// Dry tire compounds to evaluate as the next stint compound.
const DRY_COMPOUNDS: [Compound; 3] = [Compound::Soft, Compound::Medium, Compound::Hard];

/// Predict the optimal pit window for a driver at a given point in the race.
///
/// Iterates over candidate pit laps and next-stint compounds, evaluating the
/// total remaining race time for each option. Returns the combination that
/// minimizes total time.
///
/// # Errors
///
/// Returns an error if no degradation models are available for any compound,
/// or if the inputs are invalid.
#[allow(clippy::implicit_hasher)]
pub fn predict_pit_window(
    laps_total: u16,
    current_lap: u16,
    current_compound: Compound,
    tire_age: u16,
    models: &HashMap<Compound, DegradationModel>,
    pit_stop_loss_ms: Option<u32>,
) -> Result<PitWindowPrediction, StintlabError> {
    let pit_loss = pit_stop_loss_ms.unwrap_or(DEFAULT_PIT_STOP_LOSS_MS);

    let current_model = models.get(&current_compound).ok_or_else(|| {
        StintlabError::DegradationModelNotFound {
            compound: current_compound,
            circuit_key: String::new(),
        }
    })?;

    if current_lap >= laps_total {
        return Err(StintlabError::InvalidInput(
            "current_lap must be less than laps_total".into(),
        ));
    }

    let earliest_pit = current_lap + 1;
    let latest_pit = laps_total.saturating_sub(MIN_LAPS_AFTER_PIT);

    if earliest_pit > latest_pit {
        return Ok(no_pit_prediction(laps_total, current_compound));
    }

    let mut best_total_time = f64::MAX;
    let mut found_candidate = false;
    let mut best_pit_lap: u16 = current_lap + 1;
    let mut best_next_compound = current_compound;
    let mut best_r_squared_min = 0.0_f64;

    for candidate_pit_lap in earliest_pit..=latest_pit {
        for &next_compound in &DRY_COMPOUNDS {
            let Some(next_model) = models.get(&next_compound) else {
                continue;
            };

            // Time on current tires: current_lap+1 through candidate_pit_lap
            let current_stint_time = compute_stint_time(
                current_model,
                tire_age,
                current_lap + 1,
                candidate_pit_lap,
                laps_total,
            )?;

            // Time on new tires: candidate_pit_lap+1 through laps_total (fresh, age 1)
            let next_stint_time = compute_stint_time(
                next_model,
                1,
                candidate_pit_lap + 1,
                laps_total,
                laps_total,
            )?;

            let total = current_stint_time + f64::from(pit_loss) + next_stint_time;
            let r_sq_min = current_model.r_squared.min(next_model.r_squared);

            if total < best_total_time {
                best_total_time = total;
                best_pit_lap = candidate_pit_lap;
                best_next_compound = next_compound;
                best_r_squared_min = r_sq_min;
                found_candidate = true;
            }
        }
    }

    if !found_candidate {
        return Err(StintlabError::InsufficientData(
            "no valid pit strategy found -- check degradation models".into(),
        ));
    }

    // Compute time without pitting for comparison
    let no_pit_time = compute_stint_time(
        current_model,
        tire_age,
        current_lap + 1,
        laps_total,
        laps_total,
    )?;

    let time_loss = best_total_time - no_pit_time;

    // Confidence derived from R-squared of the models used
    let confidence = best_r_squared_min;

    // Window: optimal +/- adjusted by confidence
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let window_half = ((2.0 * (1.0 - confidence)).ceil() as u16).max(1);
    let window_start = best_pit_lap.saturating_sub(window_half).max(current_lap + 1);
    let window_end = (best_pit_lap + window_half).min(latest_pit);

    Ok(PitWindowPrediction {
        optimal_lap: best_pit_lap,
        window_start,
        window_end,
        predicted_time_loss_ms: time_loss,
        confidence,
        next_compound: best_next_compound,
    })
}

/// When there are not enough laps to pit, return a prediction indicating no stop.
fn no_pit_prediction(laps_total: u16, compound: Compound) -> PitWindowPrediction {
    PitWindowPrediction {
        optimal_lap: laps_total,
        window_start: laps_total,
        window_end: laps_total,
        predicted_time_loss_ms: 0.0,
        confidence: 0.0,
        next_compound: compound,
    }
}

/// Predict the total remaining race time without pitting.
///
/// Useful for comparing with a pit strategy to quantify the benefit.
///
/// # Errors
///
/// Returns `InvalidInput` if lap range is invalid.
pub fn remaining_time_no_pit(
    model: &DegradationModel,
    current_lap: u16,
    tire_age: u16,
    laps_total: u16,
) -> Result<f64, StintlabError> {
    compute_stint_time(model, tire_age, current_lap + 1, laps_total, laps_total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_models() -> HashMap<Compound, DegradationModel> {
        let mut models = HashMap::new();
        models.insert(
            Compound::Medium,
            DegradationModel {
                compound: Compound::Medium,
                circuit_key: "bahrain".into(),
                base_lap_time_ms: 90_000,
                degradation_slope_ms: 80.0,
                fuel_correction_ms: 30.0,
                r_squared: 0.90,
                sample_count: 100,
            },
        );
        models.insert(
            Compound::Hard,
            DegradationModel {
                compound: Compound::Hard,
                circuit_key: "bahrain".into(),
                base_lap_time_ms: 90_500,
                degradation_slope_ms: 40.0,
                fuel_correction_ms: 30.0,
                r_squared: 0.88,
                sample_count: 100,
            },
        );
        models.insert(
            Compound::Soft,
            DegradationModel {
                compound: Compound::Soft,
                circuit_key: "bahrain".into(),
                base_lap_time_ms: 89_500,
                degradation_slope_ms: 120.0,
                fuel_correction_ms: 30.0,
                r_squared: 0.85,
                sample_count: 100,
            },
        );
        models
    }

    #[test]
    fn predict_pit_window_returns_valid_result() {
        let models = make_models();
        let prediction =
            predict_pit_window(57, 10, Compound::Medium, 10, &models, None).unwrap();

        assert!(prediction.optimal_lap > 10);
        assert!(prediction.optimal_lap <= 52);
        assert!(prediction.window_start <= prediction.optimal_lap);
        assert!(prediction.window_end >= prediction.optimal_lap);
        assert!(prediction.confidence > 0.0);
        assert!(prediction.confidence <= 1.0);
    }

    #[test]
    fn predict_pit_window_prefers_lower_deg_for_long_stint() {
        let models = make_models();
        let prediction =
            predict_pit_window(57, 5, Compound::Soft, 5, &models, None).unwrap();

        assert!(
            prediction.next_compound == Compound::Hard
                || prediction.next_compound == Compound::Medium,
            "expected Hard or Medium, got {:?}",
            prediction.next_compound
        );
    }

    #[test]
    fn predict_pit_window_late_race_no_pit() {
        let models = make_models();
        let prediction =
            predict_pit_window(57, 55, Compound::Medium, 20, &models, None).unwrap();

        assert!(prediction.optimal_lap >= 55);
    }

    #[test]
    fn predict_pit_window_missing_current_model() {
        let models = make_models();
        let result =
            predict_pit_window(57, 10, Compound::Intermediate, 10, &models, None);
        assert!(result.is_err());
    }

    #[test]
    fn predict_pit_window_invalid_current_lap() {
        let models = make_models();
        let result = predict_pit_window(57, 57, Compound::Medium, 10, &models, None);
        assert!(result.is_err());
    }

    #[test]
    fn predict_pit_window_custom_pit_loss() {
        let models = make_models();
        let fast_pit =
            predict_pit_window(57, 10, Compound::Medium, 10, &models, Some(18_000)).unwrap();
        let slow_pit =
            predict_pit_window(57, 10, Compound::Medium, 10, &models, Some(30_000)).unwrap();

        assert!(fast_pit.optimal_lap > 10);
        assert!(slow_pit.optimal_lap > 10);
    }

    #[test]
    fn remaining_time_no_pit_basic() {
        let model = DegradationModel {
            compound: Compound::Medium,
            circuit_key: "test".into(),
            base_lap_time_ms: 90_000,
            degradation_slope_ms: 60.0,
            fuel_correction_ms: 30.0,
            r_squared: 0.9,
            sample_count: 100,
        };

        let time = remaining_time_no_pit(&model, 50, 50, 57).unwrap();
        assert!(time > 600_000.0);
        assert!(time < 700_000.0);
    }
}
