//! `migrate` binary — apply the embedded SQL migrations to DATABASE_URL.
//!
//! Runs the migrations compiled into the binary (`db::MIGRATOR`) so a Pi
//! deploy needs no separate sqlx-cli install, and the applied schema can never
//! drift from the binaries that were shipped alongside it. Idempotent:
//! already-applied migrations are skipped (and their checksums verified).
//!
//! Only DATABASE_URL is required — EVE credentials are not — so migrations can
//! run before the rest of the env file is filled in.

use clap::Parser;
use eve_core::{AppError, telemetry};
use eve_trade_hub_analyzer::db;
use sqlx::postgres::PgPoolOptions;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init();
    let _args = Args::parse();
    let _ = dotenvy::dotenv();

    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| AppError::Config("DATABASE_URL is required".into()))?;

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;

    db::MIGRATOR.run(&pool).await?;
    tracing::info!("migrations up to date");
    Ok(())
}
