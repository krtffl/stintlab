use std::sync::{Arc, Mutex};

use rusqlite::Connection;

/// Shared application state passed to all route handlers via axum's State extractor.
///
/// Wraps a `SQLite` connection in `Arc<Mutex<_>>` for thread-safe access.
/// `SQLite` in WAL mode supports concurrent reads, but we serialize writes
/// through the mutex to avoid contention.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
}

impl AppState {
    /// Create a new `AppState` from an existing `SQLite` connection.
    pub fn new(conn: Connection) -> Self {
        Self {
            db: Arc::new(Mutex::new(conn)),
        }
    }
}
