use rusqlite::{Connection, params};
use tracing::debug;

use stintlab_core::error::StintlabError;
use stintlab_core::models::{DegradationModel, Lap, Race, Stint};

/// SQL schema for the stintlab database.
const SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS races (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    season      INTEGER NOT NULL,
    round       INTEGER NOT NULL,
    name        TEXT    NOT NULL,
    circuit_key TEXT    NOT NULL,
    date        TEXT    NOT NULL,
    laps_total  INTEGER NOT NULL,
    ingested_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(season, round)
);

CREATE TABLE IF NOT EXISTS laps (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    race_id     INTEGER NOT NULL REFERENCES races(id),
    driver      TEXT    NOT NULL,
    lap_number  INTEGER NOT NULL,
    lap_time_ms INTEGER,
    sector1_ms  INTEGER,
    sector2_ms  INTEGER,
    sector3_ms  INTEGER,
    compound    TEXT    NOT NULL,
    tire_age    INTEGER NOT NULL,
    position    INTEGER NOT NULL,
    pit_in      INTEGER NOT NULL DEFAULT 0,
    pit_out     INTEGER NOT NULL DEFAULT 0,
    UNIQUE(race_id, driver, lap_number)
);

CREATE TABLE IF NOT EXISTS stints (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    race_id      INTEGER NOT NULL REFERENCES races(id),
    driver       TEXT    NOT NULL,
    stint_number INTEGER NOT NULL,
    compound     TEXT    NOT NULL,
    start_lap    INTEGER NOT NULL,
    end_lap      INTEGER NOT NULL,
    UNIQUE(race_id, driver, stint_number)
);

CREATE TABLE IF NOT EXISTS degradation_models (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    circuit_key          TEXT    NOT NULL,
    compound             TEXT    NOT NULL,
    base_lap_time_ms     INTEGER NOT NULL,
    degradation_slope_ms REAL    NOT NULL,
    fuel_correction_ms   REAL    NOT NULL,
    r_squared            REAL    NOT NULL,
    sample_count         INTEGER NOT NULL,
    fitted_at            TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(circuit_key, compound)
);

CREATE INDEX IF NOT EXISTS idx_laps_race_driver ON laps(race_id, driver);
CREATE INDEX IF NOT EXISTS idx_stints_race_driver ON stints(race_id, driver);
CREATE INDEX IF NOT EXISTS idx_races_season ON races(season);
";

/// Initialize the database with the schema and WAL mode.
pub fn init_db(path: &str) -> Result<Connection, StintlabError> {
    let conn = Connection::open(path).map_err(|e| StintlabError::Database(e.to_string()))?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .map_err(|e| StintlabError::Database(e.to_string()))?;
    conn.execute_batch(SCHEMA)
        .map_err(|e| StintlabError::Database(e.to_string()))?;
    debug!(path, "database initialized");
    Ok(conn)
}

/// Insert or replace a race record. Returns the race ID.
pub fn insert_race(conn: &Connection, race: &Race) -> Result<i64, StintlabError> {
    conn.execute(
        "INSERT OR REPLACE INTO races (season, round, name, circuit_key, date, laps_total)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            race.season,
            race.round,
            race.name,
            race.circuit_key,
            race.date.to_string(),
            race.laps_total,
        ],
    )
    .map_err(|e| StintlabError::Database(e.to_string()))?;

    let id = conn.last_insert_rowid();
    debug!(id, name = %race.name, "inserted race");
    Ok(id)
}

/// Insert laps in a batch transaction.
pub fn insert_laps(conn: &Connection, laps: &[Lap]) -> Result<usize, StintlabError> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| StintlabError::Database(e.to_string()))?;

    let mut count = 0;
    for lap in laps {
        tx.execute(
            "INSERT OR IGNORE INTO laps
             (race_id, driver, lap_number, lap_time_ms, sector1_ms, sector2_ms, sector3_ms,
              compound, tire_age, position, pit_in, pit_out)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                lap.race_id,
                lap.driver,
                lap.lap_number,
                lap.lap_time_ms,
                lap.sector1_ms,
                lap.sector2_ms,
                lap.sector3_ms,
                lap.compound.to_string(),
                lap.tire_age,
                lap.position,
                i32::from(lap.pit_in),
                i32::from(lap.pit_out),
            ],
        )
        .map_err(|e| StintlabError::Database(e.to_string()))?;
        count += 1;
    }

    tx.commit()
        .map_err(|e| StintlabError::Database(e.to_string()))?;
    debug!(count, "inserted laps");
    Ok(count)
}

