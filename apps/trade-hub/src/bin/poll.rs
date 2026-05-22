//! `poll` daemon — runs hub + jita snapshots on independent timers so
//! neither poller blocks the other.
//!
//! Lifecycle:
//! - On startup: ensure partitions, then spawn hub + jita tasks.
//! - Each task ticks on its own `POLL_INTERVAL_SECS` interval; errors are
//!   recorded in `snapshot_runs` by the pollers, the tasks keep going.
//! - Once per UTC day: ensure new partitions, drop partitions whose week
//!   ended more than 30 days ago.
//! - SIGINT / SIGTERM: cancel both tasks and exit cleanly.

use chrono::{Datelike, Utc};
use clap::Parser;
use eve_esi::EsiClient;
use eve_auth::{AccessTokenCache, AuthEndpoints};
use eve_trade_hub_analyzer::snapshot::{drop_old_partitions, ensure_partitions, hub, jita};
use eve_trade_hub_analyzer::{Config, db};
use eve_core::telemetry;
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
    let esi = EsiClient::new(&config.eve_user_agent)?;
    let cache = AccessTokenCache::new();
    let endpoints = AuthEndpoints::production();

    ensure_partitions(&pool, Utc::now()).await?;

    if args.once {
        let (hub_res, jita_res) = tokio::join!(
            hub::poll_hub(&pool, &http, &esi, &cache, &config, &endpoints),
            jita::poll_jita(&pool, &esi, &config),
        );
        log_hub_result(hub_res);
        log_jita_result(jita_res);
        return Ok(());
    }

    // --- Spawn independent poller tasks ---

    let (shutdown_tx, _) = tokio::sync::watch::channel(false);

    let hub_handle = tokio::spawn({
        let pool = pool.clone();
        let http = http.clone();
        let esi = esi.clone();
        let cache = cache.clone();
        let config = config.clone();
        let endpoints = endpoints.clone();
        let mut shutdown_rx = shutdown_tx.subscribe();
        async move {
            let mut ticker = tokio::time::interval(config.poll_interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        let res = hub::poll_hub(&pool, &http, &esi, &cache, &config, &endpoints).await;
                        log_hub_result(res);
                    }
                    _ = shutdown_rx.changed() => break,
                }
            }
        }
    });

    let jita_handle = tokio::spawn({
        let pool = pool.clone();
        let esi = esi.clone();
        let config = config.clone();
        let mut shutdown_rx = shutdown_tx.subscribe();
        async move {
            let mut ticker = tokio::time::interval(config.poll_interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        let res = jita::poll_jita(&pool, &esi, &config).await;
                        log_jita_result(res);
                    }
                    _ = shutdown_rx.changed() => break,
                }
            }
        }
    });

    // --- Main task: signal handling + partition housekeeping ---

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut last_partition_day = Utc::now().day();

    // Check for partition housekeeping every hour.
    let mut housekeeping_ticker = tokio::time::interval(std::time::Duration::from_secs(3600));
    housekeeping_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = housekeeping_ticker.tick() => {
                let now = Utc::now();
                if now.day() != last_partition_day {
                    if let Err(e) = partition_housekeeping(&pool).await {
                        error!(error = %e, "partition housekeeping failed");
                    }
                    last_partition_day = now.day();
                }
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

    let _ = shutdown_tx.send(true);
    let _ = hub_handle.await;
    let _ = jita_handle.await;

    Ok(())
}

fn log_hub_result(
    res: Result<
        Vec<eve_trade_hub_analyzer::snapshot::RunSummary>,
        eve_core::AppError,
    >,
) {
    match res {
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
}

fn log_jita_result(
    res: Result<
        eve_trade_hub_analyzer::snapshot::RunSummary,
        eve_core::AppError,
    >,
) {
    match res {
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
