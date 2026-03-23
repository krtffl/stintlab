use crate::error::StintlabError;
use crate::models::{Compound, DegradationModel, Lap};

/// Minimum number of valid laps required to fit a degradation model.
const MIN_SAMPLES: usize = 30;

/// Fit a degradation model from lap data using OLS linear regression.
///
/// Model: `lap_time_ms = base + slope * tire_age - fuel_correction * (laps_total - lap_number)`
///
/// Returns `None` if fewer than [`MIN_SAMPLES`] valid laps are available after filtering.
/// Excludes pit in/out laps and laps without a recorded time.
#[must_use]
#[allow(clippy::cast_precision_loss)] // usize → f64 safe for sample counts
pub fn fit_model(
    circuit_key: &str,
    compound: Compound,
    laps: &[Lap],
    laps_total: u16,
) -> Option<DegradationModel> {
    // Filter to valid laps: correct compound, has lap time, not pit in/out
    let valid: Vec<&Lap> = laps
        .iter()
        .filter(|l| {
            l.compound == compound && l.lap_time_ms.is_some() && !l.pit_in && !l.pit_out
        })
        .collect();

    if valid.len() < MIN_SAMPLES {
        return None;
    }

    // Collect raw times for outlier detection
    let times: Vec<f64> = valid
        .iter()
        .map(|l| f64::from(l.lap_time_ms.unwrap_or(0)))
        .collect();

    let n_f = times.len() as f64;
    let mean = times.iter().sum::<f64>() / n_f;
    let variance = times.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / n_f;
    let std_dev = variance.sqrt();

    // Exclude outliers (>3 standard deviations from mean)
    let filtered: Vec<&Lap> = valid
        .into_iter()
        .filter(|l| {
            let t = f64::from(l.lap_time_ms.unwrap_or(0));
            (t - mean).abs() <= 3.0 * std_dev
        })
        .collect();

    if filtered.len() < MIN_SAMPLES {
        return None;
    }

    // Build OLS matrices: y = X * beta
    // X columns: [1 (intercept), tire_age, fuel_remaining]
    // where fuel_remaining = laps_total - lap_number
    let n = filtered.len();
    let mut y = vec![0.0_f64; n];
    let mut x = vec![[0.0_f64; 3]; n];

    for (i, lap) in filtered.iter().enumerate() {
        y[i] = f64::from(lap.lap_time_ms.unwrap_or(0));
        x[i][0] = 1.0; // intercept
        x[i][1] = f64::from(lap.tire_age); // tire age
        x[i][2] = f64::from(laps_total.saturating_sub(lap.lap_number)); // fuel remaining
    }

    // Solve normal equations: (X^T X) beta = X^T y
    let beta = solve_normal_equations_3x3(&x, &y)?;

    let base_lap_time_ms = beta[0];
    let degradation_slope_ms = beta[1];
    // fuel_correction is negated: model is base + slope*age - fuel_corr*remaining
    // OLS fits: y = b0 + b1*age + b2*remaining, so fuel_correction = -b2
    let fuel_correction_ms = -beta[2];

    // Compute R-squared
    let y_mean = y.iter().sum::<f64>() / n as f64;
    let ss_tot: f64 = y.iter().map(|yi| (yi - y_mean).powi(2)).sum();
    let ss_res: f64 = y
        .iter()
        .enumerate()
        .map(|(i, yi)| {
            let y_hat = beta[0] + beta[1] * x[i][1] + beta[2] * x[i][2];
            (yi - y_hat).powi(2)
        })
        .sum();

    let r_squared = if ss_tot > 0.0 {
        1.0 - ss_res / ss_tot
    } else {
        0.0
    };

    // Clamp base to non-negative and convert to u32
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let base_u32 = base_lap_time_ms.round().max(0.0) as u32;

    #[allow(clippy::cast_possible_truncation)]
    let sample_count = n as u32;

    Some(DegradationModel {
        compound,
        circuit_key: circuit_key.to_owned(),
        base_lap_time_ms: base_u32,
        degradation_slope_ms,
        fuel_correction_ms,
        r_squared: r_squared.clamp(0.0, 1.0),
        sample_count,
    })
}

/// Predict lap time given model parameters, tire age, and race position.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn predict_lap_time(
    model: &DegradationModel,
    tire_age: u16,
    lap_number: u16,
    laps_total: u16,
) -> u32 {
    let deg = model.degradation_slope_ms * f64::from(tire_age);
    let fuel = model.fuel_correction_ms * f64::from(laps_total.saturating_sub(lap_number));
    let predicted = f64::from(model.base_lap_time_ms) + deg - fuel;
    predicted.round().max(0.0) as u32
}

