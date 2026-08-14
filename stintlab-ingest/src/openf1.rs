use std::time::Duration;

use serde::Deserialize;
use tracing::{debug, warn};

use stintlab_core::error::StintlabError;

/// `OpenF1` community tier base URL (free, no auth, 3 req/sec).
const BASE_URL: &str = "https://api.openf1.org/v1";

/// Rate limit delay between requests (community tier: 3 req/sec).
const RATE_LIMIT_DELAY: Duration = Duration::from_millis(340);

/// Maximum retry attempts for rate-limited requests.
const MAX_RETRIES: u32 = 3;

/// `OpenF1` API client for fetching historical F1 data.
pub struct OpenF1Client {
    http: reqwest::Client,
}

/// Raw session data from `OpenF1`.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct RawSession {
    pub session_key: i64,
    pub session_name: String,
    pub session_type: Option<String>,
    pub country_name: Option<String>,
    pub circuit_short_name: Option<String>,
    pub date_start: Option<String>,
    pub year: Option<u16>,
}

/// Raw lap data from `OpenF1`.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct RawLap {
    pub session_key: i64,
    pub driver_number: u16,
    pub lap_number: Option<u16>,
    pub lap_duration: Option<f64>,
    pub duration_sector_1: Option<f64>,
    pub duration_sector_2: Option<f64>,
    pub duration_sector_3: Option<f64>,
    pub is_pit_out_lap: Option<bool>,
    pub i1_speed: Option<f64>,
    pub i2_speed: Option<f64>,
    pub st_speed: Option<f64>,
}

/// Raw stint data from `OpenF1`.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct RawStint {
    pub session_key: i64,
    pub driver_number: u16,
    pub stint_number: Option<u8>,
    pub compound: Option<String>,
    pub tyre_age_at_start: Option<u16>,
    pub lap_start: Option<u16>,
    pub lap_end: Option<u16>,
}

/// Raw driver data from `OpenF1`.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct RawDriver {
    pub session_key: i64,
    pub driver_number: u16,
    pub broadcast_name: Option<String>,
    pub name_acronym: Option<String>,
    pub team_name: Option<String>,
}

impl OpenF1Client {
    /// Create a new client with default settings.
    pub fn new() -> Result<Self, StintlabError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| StintlabError::OpenF1Error {
                status: 0,
                body: e.to_string(),
            })?;
        Ok(Self { http })
    }

    /// Fetch race sessions for a given year, optionally filtered by country.
    pub async fn get_sessions(
        &self,
        year: u16,
        country: Option<&str>,
    ) -> Result<Vec<RawSession>, StintlabError> {
        let mut url = format!("{BASE_URL}/sessions?year={year}&session_type=Race");
        if let Some(c) = country {
            url.push_str("&country_name=");
            url.push_str(c);
        }
        debug!(url = %url, "fetching sessions");
        self.get_json(&url).await
    }

    /// Fetch lap data for a session.
    pub async fn get_laps(&self, session_key: i64) -> Result<Vec<RawLap>, StintlabError> {
        let url = format!("{BASE_URL}/laps?session_key={session_key}");
        debug!(session_key, "fetching laps");
        self.get_json(&url).await
    }

    /// Fetch stint data for a session.
    pub async fn get_stints(&self, session_key: i64) -> Result<Vec<RawStint>, StintlabError> {
        let url = format!("{BASE_URL}/stints?session_key={session_key}");
        debug!(session_key, "fetching stints");
        self.get_json(&url).await
    }

    /// Fetch driver data for a session.
    pub async fn get_drivers(&self, session_key: i64) -> Result<Vec<RawDriver>, StintlabError> {
        let url = format!("{BASE_URL}/drivers?session_key={session_key}");
        debug!(session_key, "fetching drivers");
        self.get_json(&url).await
    }

    /// Generic GET request with rate limiting and retry on 429.
    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<T, StintlabError> {
        let mut attempts = 0;

        loop {
            tokio::time::sleep(RATE_LIMIT_DELAY).await;

            let response =
                self.http
                    .get(url)
                    .send()
                    .await
                    .map_err(|e| StintlabError::OpenF1Error {
                        status: 0,
                        body: e.to_string(),
                    })?;

            let status = response.status();

            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                attempts += 1;
                if attempts >= MAX_RETRIES {
                    return Err(StintlabError::RateLimited {
                        retry_after_secs: 30,
                    });
                }
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(2);
                warn!(retry_after, attempt = attempts, "rate limited, backing off");
                tokio::time::sleep(Duration::from_secs(retry_after)).await;
                continue;
            }

            if !status.is_success() {
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| String::from("<failed to read body>"));
                return Err(StintlabError::OpenF1Error {
                    status: status.as_u16(),
                    body,
                });
            }

            let data = response
                .json::<T>()
                .await
                .map_err(|e| StintlabError::OpenF1Error {
                    status: status.as_u16(),
                    body: format!("deserialization error: {e}"),
                })?;

            return Ok(data);
        }
    }
}
