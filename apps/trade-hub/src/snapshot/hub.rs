//! Hub poller — one cycle = one snapshot of every `tracked_stations` row.

use std::collections::HashSet;
use std::time::Instant;

use chrono::Utc;
use sqlx::PgPool;
use tracing::{error, info, instrument, warn};

use crate::Config;
use eve_auth::{AccessTokenCache, AuthEndpoints, get_access_token};
use eve_core::{AppError, AppResult};
use eve_esi::EsiClient;
use eve_esi::market::structure_orders;
use eve_market::{RunSummary, ensure_partitions, filter_by_type, ingest_orders};

use super::tracked_types;

#[instrument(skip_all)]
pub async fn poll_hub(
    pool: &PgPool,
    http: &reqwest::Client,
    esi: &EsiClient,
    cache: &AccessTokenCache,
    config: &Config,
    endpoints: &AuthEndpoints,
) -> AppResult<Vec<RunSummary>> {
    let stations: Vec<i64> = sqlx::query_scalar("SELECT station_id FROM tracked_stations")
        .fetch_all(pool)
        .await?;
    if stations.is_empty() {
        info!("no tracked stations, hub cycle is a no-op");
        return Ok(Vec::new());
    }

    let tracked = tracked_types(pool).await?;
    if tracked.is_empty() {
        warn!("tracked_types is empty; every order will be dropped");
    }
    ensure_partitions(pool, Utc::now()).await?;

    let character_id: Option<i64> = sqlx::query_scalar(
        "SELECT character_id FROM characters \
         WHERE status = 'active' \
         ORDER BY character_id LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    let character_id = character_id.ok_or_else(|| {
        AppError::Auth("no active linked character; run `cargo run --bin auth`".into())
    })?;
    let sso = config.eve_sso();
    let token = get_access_token(cache, &sso, endpoints, pool, http, character_id).await?;

    let mut summaries = Vec::with_capacity(stations.len());
    for station_id in stations {
        match poll_one_station(pool, esi, &token, station_id, &tracked).await {
            Ok(summary) => summaries.push(summary),
            Err(e) => {
                error!(station_id, error = %e, "hub station poll failed");
            }
        }
    }
    Ok(summaries)
}

#[instrument(skip_all, fields(station_id))]
async fn poll_one_station(
    pool: &PgPool,
    esi: &EsiClient,
    access_token: &str,
    station_id: i64,
    tracked: &HashSet<i64>,
) -> AppResult<RunSummary> {
    let started = Utc::now();
    let started_instant = Instant::now();
    let run_id: i64 = sqlx::query_scalar(
        "INSERT INTO market_poll_runs (started_at, source, location_id) \
         VALUES ($1, 'hub', $2) RETURNING id",
    )
    .bind(started)
    .bind(station_id)
    .fetch_one(pool)
    .await?;

    let result = poll_one_station_inner(pool, esi, access_token, station_id, tracked).await;

    let elapsed = started_instant.elapsed().as_millis() as i64;
    match &result {
        Ok((seen, kept)) => {
            sqlx::query(
                "UPDATE market_poll_runs SET finished_at = $1, orders_seen = $2, \
                 orders_kept = $3, duration_ms = $4 WHERE id = $5",
            )
            .bind(Utc::now())
            .bind(*seen as i32)
            .bind(*kept as i32)
            .bind(elapsed as i32)
            .bind(run_id)
            .execute(pool)
            .await?;
        }
        Err(e) => {
            sqlx::query(
                "UPDATE market_poll_runs SET finished_at = $1, error = $2, duration_ms = $3 \
                 WHERE id = $4",
            )
            .bind(Utc::now())
            .bind(e.to_string())
            .bind(elapsed as i32)
            .bind(run_id)
            .execute(pool)
            .await?;
        }
    }

    let (seen, kept) = result?;
    Ok(RunSummary {
        source: "hub",
        location_id: Some(station_id),
        orders_seen: seen,
        orders_kept: kept,
        duration_ms: elapsed as u64,
    })
}

async fn poll_one_station_inner(
    pool: &PgPool,
    esi: &EsiClient,
    access_token: &str,
    station_id: i64,
    tracked: &HashSet<i64>,
) -> AppResult<(u64, u64)> {
    let orders = structure_orders(esi, station_id, access_token).await?;
    let seen = orders.len() as u64;
    let kept_orders = filter_by_type(orders, tracked);
    let kept = kept_orders.len() as u64;

    let observed_at = Utc::now();
    ingest_orders(pool, station_id, None, &kept_orders, observed_at).await?;

    info!(station_id, seen, kept, "hub snapshot written");
    Ok((seen, kept))
}
