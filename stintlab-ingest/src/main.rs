mod normalize;
mod openf1;
mod storage;

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use tracing::{info, warn};

use stintlab_core::degradation;
use stintlab_core::models::Compound;

/// stintlab data ingestion pipeline.
///
/// Fetches historical F1 race data from the `OpenF1` API and stores it
/// locally in `SQLite`. Fits degradation models for each (circuit, compound) pair.
#[derive(Parser)]
#[command(name = "stintlab-ingest", version, about)]
struct Cli {
    /// Season year to ingest (e.g. 2024).
    #[arg(long)]
    season: u16,

    /// Specific round number to ingest. If omitted, ingests all rounds.
    #[arg(long)]
    round: Option<u8>,

    /// Path to the `SQLite` database file.
    #[arg(long, default_value = "data/races.db")]
    db: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "stintlab_ingest=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    // Ensure data directory exists
    if let Some(parent) = cli.db.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    let db_path = cli.db.to_str().context("invalid database path")?;
    let conn = storage::init_db(db_path).context("failed to initialize database")?;

    let client = openf1::OpenF1Client::new().context("failed to create OpenF1 client")?;

    info!(season = cli.season, round = ?cli.round, "starting ingestion");

    // Fetch sessions for the season
    let sessions = client
        .get_sessions(cli.season, None)
        .await
        .context("failed to fetch sessions")?;

    info!(count = sessions.len(), "found sessions");

    for (idx, session) in sessions.iter().enumerate() {
        let round = u8::try_from(idx + 1).unwrap_or(u8::MAX);

        // Skip if a specific round was requested and this isn't it
        if let Some(target_round) = cli.round
            && round != target_round
        {
            continue;
        }

        info!(
            round,
            session_key = session.session_key,
            name = session.country_name.as_deref().unwrap_or("?"),
            "processing session"
        );

        // Convert to domain Race
        let Some(mut race) = normalize::session_to_race(session, round) else {
            warn!(
                session_key = session.session_key,
                "skipping: incomplete session data"
            );
            continue;
        };

        // Fetch drivers, laps, and stints
        let drivers = client
            .get_drivers(session.session_key)
            .await
            .context("failed to fetch drivers")?;
        let raw_laps = client
            .get_laps(session.session_key)
            .await
            .context("failed to fetch laps")?;
        let raw_stints = client
            .get_stints(session.session_key)
            .await
            .context("failed to fetch stints")?;

        let driver_map = normalize::build_driver_map(&drivers);
        let laps_total = normalize::derive_laps_total(&raw_laps);
        race.laps_total = laps_total;

        // Insert race
        let race_id = storage::insert_race(&conn, &race).context("failed to insert race")?;

        // Normalize and insert laps
        let stint_compounds = normalize::build_stint_compound_map(&raw_stints);
        let mut laps = normalize::normalize_laps(&raw_laps, race_id, &driver_map, &stint_compounds);

        // Normalize and insert stints
        let stints = normalize::normalize_stints(&raw_stints, race_id, &driver_map);
        normalize::mark_pit_in_laps(&mut laps, &stints);

        let lap_count = storage::insert_laps(&conn, &laps).context("failed to insert laps")?;
        let stint_count =
            storage::insert_stints(&conn, &stints).context("failed to insert stints")?;

        info!(lap_count, stint_count, "inserted data");

        // Fit degradation models for each compound at this circuit
        let compounds_seen: HashSet<Compound> = laps.iter().map(|l| l.compound).collect();

        for compound in compounds_seen {
            match degradation::fit_model(&race.circuit_key, compound, &laps, laps_total) {
                Some(model) => {
                    info!(
                        circuit = %race.circuit_key,
                        compound = %compound,
                        r_squared = model.r_squared,
                        slope = model.degradation_slope_ms,
                        samples = model.sample_count,
                        "fitted degradation model"
                    );
                    storage::save_degradation_model(&conn, &model)
                        .context("failed to save degradation model")?;
                }
                None => {
                    warn!(
                        circuit = %race.circuit_key,
                        compound = %compound,
                        "insufficient data to fit model (need >= 30 laps)"
                    );
                }
            }
        }
    }

    info!("ingestion complete");
    Ok(())
}
