use std::path::PathBuf;

use eve_core::{AppError, AppResult};

pub struct Config {
    pub db_path: PathBuf,
    pub chatlog_dir: PathBuf,
    pub dirty_timeout_min: i64,
}

impl Config {
    pub fn from_env() -> AppResult<Self> {
        let db_path = match std::env::var("INTEL_DB_PATH") {
            Ok(v) => PathBuf::from(v),
            Err(_) => {
                let base = dirs::data_local_dir().ok_or_else(|| {
                    AppError::Config("could not resolve data_local_dir".into())
                })?;
                base.join("eve-intel").join("intel.sqlite")
            }
        };
        let chatlog_dir = match std::env::var("INTEL_CHATLOG_DIR") {
            Ok(v) => PathBuf::from(v),
            Err(_) => {
                let docs = dirs::document_dir().ok_or_else(|| {
                    AppError::Config("could not resolve document_dir".into())
                })?;
                docs.join("EVE").join("logs").join("Chatlogs")
            }
        };
        let dirty_timeout_min = std::env::var("INTEL_DIRTY_TIMEOUT_MIN")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(20);

        Ok(Self {
            db_path,
            chatlog_dir,
            dirty_timeout_min,
        })
    }
}
