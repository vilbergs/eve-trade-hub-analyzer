//! `rollup` binary — fold a day of snapshots into market_daily_agg, then
//! refresh Jita ESI history for every type currently seen in
//! market_orders_current at the Jita region.
//!
//! Usage: `rollup [--day YYYY-MM-DD]`. Default day is yesterday UTC.

use chrono::{Duration, NaiveDate, Utc};
use clap::Parser;
use eve_trade_hub_analyzer::error::{AppError, AppResult};
use eve_trade_hub_analyzer::esi::EsiClient;
use eve_trade_hub_analyzer::esi::market;
use eve_trade_hub_analyzer::{Config, db, telemetry};
use futures::StreamExt;
use sqlx::PgPool;
use tracing::{info, warn};

const HISTORY_CONCURRENCY: usize = 20;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// The day to roll up (YYYY-MM-DD). Defaults to yesterday UTC.
    #[arg(long)]
    day: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init();
    let args = Args::parse();
    let config = Config::from_env()?;
    let pool = db::build_pool(&config).await?;
    let esi = EsiClient::new(&config)?;

    let day: NaiveDate = match args.day {
        Some(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d")
            .map_err(|e| AppError::Other(format!("--day must be YYYY-MM-DD: {e}")))?,
        None => (Utc::now() - Duration::days(1)).date_naive(),
    };
    info!(%day, "rolling up");

    let aggregated = roll_day(&pool, day).await?;
    info!(%day, rows = aggregated, "market_daily_agg upserted");

    let updated = refresh_jita_history(&pool, &esi, config.jita_region_id).await?;
    info!(
        region_id = config.jita_region_id,
        rows = updated,
        "market_history upserted"
    );

    Ok(())
}

/// Compute per-(location, type) lowest_sell extrema and consumption deltas
/// for `day`'s snapshots and upsert into market_daily_agg.
async fn roll_day(pool: &PgPool, day: NaiveDate) -> AppResult<u64> {
    let day_start = day
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| AppError::Other("invalid day".into()))?
        .and_utc();
    let day_end = day_start + Duration::days(1);

    let res = sqlx::query(
        r#"
        WITH
        per_snapshot_lowest AS (
            SELECT
                location_id,
                type_id,
                snapshot_ts,
                MIN(price) FILTER (WHERE is_buy = false) AS lowest_sell
            FROM market_orders_snapshots
            WHERE snapshot_ts >= $1 AND snapshot_ts < $2
            GROUP BY location_id, type_id, snapshot_ts
        ),
        price_stats AS (
            SELECT
                location_id,
                type_id,
                (array_agg(lowest_sell ORDER BY snapshot_ts ASC)
                     FILTER (WHERE lowest_sell IS NOT NULL))[1]  AS open_lowest_sell,
                (array_agg(lowest_sell ORDER BY snapshot_ts DESC)
                     FILTER (WHERE lowest_sell IS NOT NULL))[1] AS close_lowest_sell,
                MIN(lowest_sell)                                  AS min_lowest_sell,
                MAX(lowest_sell)                                  AS max_lowest_sell
            FROM per_snapshot_lowest
            GROUP BY location_id, type_id
        ),
        ordered AS (
            SELECT
                order_id, location_id, type_id, snapshot_ts, price, volume_remain,
                LAG(volume_remain) OVER (PARTITION BY order_id ORDER BY snapshot_ts) AS prev_remain
            FROM market_orders_snapshots
            WHERE snapshot_ts >= $1 AND snapshot_ts < $2
        ),
        consumption AS (
            SELECT
                location_id,
                type_id,
                COALESCE(SUM(GREATEST(0, prev_remain - volume_remain))::BIGINT, 0)        AS units_consumed,
                COALESCE(SUM(GREATEST(0, prev_remain - volume_remain) * price), 0::DOUBLE PRECISION) AS isk_consumed
            FROM ordered
            WHERE prev_remain IS NOT NULL
            GROUP BY location_id, type_id
        )
        INSERT INTO market_daily_agg (
            day, location_id, type_id,
            open_lowest_sell, close_lowest_sell, min_lowest_sell, max_lowest_sell,
            units_consumed, isk_consumed
        )
        SELECT
            $3::DATE,
            COALESCE(ps.location_id, c.location_id),
            COALESCE(ps.type_id, c.type_id),
            ps.open_lowest_sell,
            ps.close_lowest_sell,
            ps.min_lowest_sell,
            ps.max_lowest_sell,
            COALESCE(c.units_consumed, 0),
            COALESCE(c.isk_consumed, 0)
        FROM price_stats ps
        FULL OUTER JOIN consumption c
            ON ps.location_id = c.location_id AND ps.type_id = c.type_id
        ON CONFLICT (day, location_id, type_id) DO UPDATE SET
            open_lowest_sell  = EXCLUDED.open_lowest_sell,
            close_lowest_sell = EXCLUDED.close_lowest_sell,
            min_lowest_sell   = EXCLUDED.min_lowest_sell,
            max_lowest_sell   = EXCLUDED.max_lowest_sell,
            units_consumed    = EXCLUDED.units_consumed,
            isk_consumed      = EXCLUDED.isk_consumed
        "#,
    )
    .bind(day_start)
    .bind(day_end)
    .bind(day)
    .execute(pool)
    .await?;

    Ok(res.rows_affected())
}

/// Fetch ESI history for every type currently present in
/// market_orders_current at the Jita region and upsert.
async fn refresh_jita_history(pool: &PgPool, esi: &EsiClient, region_id: i64) -> AppResult<u64> {
    let type_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT type_id FROM market_orders_current WHERE region_id = $1",
    )
    .bind(region_id)
    .fetch_all(pool)
    .await?;

    if type_ids.is_empty() {
        warn!(
            region_id,
            "no types in market_orders_current for region; nothing to refresh"
        );
        return Ok(0);
    }

    let results: Vec<_> = futures::stream::iter(type_ids)
        .map(|type_id| {
            let esi = esi.clone();
            let pool = pool.clone();
            async move {
                let history = match market::region_history(&esi, region_id, type_id).await {
                    Ok(h) => h,
                    Err(e) => {
                        warn!(type_id, error = %e, "history fetch failed");
                        return 0u64;
                    }
                };
                let mut written = 0u64;
                for h in history {
                    let res = sqlx::query(
                        "INSERT INTO market_history \
                            (region_id, type_id, date, average, highest, lowest, volume, order_count) \
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
                         ON CONFLICT (region_id, type_id, date) DO UPDATE SET \
                            average = EXCLUDED.average, \
                            highest = EXCLUDED.highest, \
                            lowest = EXCLUDED.lowest, \
                            volume = EXCLUDED.volume, \
                            order_count = EXCLUDED.order_count",
                    )
                    .bind(region_id)
                    .bind(type_id)
                    .bind(h.date)
                    .bind(h.average)
                    .bind(h.highest)
                    .bind(h.lowest)
                    .bind(h.volume)
                    .bind(h.order_count)
                    .execute(&pool)
                    .await;
                    match res {
                        Ok(r) => written += r.rows_affected(),
                        Err(e) => warn!(type_id, error = %e, "history insert failed"),
                    }
                }
                written
            }
        })
        .buffer_unordered(HISTORY_CONCURRENCY)
        .collect()
        .await;

    Ok(results.into_iter().sum())
}
