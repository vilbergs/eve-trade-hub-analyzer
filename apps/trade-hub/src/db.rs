use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::Config;
use crate::error::AppResult;

pub async fn build_pool(config: &Config) -> AppResult<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Some(Duration::from_secs(600)))
        .max_lifetime(Some(Duration::from_secs(1800)))
        .connect(&config.database_url)
        .await?;
    Ok(pool)
}

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