/// Insert stints in a batch transaction.
pub fn insert_stints(conn: &Connection, stints: &[Stint]) -> Result<usize, StintlabError> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| StintlabError::Database(e.to_string()))?;

    let mut count = 0;
    for stint in stints {
        tx.execute(
            "INSERT OR IGNORE INTO stints
             (race_id, driver, stint_number, compound, start_lap, end_lap)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                stint.race_id,
                stint.driver,
                stint.stint_number,
                stint.compound.to_string(),
                stint.start_lap,
                stint.end_lap,
            ],
        )
        .map_err(|e| StintlabError::Database(e.to_string()))?;
        count += 1;
    }

    tx.commit()
        .map_err(|e| StintlabError::Database(e.to_string()))?;
    debug!(count, "inserted stints");
    Ok(count)
}

/// Save or update a degradation model (upsert on `circuit_key` + compound).
pub fn save_degradation_model(
    conn: &Connection,
    model: &DegradationModel,
) -> Result<(), StintlabError> {
    conn.execute(
        "INSERT OR REPLACE INTO degradation_models
         (circuit_key, compound, base_lap_time_ms, degradation_slope_ms,
          fuel_correction_ms, r_squared, sample_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            model.circuit_key,
            model.compound.to_string(),
            model.base_lap_time_ms,
            model.degradation_slope_ms,
            model.fuel_correction_ms,
            model.r_squared,
            model.sample_count,
        ],
    )
    .map_err(|e| StintlabError::Database(e.to_string()))?;

    debug!(
        circuit = %model.circuit_key,
        compound = %model.compound,
        r_squared = model.r_squared,
        "saved degradation model"
    );
    Ok(())
}

/// Load a degradation model for a specific circuit and compound.
#[allow(dead_code)]
pub fn load_degradation_model(
    conn: &Connection,
    circuit_key: &str,
    compound: &str,
) -> Result<Option<DegradationModel>, StintlabError> {
    let mut stmt = conn
        .prepare(
            "SELECT circuit_key, compound, base_lap_time_ms, degradation_slope_ms,
                    fuel_correction_ms, r_squared, sample_count
             FROM degradation_models
             WHERE circuit_key = ?1 AND compound = ?2",
        )
        .map_err(|e| StintlabError::Database(e.to_string()))?;

    let result = stmt
        .query_row(params![circuit_key, compound], |row| {
            let compound_str: String = row.get(1)?;
            Ok(DegradationModel {
                circuit_key: row.get(0)?,
                compound: compound_str
                    .parse()
                    .unwrap_or(stintlab_core::models::Compound::Medium),
                base_lap_time_ms: row.get(2)?,
                degradation_slope_ms: row.get(3)?,
                fuel_correction_ms: row.get(4)?,
                r_squared: row.get(5)?,
                sample_count: row.get(6)?,
            })
        })
        .ok();

    Ok(result)
}

/// List all races, optionally filtered by season.
#[allow(dead_code)]
pub fn list_races(conn: &Connection, season: Option<u16>) -> Result<Vec<Race>, StintlabError> {
    let sql = match season {
        Some(_) => {
            "SELECT id, season, round, name, circuit_key, date, laps_total
             FROM races WHERE season = ?1 ORDER BY date DESC"
        }
        None => {
            "SELECT id, season, round, name, circuit_key, date, laps_total
             FROM races ORDER BY date DESC"
        }
    };

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| StintlabError::Database(e.to_string()))?;

    let rows = if let Some(s) = season {
        stmt.query_map(params![s], row_to_race)
    } else {
        stmt.query_map([], row_to_race)
    }
    .map_err(|e| StintlabError::Database(e.to_string()))?;

    let mut races = Vec::new();
    for row in rows {
        races.push(row.map_err(|e| StintlabError::Database(e.to_string()))?);
    }
    Ok(races)
}

