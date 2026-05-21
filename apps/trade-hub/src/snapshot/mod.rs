//! Market snapshot pollers.
//!
//! Two flavours: `hub` (loops over every `tracked_stations` row and pulls
//! `/markets/structures/{id}/` with a character access token) and `jita`
//! (pulls `/markets/{JITA_REGION_ID}/orders/`, no auth). Both filter the
//! ESI response down to `tracked_types` before persisting, per
//! ADDENDUM.md §2.

pub mod hub;
pub mod jita;

use std::collections::HashSet;

use chrono::{DateTime, Datelike, Days, NaiveDate, Utc};
use sqlx::PgPool;

use eve_core::AppResult;
use crate::esi::market::MarketOrder;

#[derive(Debug, Clone)]
pub struct RunSummary {
    pub source: &'static str,
    pub location_id: Option<i64>,
    pub orders_seen: u64,
    pub orders_kept: u64,
    pub duration_ms: u64,
}

pub(crate) async fn tracked_types(pool: &PgPool) -> AppResult<HashSet<i64>> {
    let rows: Vec<i64> = sqlx::query_scalar("SELECT type_id FROM tracked_types")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().collect())
}

pub(crate) fn filter_to_tracked(
    orders: Vec<MarketOrder>,
    tracked: &HashSet<i64>,
) -> Vec<MarketOrder> {
    orders
        .into_iter()
        .filter(|o| tracked.contains(&o.type_id))
        .collect()
}

/// Ensure a weekly partition exists for `target` and for the following week.
/// Idempotent: PARTITION OF ... IF NOT EXISTS skips when already created.
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

/// Drop weekly partitions whose end is older than `cutoff`.
pub async fn drop_old_partitions(pool: &PgPool, cutoff: DateTime<Utc>) -> AppResult<u64> {
    // Names look like market_orders_snapshots_YYYYMMDD; parse the date and
    // drop if start + 7d <= cutoff.
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

fn week_start(t: DateTime<Utc>) -> NaiveDate {
    let date = t.date_naive();
    // Anchor weeks on Monday.
    let weekday = date.weekday().num_days_from_monday() as i64;
    date - chrono::Duration::days(weekday)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn week_start_anchors_to_monday() {
        // Wed 2026-05-13 → Mon 2026-05-11
        let wed = Utc.with_ymd_and_hms(2026, 5, 13, 9, 0, 0).unwrap();
        assert_eq!(
            week_start(wed),
            NaiveDate::from_ymd_opt(2026, 5, 11).unwrap()
        );
        // Mon 2026-05-11 stays put
        let mon = Utc.with_ymd_and_hms(2026, 5, 11, 9, 0, 0).unwrap();
        assert_eq!(
            week_start(mon),
            NaiveDate::from_ymd_opt(2026, 5, 11).unwrap()
        );
    }

    #[test]
    fn filter_to_tracked_drops_others() {
        let tracked: HashSet<i64> = [34_i64, 35].into_iter().collect();
        let orders = vec![order(1, 34), order(2, 35), order(3, 99), order(4, 34)];
        let kept = filter_to_tracked(orders, &tracked);
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
