//! `poll` daemon — runs hub + jita snapshots on `POLL_INTERVAL_SECS` and
//! keeps the weekly partitions of `market_orders_snapshots` tidy.
//!
//! Lifecycle:
//! - On startup: ensure partitions, then enter the tick loop.
//! - Each tick: spawn `poll_hub` + `poll_jita` in parallel; log per-source
//!   summaries; errors are recorded in `snapshot_runs` by the pollers
//!   themselves, the daemon keeps going.
//! - Once per UTC day: ensure new partitions, drop partitions whose week
//!   ended more than 30 days ago.
//! - SIGINT / SIGTERM: finish whatever's in flight, then exit cleanly.

use chrono::{Datelike, Utc};
use clap::Parser;
use eve_trade_hub_analyzer::esi::EsiClient;
use eve_trade_hub_analyzer::esi::auth::{AccessTokenCache, AuthEndpoints};
use eve_trade_hub_analyzer::snapshot::{drop_old_partitions, ensure_partitions, hub, jita};
use eve_trade_hub_analyzer::{Config, db, telemetry};
use tokio::signal::unix::{SignalKind, signal};
use tracing::{error, info};

const RETENTION_DAYS: i64 = 30;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Run a single poll cycle and exit (useful for testing).
    #[arg(long, default_value_t = false)]
    once: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init();
    let args = Args::parse();
    let config = Config::from_env()?;
    let pool = db::build_pool(&config).await?;
    let http = reqwest::Client::builder()
        .user_agent(&config.eve_user_agent)
        .gzip(true)
        .build()?;
    let esi = EsiClient::new(&config)?;
    let cache = AccessTokenCache::new();
    let endpoints = AuthEndpoints::production();

    ensure_partitions(&pool, Utc::now()).await?;

    if args.once {
        run_cycle(&pool, &http, &esi, &cache, &config, &endpoints).await;
        return Ok(());
    }

    let mut ticker = tokio::time::interval(config.poll_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_partition_day = Utc::now().day();

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let now = Utc::now();
                // Run partition housekeeping on the first tick of every UTC day.
                if now.day() != last_partition_day {
                    if let Err(e) = partition_housekeeping(&pool).await {
                        error!(error = %e, "partition housekeeping failed");
                    }
                    last_partition_day = now.day();
                }
                run_cycle(&pool, &http, &esi, &cache, &config, &endpoints).await;
            }
            _ = sigterm.recv() => {
                info!("SIGTERM received, shutting down");
                break;
            }
            _ = sigint.recv() => {
                info!("SIGINT received, shutting down");
                break;
            }
        }
    }

    Ok(())
}

async fn run_cycle(
    pool: &sqlx::PgPool,
    http: &reqwest::Client,
    esi: &EsiClient,
    cache: &AccessTokenCache,
    config: &Config,
    endpoints: &AuthEndpoints,
) {
    let (hub_res, jita_res) = tokio::join!(
        hub::poll_hub(pool, http, esi, cache, config, endpoints),
        jita::poll_jita(pool, esi, config),
    );
    match hub_res {
        Ok(summaries) => {
            for s in summaries {
                info!(
                    source = s.source,
                    station_id = ?s.location_id,
                    orders_seen = s.orders_seen,
                    orders_kept = s.orders_kept,
                    duration_ms = s.duration_ms,
                    "hub poll ok"
                );
            }
        }
        Err(e) => error!(error = %e, "hub poll failed"),
    }
    match jita_res {
        Ok(s) => info!(
            source = s.source,
            region_id = ?s.location_id,
            orders_seen = s.orders_seen,
            orders_kept = s.orders_kept,
            duration_ms = s.duration_ms,
            "jita poll ok"
        ),
        Err(e) => error!(error = %e, "jita poll failed"),
    }
}

async fn partition_housekeeping(pool: &sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let now = Utc::now();
    ensure_partitions(pool, now).await?;
    let cutoff = now - chrono::Duration::days(RETENTION_DAYS);
    let dropped = drop_old_partitions(pool, cutoff).await?;
    info!(
        dropped,
        retention_days = RETENTION_DAYS,
        "partition housekeeping done"
    );
    Ok(())
}
