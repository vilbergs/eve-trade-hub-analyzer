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
    pub token_encryption_key: [u8; 32],
    pub hub_structure_id: Option<i64>,
    pub haul_isk_per_m3: f64,
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
            token_encryption_key: decode_key(&require("TOKEN_ENCRYPTION_KEY")?)?,
            hub_structure_id: optional_parse("HUB_STRUCTURE_ID")?,
            haul_isk_per_m3: optional_parse("HAUL_ISK_PER_M3")?.unwrap_or(1000.0),
            jita_region_id: optional_parse("JITA_REGION_ID")?.unwrap_or(10_000_002),
            poll_interval: Duration::from_secs(
                optional_parse::<u64>("POLL_INTERVAL_SECS")?.unwrap_or(300),
            ),
        })
    }

    pub fn require_hub_structure_id(&self) -> AppResult<i64> {
        self.hub_structure_id
            .ok_or_else(|| AppError::Config("HUB_STRUCTURE_ID is not set".into()))
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

fn decode_key(b64: &str) -> AppResult<[u8; 32]> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let bytes = STANDARD
        .decode(b64.trim())
        .map_err(|e| AppError::Config(format!("TOKEN_ENCRYPTION_KEY is not valid base64: {e}")))?;
    if bytes.len() != 32 {
        return Err(AppError::Config(format!(
            "TOKEN_ENCRYPTION_KEY must decode to 32 bytes, got {}",
            bytes.len()
        )));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_key_rejects_short_input() {
        let short = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [0u8; 16]);
        assert!(decode_key(&short).is_err());
    }

    #[test]
    fn decode_key_accepts_32_bytes() {
        let good = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [7u8; 32]);
        let k = decode_key(&good).unwrap();
        assert_eq!(k, [7u8; 32]);
    }
}