/// Solve the 3x3 normal equations system `(X^T X) beta = X^T y` using Cramer's rule.
///
/// Returns `None` if the system is singular (determinant ~ 0).
fn solve_normal_equations_3x3(x: &[[f64; 3]], y: &[f64]) -> Option<[f64; 3]> {
    // Compute X^T X (3x3 symmetric matrix)
    let mut xtx = [[0.0_f64; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            xtx[i][j] = x.iter().map(|row| row[i] * row[j]).sum();
        }
    }

    // Compute X^T y (3x1 vector)
    let mut xty = [0.0_f64; 3];
    for i in 0..3 {
        xty[i] = x.iter().zip(y.iter()).map(|(row, &yi)| row[i] * yi).sum();
    }

    // Solve via Cramer's rule for 3x3
    let det = det3x3(&xtx);
    if det.abs() < 1e-12 {
        return None;
    }

    let mut result = [0.0_f64; 3];
    for col in 0..3 {
        let mut m = xtx;
        for (row_idx, row) in m.iter_mut().enumerate() {
            row[col] = xty[row_idx];
        }
        result[col] = det3x3(&m) / det;
    }

    Some(result)
}

/// Compute the determinant of a 3x3 matrix.
fn det3x3(m: &[[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// Compute the total race time for a stint sequence using degradation models.
///
/// Used internally by the strategy engine to evaluate candidate pit strategies.
///
/// # Errors
///
/// Returns `InvalidInput` if `start_lap > end_lap`.
pub fn compute_stint_time(
    model: &DegradationModel,
    start_tire_age: u16,
    start_lap: u16,
    end_lap: u16,
    laps_total: u16,
) -> Result<f64, StintlabError> {
    if start_lap > end_lap {
        return Err(StintlabError::InvalidInput(
            "start_lap must be <= end_lap".into(),
        ));
    }

    let mut total = 0.0_f64;
    for lap in start_lap..=end_lap {
        let tire_age = start_tire_age + (lap - start_lap);
        let time = predict_lap_time(model, tire_age, lap, laps_total);
        total += f64::from(time);
    }
    Ok(total)
}

#[cfg(test)]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
mod tests {
    use super::*;

    /// Generate synthetic lap data with known degradation characteristics.
    ///
    /// Simulates multiple drivers with different stint start laps to break
    /// the collinearity between `tire_age` and `laps_total - lap_number`.
    /// Without this, OLS cannot separate the two effects (perfect linear dependence).
    fn make_test_laps(
        n: usize,
        base_ms: u32,
        slope_per_lap: f64,
        fuel_effect: f64,
        laps_total: u16,
        compound: Compound,
    ) -> Vec<Lap> {
        // Generate data from 3 "drivers" with different stint starts
        // to break the collinearity between tire_age and fuel_remaining.
        let stint_configs: &[(u16, &str)] = &[
            (1, "DR1"),   // stint starting lap 1
            (15, "DR2"),  // stint starting lap 15
            (25, "DR3"),  // stint starting lap 25
        ];

        let laps_per_driver = n / stint_configs.len();
        let mut all_laps = Vec::with_capacity(n);

        for &(stint_start_lap, driver) in stint_configs {
            for i in 0..laps_per_driver {
                let tire_age = (i as u16) + 1;
                let lap_num = stint_start_lap + i as u16;
                if lap_num > laps_total {
                    break;
                }
                let fuel_remaining = f64::from(laps_total - lap_num);
                let time = f64::from(base_ms)
                    + slope_per_lap * f64::from(tire_age)
                    - fuel_effect * fuel_remaining;
                all_laps.push(Lap {
                    race_id: 1,
                    driver: driver.into(),
                    lap_number: lap_num,
                    lap_time_ms: Some(time.round() as u32),
                    sector1_ms: None,
                    sector2_ms: None,
                    sector3_ms: None,
                    compound,
                    tire_age,
                    position: 1,
                    pit_in: false,
                    pit_out: false,
                });
            }
        }

        all_laps
    }

    #[test]
    fn fit_model_recovers_known_parameters() {
        // 60 laps across 3 drivers (20 each) with varying stint starts
        let laps = make_test_laps(60, 90_000, 60.0, 30.0, 57, Compound::Medium);
        let model = fit_model("test_circuit", Compound::Medium, &laps, 57)
            .expect("should fit with sufficient multi-driver data");

        let base_diff = (f64::from(model.base_lap_time_ms) - 90_000.0).abs();
        assert!(
            base_diff < 500.0,
            "base {}, expected ~90000, diff {base_diff}",
            model.base_lap_time_ms
        );

        assert!(
            (model.degradation_slope_ms - 60.0).abs() < 5.0,
            "slope {}, expected ~60",
            model.degradation_slope_ms
        );

        assert!(
            (model.fuel_correction_ms - 30.0).abs() < 5.0,
            "fuel_correction {}, expected ~30",
            model.fuel_correction_ms
        );

        assert!(
            model.r_squared > 0.95,
            "r_squared {} should be > 0.95",
            model.r_squared
        );
    }

    #[test]
    fn fit_model_returns_none_insufficient_data() {
        // 9 laps across 3 drivers = 3 each, well under MIN_SAMPLES=30
        let laps = make_test_laps(9, 90_000, 60.0, 30.0, 57, Compound::Soft);
        assert!(fit_model("test", Compound::Soft, &laps, 57).is_none());
    }

    #[test]
    fn fit_model_excludes_pit_laps() {
        let mut laps = make_test_laps(36, 90_000, 60.0, 30.0, 57, Compound::Hard);
        let last = laps.len() - 1;
        laps[0].pit_out = true;
        laps[last].pit_in = true;
        // After excluding 2 pit laps, should still have >= 30 valid laps
        let model = fit_model("test", Compound::Hard, &laps, 57);
        assert!(
            model.is_some(),
            "expected model to fit with {} laps ({} after pit exclusion)",
            laps.len(),
            laps.len() - 2
        );
    }

    #[test]
    fn fit_model_filters_wrong_compound() {
        let mut laps = make_test_laps(60, 90_000, 60.0, 30.0, 57, Compound::Medium);
        // Change most laps to Soft, leaving fewer than 30 Medium laps
        let cutoff = laps.len() - 10;
        for lap in &mut laps[..cutoff] {
            lap.compound = Compound::Soft;
        }
        assert!(fit_model("test", Compound::Medium, &laps, 57).is_none());
    }

    #[test]
    fn predict_lap_time_basic() {
        let model = DegradationModel {
            compound: Compound::Medium,
            circuit_key: "bahrain".into(),
            base_lap_time_ms: 90_000,
            degradation_slope_ms: 60.0,
            fuel_correction_ms: 30.0,
            r_squared: 0.9,
            sample_count: 100,
        };

        let predicted = predict_lap_time(&model, 1, 1, 57);
        assert_eq!(predicted, 88380);

        let predicted = predict_lap_time(&model, 57, 57, 57);
        assert_eq!(predicted, 93420);
    }

    #[test]
    fn predict_lap_time_no_negative() {
        let model = DegradationModel {
            compound: Compound::Soft,
            circuit_key: "test".into(),
            base_lap_time_ms: 100,
            degradation_slope_ms: 0.0,
            fuel_correction_ms: 1000.0,
            r_squared: 0.5,
            sample_count: 50,
        };

        let predicted = predict_lap_time(&model, 1, 1, 51);
        assert_eq!(predicted, 0);
    }

    #[test]
    fn compute_stint_time_single_lap() {
        let model = DegradationModel {
            compound: Compound::Hard,
            circuit_key: "test".into(),
            base_lap_time_ms: 90_000,
            degradation_slope_ms: 50.0,
            fuel_correction_ms: 25.0,
            r_squared: 0.85,
            sample_count: 80,
        };

        let time = compute_stint_time(&model, 1, 10, 10, 57).unwrap();
        let expected = f64::from(predict_lap_time(&model, 1, 10, 57));
        assert!((time - expected).abs() < 1.0);
    }

    #[test]
    fn compute_stint_time_invalid_range() {
        let model = DegradationModel {
            compound: Compound::Medium,
            circuit_key: "test".into(),
            base_lap_time_ms: 90_000,
            degradation_slope_ms: 50.0,
            fuel_correction_ms: 25.0,
            r_squared: 0.85,
            sample_count: 80,
        };

        assert!(compute_stint_time(&model, 1, 20, 10, 57).is_err());
    }

    #[test]
    fn det3x3_identity() {
        let m = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        assert!((det3x3(&m) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn det3x3_known() {
        let m = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 0.0]];
        assert!((det3x3(&m) - 27.0).abs() < 1e-10);
    }
}
