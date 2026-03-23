use crate::models::Compound;

/// All errors that can originate from the stintlab-core library.
///
/// Infrastructure errors (database, HTTP) are represented as strings
/// so this crate stays free of infrastructure dependencies.
#[derive(thiserror::Error, Debug)]
pub enum StintlabError {
    #[error("race not found: {0}")]
    RaceNotFound(i64),

    #[error("degradation model not found for {compound:?} at circuit {circuit_key}")]
    DegradationModelNotFound {
        compound: Compound,
        circuit_key: String,
    },

    #[error("insufficient data: {0}")]
    InsufficientData(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("OpenF1 API error: {status} - {body}")]
    OpenF1Error { status: u16, body: String },

    #[error("OpenF1 rate limited (HTTP 429), retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("FastF1 error: {0}")]
    FastF1Error(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
