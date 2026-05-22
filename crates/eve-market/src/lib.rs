//! Market storage layer.
//!
//! Owns the on-disk shape of an order book (`market_orders_current`,
//! `market_orders_snapshots` + weekly partitions) and the daily
//! aggregate (`market_orders_daily`). Exposes pure write helpers
//! (`ingest_orders`, `roll_day`), partition management, and a generic
//! `filter_by_type` helper. The polling driver itself lives in the
//! consuming app — eve-market knows nothing about how rows arrive,
//! only what to do with them once they're here.

use std::collections::HashSet;

use chrono::{DateTime, Datelike, Days, NaiveDate, Utc};
use sqlx::PgPool;

use eve_core::{AppError, AppResult};
use eve_esi::market::MarketOrder;

/// Per-cycle telemetry returned by polling drivers. Kept here (rather
/// than the driver crate) because it's a property of having ingested
/// a batch, not a property of any one driver.
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub source: &'static str,
    pub location_id: Option<i64>,
    pub orders_seen: u64,
    pub orders_kept: u64,
    pub duration_ms: u64,
}

/// Keep only orders whose `type_id` is in `allowed`. Generic over the
/// allowed-set so callers can pass any collection that hashes.
pub fn filter_by_type(orders: Vec<MarketOrder>, allowed: &HashSet<i64>) -> Vec<MarketOrder> {
    orders
        .into_iter()
        .filter(|o| allowed.contains(&o.type_id))
        .collect()
}

/// Ensure a weekly partition exists for `target` and for the following week.
/// Idempotent: `CREATE TABLE IF NOT EXISTS` skips when already created.
pub async fn ensure_partitions(pool: &PgPool, target: DateTime<Utc>) -> AppResult<()> {
    for offset in 0..2u64 {
        let start = week_start(target) + Days::new(offset * 7);
        let end = start + Days::new(7);
        let name = format!("market_orders_snapshots_{}", start.format("%Y%m%d"));
        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {name} \
             PARTITION OF market_orders_snapshots \
             FOR VALUES FROM ('{start}') TO ('{end}')",
            start = start.format("%Y-%m-%d"),
            end = end.format("%Y-%m-%d"),
        );
        sqlx::query(&sql).execute(pool).await?;
    }
    Ok(())
}

/// Drop weekly partitions whose end is older than `cutoff`. Returns
/// the count dropped.
pub async fn drop_old_partitions(pool: &PgPool, cutoff: DateTime<Utc>) -> AppResult<u64> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT child.relname \
         FROM pg_inherits i \
         JOIN pg_class child ON child.oid = i.inhrelid \
         JOIN pg_class parent ON parent.oid = i.inhparent \
         WHERE parent.relname = 'market_orders_snapshots'",
    )
    .fetch_all(pool)
    .await?;

    let mut dropped = 0;
    for (name,) in rows {
        let Some(date_part) = name.strip_prefix("market_orders_snapshots_") else {
            continue;
        };
        let Ok(start) = NaiveDate::parse_from_str(date_part, "%Y%m%d") else {
            continue;
        };
        let end = start + chrono::Duration::days(7);
        let end_utc = end
            .and_hms_opt(0, 0, 0)
            .map(|d| d.and_utc())
            .unwrap_or(cutoff);
        if end_utc <= cutoff {
            let sql = format!("DROP TABLE IF EXISTS {name}");
            sqlx::query(&sql).execute(pool).await?;
            dropped += 1;
        }
    }
    Ok(dropped)
}

