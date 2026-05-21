use std::env;
use std::time::Duration;

use eve_core::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub eve_client_id: String,
    pub eve_client_secret: String,
    pub eve_callback_url: String,
    pub eve_user_agent: String,
    pub jita_region_id: i64,
    pub poll_interval: Duration,
    pub google: Option<GoogleConfig>,
}

/// Service-account-based Google Sheets config. Optional so binaries
/// that don't touch Sheets (poll, rollup, report, …) can still boot
/// without these env vars set. Auth is fully headless — share the
/// target spreadsheet with the service account's `client_email`.
#[derive(Debug, Clone)]
pub struct GoogleConfig {
    /// Path to the service account JSON key file on disk.
    pub service_account_key_path: String,
    /// Spreadsheet ID (the long string between `/d/` and `/edit` in a
    /// Sheets URL).
    pub spreadsheet_id: String,
}

impl Config {
    pub fn from_env() -> AppResult<Self> {
        let _ = dotenvy::dotenv();

        Ok(Self {
            database_url: require("DATABASE_URL")?,
            eve_client_id: require("EVE_CLIENT_ID")?,
            eve_client_secret: require("EVE_CLIENT_SECRET")?,
            eve_callback_url: require("EVE_CALLBACK_URL")?,
            eve_user_agent: require("EVE_USER_AGENT")?,
            jita_region_id: optional_parse("JITA_REGION_ID")?.unwrap_or(10_000_002),
            poll_interval: Duration::from_secs(
                optional_parse::<u64>("POLL_INTERVAL_SECS")?.unwrap_or(300),
            ),
            google: GoogleConfig::from_env()?,
        })
    }

    pub fn google(&self) -> AppResult<&GoogleConfig> {
        self.google.as_ref().ok_or_else(|| {
            AppError::Config(
                "Google config missing — set GOOGLE_SERVICE_ACCOUNT_KEY_PATH and GOOGLE_SPREADSHEET_ID"
                    .into(),
            )
        })
    }
}

impl GoogleConfig {
    fn from_env() -> AppResult<Option<Self>> {
        let key = optional("GOOGLE_SERVICE_ACCOUNT_KEY_PATH");
        let sheet = optional("GOOGLE_SPREADSHEET_ID");
        match (key, sheet) {
            (None, None) => Ok(None),
            (Some(service_account_key_path), Some(spreadsheet_id)) => Ok(Some(Self {
                service_account_key_path,
                spreadsheet_id,
            })),
            _ => Err(AppError::Config(
                "Google config is partial — set both GOOGLE_SERVICE_ACCOUNT_KEY_PATH and GOOGLE_SPREADSHEET_ID, or neither".into(),
            )),
        }
    }
}

fn require(key: &str) -> AppResult<String> {
    env::var(key).map_err(|_| AppError::Config(format!("{key} is required")))
}

fn optional(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

fn optional_parse<T: std::str::FromStr>(key: &str) -> AppResult<Option<T>>
where
    T::Err: std::fmt::Display,
{
    match env::var(key) {
        Ok(v) if v.is_empty() => Ok(None),
        Ok(v) => v
            .parse::<T>()
            .map(Some)
            .map_err(|e| AppError::Config(format!("{key} is invalid: {e}"))),
        Err(_) => Ok(None),
    }
}
