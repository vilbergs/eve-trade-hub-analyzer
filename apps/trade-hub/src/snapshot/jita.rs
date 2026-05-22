//! Jita (or any other configured region) poller.
//!
//! Public endpoint, no token. Same `tracked_types` whitelist as the hub
//! poller per ADDENDUM.md §2.

use std::time::Instant;

use chrono::Utc;
use sqlx::PgPool;
use tracing::{info, instrument};

use crate::Config;
use eve_core::AppResult;
use eve_esi::EsiClient;
use eve_esi::market::region_orders;

use super::{RunSummary, ensure_partitions, filter_to_tracked, hub::write_orders, tracked_types};

#[instrument(skip_all)]
pub async fn poll_jita(pool: &PgPool, esi: &EsiClient, config: &Config) -> AppResult<RunSummary> {
    let started = Utc::now();
    let started_instant = Instant::now();
    ensure_partitions(pool, started).await?;

    let region_id = config.jita_region_id;
    let run_id: i64 = sqlx::query_scalar(
        "INSERT INTO snapshot_runs (started_at, source, location_id) \
         VALUES ($1, 'jita', $2) RETURNING id",
    )
    .bind(started)
    .bind(region_id)
    .fetch_one(pool)
    .await?;

    let tracked = tracked_types(pool).await?;

    let result = poll_jita_inner(pool, esi, region_id, &tracked).await;
    let elapsed = started_instant.elapsed().as_millis() as i64;

    match &result {
        Ok((seen, kept)) => {
            sqlx::query(
                "UPDATE snapshot_runs SET finished_at = $1, orders_seen = $2, \
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
                "UPDATE snapshot_runs SET finished_at = $1, error = $2, duration_ms = $3 \
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
        source: "jita",
        location_id: Some(region_id),
        orders_seen: seen,
        orders_kept: kept,
        duration_ms: elapsed as u64,
    })
}

async fn poll_jita_inner(
    pool: &PgPool,
    esi: &EsiClient,
    region_id: i64,
    tracked: &std::collections::HashSet<i64>,
) -> AppResult<(u64, u64)> {
    let orders = region_orders(esi, region_id).await?;
    let seen = orders.len() as u64;
    let kept_orders = filter_to_tracked(orders, tracked);
    let kept = kept_orders.len() as u64;

    let observed_at = Utc::now();
    // Group orders by location_id and write per-location so the
    // DELETE in write_orders only wipes the slice we just refreshed.
    let mut by_loc: std::collections::HashMap<i64, Vec<_>> = std::collections::HashMap::new();
    for o in kept_orders {
        by_loc.entry(o.location_id).or_default().push(o);
    }
    // First wipe all rows for this region so locations that no longer have
    // any orders are emptied too.
    sqlx::query("DELETE FROM market_orders_current WHERE region_id = $1")
        .bind(region_id)
        .execute(pool)
        .await?;
    for (location_id, orders) in by_loc {
        write_orders(pool, location_id, Some(region_id), &orders, observed_at).await?;
    }

    info!(region_id, seen, kept, "jita snapshot written");
    Ok((seen, kept))
}
