use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::error;

use stintlab_core::error::StintlabError;
use stintlab_core::models::{Compound, DegradationModel};
use stintlab_core::strategy;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RacesQuery {
    pub season: Option<u16>,
}

#[derive(Deserialize)]
pub struct LapsQuery {
    pub driver: Option<String>,
}

#[derive(Deserialize)]
pub struct StintsQuery {
    pub driver: Option<String>,
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PitWindowRequest {
    pub race_id: i64,
    #[allow(dead_code)]
    pub driver: String,
    pub current_lap: u16,
    pub current_compound: String,
    pub tire_age: u16,
}

#[derive(Serialize)]
pub struct RaceResponse {
    pub id: i64,
    pub season: u16,
    pub round: u8,
    pub name: String,
    pub circuit_key: String,
    pub date: String,
    pub laps_total: u16,
}

#[derive(Serialize)]
pub struct LapsResponse {
    pub race_id: i64,
    pub drivers: HashMap<String, Vec<LapResponse>>,
}

#[derive(Serialize)]
pub struct LapResponse {
    pub lap_number: u16,
    pub lap_time_ms: Option<u32>,
    pub sector1_ms: Option<u32>,
    pub sector2_ms: Option<u32>,
    pub sector3_ms: Option<u32>,
    pub compound: String,
    pub tire_age: u16,
    pub position: u8,
    pub pit_in: bool,
    pub pit_out: bool,
}

#[derive(Serialize)]
pub struct StintsResponse {
    pub race_id: i64,
    pub drivers: HashMap<String, Vec<StintResponse>>,
}

#[derive(Serialize)]
pub struct StintResponse {
    pub stint_number: u8,
    pub compound: String,
    pub start_lap: u16,
    pub end_lap: u16,
    pub lap_count: u16,
}

#[derive(Serialize)]
pub struct PitWindowResponse {
    pub optimal_lap: u16,
    pub window_start: u16,
    pub window_end: u16,
    pub predicted_time_loss_s: f64,
    pub confidence: f64,
    pub next_compound: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: String,
    message: String,
}

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

fn map_error(err: StintlabError) -> (StatusCode, Json<ErrorResponse>) {
    let (status, code) = match &err {
        StintlabError::RaceNotFound(_) => (StatusCode::NOT_FOUND, "RACE_NOT_FOUND"),
        StintlabError::DegradationModelNotFound { .. } => {
            (StatusCode::UNPROCESSABLE_ENTITY, "MODEL_NOT_FOUND")
        }
        StintlabError::InsufficientData(_) => {
            (StatusCode::UNPROCESSABLE_ENTITY, "INSUFFICIENT_DATA")
        }
        StintlabError::InvalidInput(_) => (StatusCode::BAD_REQUEST, "INVALID_INPUT"),
        _ => {
            error!(error = %err, "internal server error");
            (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR")
        }
    };

    (
        status,
        Json(ErrorResponse {
            error: ErrorDetail {
                code: code.to_owned(),
                message: err.to_string(),
            },
        }),
    )
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/races
pub async fn list_races(
    State(state): State<AppState>,
    Query(params): Query<RacesQuery>,
) -> impl IntoResponse {
    let db = state.db.lock().expect("db lock poisoned");

    // Inline storage query to avoid coupling to stintlab-ingest
    let sql = match params.season {
        Some(_) => {
            "SELECT id, season, round, name, circuit_key, date, laps_total
             FROM races WHERE season = ?1 ORDER BY date DESC"
        }
        None => {
            "SELECT id, season, round, name, circuit_key, date, laps_total
             FROM races ORDER BY date DESC"
        }
    };

    let result = if let Some(s) = params.season {
        query_races(&db, sql, rusqlite::params![s])
    } else {
        query_races(&db, sql, rusqlite::params![])
    };

    match result {
        Ok(races) => (StatusCode::OK, Json(serde_json::json!(races))).into_response(),
        Err(e) => map_error(e).into_response(),
    }
}

/// GET /api/races/:id/laps
pub async fn get_laps(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<LapsQuery>,
) -> impl IntoResponse {
    let db = state.db.lock().expect("db lock poisoned");

    // Verify race exists
    let race_exists: bool = db
        .query_row(
            "SELECT COUNT(*) FROM races WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !race_exists {
        return map_error(StintlabError::RaceNotFound(id)).into_response();
    }

    let sql = match params.driver {
        Some(_) => {
            "SELECT race_id, driver, lap_number, lap_time_ms, sector1_ms, sector2_ms,
                    sector3_ms, compound, tire_age, position, pit_in, pit_out
             FROM laps WHERE race_id = ?1 AND driver = ?2
             ORDER BY driver, lap_number"
        }
        None => {
            "SELECT race_id, driver, lap_number, lap_time_ms, sector1_ms, sector2_ms,
                    sector3_ms, compound, tire_age, position, pit_in, pit_out
             FROM laps WHERE race_id = ?1
             ORDER BY driver, lap_number"
        }
    };

    let result = if let Some(ref d) = params.driver {
        query_laps(&db, sql, rusqlite::params![id, d])
    } else {
        query_laps(&db, sql, rusqlite::params![id])
    };

    match result {
        Ok(laps) => {
            let mut drivers: HashMap<String, Vec<LapResponse>> = HashMap::new();
            for lap in laps {
                drivers.entry(lap.driver.clone()).or_default().push(LapResponse {
                    lap_number: lap.lap_number,
                    lap_time_ms: lap.lap_time_ms,
                    sector1_ms: lap.sector1_ms,
                    sector2_ms: lap.sector2_ms,
                    sector3_ms: lap.sector3_ms,
                    compound: lap.compound.to_string(),
                    tire_age: lap.tire_age,
                    position: lap.position,
                    pit_in: lap.pit_in,
                    pit_out: lap.pit_out,
                });
            }
            let response = LapsResponse {
                race_id: id,
                drivers,
            };
            (StatusCode::OK, Json(serde_json::json!(response))).into_response()
        }
        Err(e) => map_error(e).into_response(),
    }
}

/// GET /api/races/:id/stints
pub async fn get_stints(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<StintsQuery>,
) -> impl IntoResponse {
    let db = state.db.lock().expect("db lock poisoned");

    let race_exists: bool = db
        .query_row(
            "SELECT COUNT(*) FROM races WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !race_exists {
        return map_error(StintlabError::RaceNotFound(id)).into_response();
    }

    let sql = match params.driver {
        Some(_) => {
            "SELECT race_id, driver, stint_number, compound, start_lap, end_lap
             FROM stints WHERE race_id = ?1 AND driver = ?2
             ORDER BY driver, stint_number"
        }
        None => {
            "SELECT race_id, driver, stint_number, compound, start_lap, end_lap
             FROM stints WHERE race_id = ?1
             ORDER BY driver, stint_number"
        }
    };

    let result = if let Some(ref d) = params.driver {
        query_stints(&db, sql, rusqlite::params![id, d])
    } else {
        query_stints(&db, sql, rusqlite::params![id])
    };

    match result {
        Ok(stints) => {
            let mut drivers: HashMap<String, Vec<StintResponse>> = HashMap::new();
            for stint in stints {
                let lap_count = stint.end_lap.saturating_sub(stint.start_lap) + 1;
                drivers
                    .entry(stint.driver.clone())
                    .or_default()
                    .push(StintResponse {
                        stint_number: stint.stint_number,
                        compound: stint.compound.to_string(),
                        start_lap: stint.start_lap,
                        end_lap: stint.end_lap,
                        lap_count,
                    });
            }
            let response = StintsResponse {
                race_id: id,
                drivers,
            };
            (StatusCode::OK, Json(serde_json::json!(response))).into_response()
        }
        Err(e) => map_error(e).into_response(),
    }
}

/// POST /api/predict/pit-window
pub async fn predict_pit_window(
    State(state): State<AppState>,
    Json(req): Json<PitWindowRequest>,
) -> impl IntoResponse {
    let db = state.db.lock().expect("db lock poisoned");

    // Look up the race to get circuit_key and laps_total
    let race = db
        .query_row(
            "SELECT id, season, round, name, circuit_key, date, laps_total
             FROM races WHERE id = ?1",
            rusqlite::params![req.race_id],
            |row| {
                Ok((
                    row.get::<_, String>(4)?,  // circuit_key
                    row.get::<_, u16>(6)?,     // laps_total
                ))
            },
        );

    let (circuit_key, laps_total) = match race {
        Ok(r) => r,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return map_error(StintlabError::RaceNotFound(req.race_id)).into_response();
        }
        Err(e) => {
            return map_error(StintlabError::Database(e.to_string())).into_response();
        }
    };

    let current_compound: Compound = match req.current_compound.parse() {
        Ok(c) => c,
        Err(e) => return map_error(e).into_response(),
    };

    // Load degradation models for this circuit
    let mut models = HashMap::new();
    for compound_name in &["Soft", "Medium", "Hard"] {
        let model = load_model(&db, &circuit_key, compound_name);
        if let Some(m) = model {
            models.insert(m.compound, m);
        }
    }

    if models.is_empty() {
        return map_error(StintlabError::InsufficientData(
            "no degradation models available for this circuit".into(),
        ))
        .into_response();
    }

    match strategy::predict_pit_window(
        laps_total,
        req.current_lap,
        current_compound,
        req.tire_age,
        &models,
        None,
    ) {
        Ok(prediction) => {
            let response = PitWindowResponse {
                optimal_lap: prediction.optimal_lap,
                window_start: prediction.window_start,
                window_end: prediction.window_end,
                predicted_time_loss_s: prediction.predicted_time_loss_ms / 1000.0,
                confidence: prediction.confidence,
                next_compound: prediction.next_compound.to_string(),
            };
            (StatusCode::OK, Json(serde_json::json!(response))).into_response()
        }
        Err(e) => map_error(e).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn load_model(
    db: &rusqlite::Connection,
    circuit_key: &str,
    compound: &str,
) -> Option<DegradationModel> {
    db.query_row(
        "SELECT circuit_key, compound, base_lap_time_ms, degradation_slope_ms,
                fuel_correction_ms, r_squared, sample_count
         FROM degradation_models
         WHERE circuit_key = ?1 AND compound = ?2",
        rusqlite::params![circuit_key, compound],
        |row| {
            let compound_str: String = row.get(1)?;
            Ok(DegradationModel {
                circuit_key: row.get(0)?,
                compound: compound_str.parse().unwrap_or(Compound::Medium),
                base_lap_time_ms: row.get(2)?,
                degradation_slope_ms: row.get(3)?,
                fuel_correction_ms: row.get(4)?,
                r_squared: row.get(5)?,
                sample_count: row.get(6)?,
            })
        },
    )
    .ok()
}

/// Lightweight lap struct for internal query results (avoids importing stintlab-ingest).
struct LapRow {
    driver: String,
    lap_number: u16,
    lap_time_ms: Option<u32>,
    sector1_ms: Option<u32>,
    sector2_ms: Option<u32>,
    sector3_ms: Option<u32>,
    compound: Compound,
    tire_age: u16,
    position: u8,
    pit_in: bool,
    pit_out: bool,
}

struct StintRow {
    driver: String,
    stint_number: u8,
    compound: Compound,
    start_lap: u16,
    end_lap: u16,
}

fn query_races(
    db: &rusqlite::Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<RaceResponse>, StintlabError> {
    let mut stmt = db
        .prepare(sql)
        .map_err(|e| StintlabError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params, |row| {
            Ok(RaceResponse {
                id: row.get(0)?,
                season: row.get(1)?,
                round: row.get(2)?,
                name: row.get(3)?,
                circuit_key: row.get(4)?,
                date: row.get(5)?,
                laps_total: row.get(6)?,
            })
        })
        .map_err(|e| StintlabError::Database(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| StintlabError::Database(e.to_string()))?);
    }
    Ok(result)
}

fn query_laps(
    db: &rusqlite::Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<LapRow>, StintlabError> {
    let mut stmt = db
        .prepare(sql)
        .map_err(|e| StintlabError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params, |row| {
            let compound_str: String = row.get(7)?;
            let pit_in: i32 = row.get(10)?;
            let pit_out: i32 = row.get(11)?;
            Ok(LapRow {
                driver: row.get(1)?,
                lap_number: row.get(2)?,
                lap_time_ms: row.get(3)?,
                sector1_ms: row.get(4)?,
                sector2_ms: row.get(5)?,
                sector3_ms: row.get(6)?,
                compound: compound_str.parse().unwrap_or(Compound::Medium),
                tire_age: row.get(8)?,
                position: row.get(9)?,
                pit_in: pit_in != 0,
                pit_out: pit_out != 0,
            })
        })
        .map_err(|e| StintlabError::Database(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| StintlabError::Database(e.to_string()))?);
    }
    Ok(result)
}

fn query_stints(
    db: &rusqlite::Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<StintRow>, StintlabError> {
    let mut stmt = db
        .prepare(sql)
        .map_err(|e| StintlabError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(params, |row| {
            let compound_str: String = row.get(3)?;
            Ok(StintRow {
                driver: row.get(1)?,
                stint_number: row.get(2)?,
                compound: compound_str.parse().unwrap_or(Compound::Medium),
                start_lap: row.get(4)?,
                end_lap: row.get(5)?,
            })
        })
        .map_err(|e| StintlabError::Database(e.to_string()))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| StintlabError::Database(e.to_string()))?);
    }
    Ok(result)
}
