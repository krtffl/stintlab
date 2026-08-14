mod api;
mod state;

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use rusqlite::Connection;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::info;

use crate::state::AppState;

/// stintlab web server.
///
/// Serves the REST API and static frontend assets.
#[derive(Parser)]
#[command(name = "stintlab-web", version, about)]
struct Cli {
    /// Port to listen on.
    #[arg(long, default_value_t = 3000)]
    port: u16,

    /// Path to the `SQLite` database file.
    #[arg(long, default_value = "data/races.db")]
    db: PathBuf,

    /// Path to the static web assets directory.
    #[arg(long, default_value = "web")]
    web_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "stintlab_web=info,tower_http=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    let db_path = cli.db.to_str().context("invalid database path")?;
    let conn = Connection::open(db_path).context("failed to open database")?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .context("failed to set WAL mode")?;

    let state = AppState::new(conn);

    let app = Router::new()
        .route("/api/races", get(api::list_races))
        .route("/api/races/{id}/laps", get(api::get_laps))
        .route("/api/races/{id}/stints", get(api::get_stints))
        .route("/api/predict/pit-window", post(api::predict_pit_window))
        .route(
            "/healthz",
            get(|| async { Json(serde_json::json!({"status": "ok"})) }),
        )
        .fallback_service(ServeDir::new(&cli.web_dir))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], cli.port));
    info!(%addr, "starting server");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
