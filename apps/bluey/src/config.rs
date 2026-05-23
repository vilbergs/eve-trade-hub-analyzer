use eve_core::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub listen_addr: String,
}

impl Config {
    pub fn from_env() -> AppResult<Self> {
        let _ = dotenvy::dotenv();

        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .map_err(|_| AppError::Config("DATABASE_URL is required".into()))?,
            listen_addr: std::env::var("LISTEN_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:3001".to_string()),
        })
    }
}
