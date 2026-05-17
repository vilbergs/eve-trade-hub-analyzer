use std::env;
use std::time::Duration;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub eve_client_id: String,
    pub eve_client_secret: String,
    pub eve_callback_url: String,
    pub eve_user_agent: String,
    pub jita_region_id: i64,
    pub poll_interval: Duration,
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
        })
    }
}

fn require(key: &str) -> AppResult<String> {
    env::var(key).map_err(|_| AppError::Config(format!("{key} is required")))
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