/// Get a single race by ID.
#[allow(dead_code)]
pub fn get_race(conn: &Connection, id: i64) -> Result<Race, StintlabError> {
    conn.query_row(
        "SELECT id, season, round, name, circuit_key, date, laps_total
         FROM races WHERE id = ?1",
        params![id],
        row_to_race,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => StintlabError::RaceNotFound(id),
        other => StintlabError::Database(other.to_string()),
    })
}

/// Get laps for a race, optionally filtered by driver.
#[allow(dead_code)]
pub fn get_laps(
    conn: &Connection,
    race_id: i64,
    driver: Option<&str>,
) -> Result<Vec<Lap>, StintlabError> {
    let sql = match driver {
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

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| StintlabError::Database(e.to_string()))?;

    let rows = if let Some(d) = driver {
        stmt.query_map(params![race_id, d], row_to_lap)
    } else {
        stmt.query_map(params![race_id], row_to_lap)
    }
    .map_err(|e| StintlabError::Database(e.to_string()))?;

    let mut laps = Vec::new();
    for row in rows {
        laps.push(row.map_err(|e| StintlabError::Database(e.to_string()))?);
    }
    Ok(laps)
}

/// Get stints for a race, optionally filtered by driver.
#[allow(dead_code)]
pub fn get_stints(
    conn: &Connection,
    race_id: i64,
    driver: Option<&str>,
) -> Result<Vec<Stint>, StintlabError> {
    let sql = match driver {
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

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| StintlabError::Database(e.to_string()))?;

    let rows = if let Some(d) = driver {
        stmt.query_map(params![race_id, d], row_to_stint)
    } else {
        stmt.query_map(params![race_id], row_to_stint)
    }
    .map_err(|e| StintlabError::Database(e.to_string()))?;

    let mut stints = Vec::new();
    for row in rows {
        stints.push(row.map_err(|e| StintlabError::Database(e.to_string()))?);
    }
    Ok(stints)
}

#[allow(dead_code)]
fn row_to_race(row: &rusqlite::Row<'_>) -> rusqlite::Result<Race> {
    let date_str: String = row.get(5)?;
    let date = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
        .unwrap_or_else(|_| chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap());
    Ok(Race {
        id: Some(row.get(0)?),
        season: row.get(1)?,
        round: row.get(2)?,
        name: row.get(3)?,
        circuit_key: row.get(4)?,
        date,
        laps_total: row.get(6)?,
    })
}

#[allow(dead_code)]
fn row_to_lap(row: &rusqlite::Row<'_>) -> rusqlite::Result<Lap> {
    let compound_str: String = row.get(7)?;
    let compound = compound_str
        .parse()
        .unwrap_or(stintlab_core::models::Compound::Medium);
    let pit_in: i32 = row.get(10)?;
    let pit_out: i32 = row.get(11)?;
    Ok(Lap {
        race_id: row.get(0)?,
        driver: row.get(1)?,
        lap_number: row.get(2)?,
        lap_time_ms: row.get(3)?,
        sector1_ms: row.get(4)?,
        sector2_ms: row.get(5)?,
        sector3_ms: row.get(6)?,
        compound,
        tire_age: row.get(8)?,
        position: row.get(9)?,
        pit_in: pit_in != 0,
        pit_out: pit_out != 0,
    })
}

#[allow(dead_code)]
fn row_to_stint(row: &rusqlite::Row<'_>) -> rusqlite::Result<Stint> {
    let compound_str: String = row.get(3)?;
    let compound = compound_str
        .parse()
        .unwrap_or(stintlab_core::models::Compound::Medium);
    Ok(Stint {
        race_id: row.get(0)?,
        driver: row.get(1)?,
        stint_number: row.get(2)?,
        compound,
        start_lap: row.get(4)?,
        end_lap: row.get(5)?,
    })
}