/// Replace `market_orders_current` rows for `location_id` with the
/// supplied batch and append matching `market_orders_snapshots` rows
/// at `observed_at`. One transaction.
pub async fn ingest_orders(
    pool: &PgPool,
    location_id: i64,
    region_id: Option<i64>,
    orders: &[MarketOrder],
    observed_at: DateTime<Utc>,
) -> AppResult<()> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM market_orders_current WHERE location_id = $1")
        .bind(location_id)
        .execute(&mut *tx)
        .await?;

    for o in orders {
        sqlx::query(
            "INSERT INTO market_orders_current \
                (order_id, location_id, region_id, type_id, is_buy, price, \
                 volume_remain, volume_total, min_volume, range, issued, \
                 duration_days, observed_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
             ON CONFLICT (order_id) DO UPDATE SET \
                location_id = EXCLUDED.location_id, \
                region_id = EXCLUDED.region_id, \
                type_id = EXCLUDED.type_id, \
                is_buy = EXCLUDED.is_buy, \
                price = EXCLUDED.price, \
                volume_remain = EXCLUDED.volume_remain, \
                volume_total = EXCLUDED.volume_total, \
                min_volume = EXCLUDED.min_volume, \
                range = EXCLUDED.range, \
                issued = EXCLUDED.issued, \
                duration_days = EXCLUDED.duration_days, \
                observed_at = EXCLUDED.observed_at",
        )
        .bind(o.order_id)
        .bind(location_id)
        .bind(region_id)
        .bind(o.type_id)
        .bind(o.is_buy_order)
        .bind(o.price)
        .bind(o.volume_remain)
        .bind(o.volume_total)
        .bind(o.min_volume)
        .bind(&o.range)
        .bind(o.issued)
        .bind(o.duration)
        .bind(observed_at)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO market_orders_snapshots \
                (order_id, snapshot_ts, location_id, region_id, type_id, \
                 is_buy, price, volume_remain) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
             ON CONFLICT (snapshot_ts, order_id) DO NOTHING",
        )
        .bind(o.order_id)
        .bind(observed_at)
        .bind(location_id)
        .bind(region_id)
        .bind(o.type_id)
        .bind(o.is_buy_order)
        .bind(o.price)
        .bind(o.volume_remain)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Wipe every `market_orders_current` row for `region_id`. Used by
/// region-wide pollers before re-ingesting per-location batches so
/// locations that no longer carry orders end up empty.
pub async fn delete_region_current(pool: &PgPool, region_id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM market_orders_current WHERE region_id = $1")
        .bind(region_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Aggregate one UTC day of `market_orders_snapshots` into
/// `market_orders_daily` (per-day open/close/min/max lowest sell +
/// consumption deltas). Idempotent — re-running overwrites the day's
/// row.
pub async fn roll_day(pool: &PgPool, day: NaiveDate) -> AppResult<u64> {
    let day_start = day
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| AppError::Other("invalid day".into()))?
        .and_utc();
    let day_end = day_start + chrono::Duration::days(1);

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
        INSERT INTO market_orders_daily (
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

fn week_start(t: DateTime<Utc>) -> NaiveDate {
    let date = t.date_naive();
    let weekday = date.weekday().num_days_from_monday() as i64;
    date - chrono::Duration::days(weekday)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn week_start_anchors_to_monday() {
        let wed = Utc.with_ymd_and_hms(2026, 5, 13, 9, 0, 0).unwrap();
        assert_eq!(
            week_start(wed),
            NaiveDate::from_ymd_opt(2026, 5, 11).unwrap()
        );
        let mon = Utc.with_ymd_and_hms(2026, 5, 11, 9, 0, 0).unwrap();
        assert_eq!(
            week_start(mon),
            NaiveDate::from_ymd_opt(2026, 5, 11).unwrap()
        );
    }

    #[test]
    fn filter_by_type_drops_others() {
        let tracked: HashSet<i64> = [34_i64, 35].into_iter().collect();
        let orders = vec![order(1, 34), order(2, 35), order(3, 99), order(4, 34)];
        let kept = filter_by_type(orders, &tracked);
        assert_eq!(kept.len(), 3);
        assert!(kept.iter().all(|o| tracked.contains(&o.type_id)));
    }

    fn order(order_id: i64, type_id: i64) -> MarketOrder {
        MarketOrder {
            order_id,
            type_id,
            location_id: 60003760,
            system_id: Some(30000142),
            volume_total: 100,
            volume_remain: 100,
            min_volume: 1,
            price: 1000.0,
            is_buy_order: false,
            duration: 90,
            issued: Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap(),
            range: "region".into(),
        }
    }
}
