//! Price-basis lookups over the market tables.
//!
//! Pure SELECT queries against `market_orders_current` and
//! `market_orders_daily`. No HTTP calls, no migrations of its own —
//! reads the schema that `eve-market` wrote.
//!
//! # Usage
//!
//! ```ignore
//! use eve_pricing::{PriceBasis, prices_for};
//!
//! let basis = PriceBasis::SellMin { location_id: 60003760 };
//! let prices = prices_for(&pool, &type_ids, &basis).await?;
//! // prices: HashMap<i64, f64>
//! ```

use std::collections::HashMap;

use eve_core::AppResult;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// How to determine a type's price.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PriceBasis {
    /// Lowest sell order at a specific station/structure.
    SellMin { location_id: i64 },
    /// Highest buy order at a specific station/structure.
    BuyMax { location_id: i64 },
    /// Lowest sell order anywhere in a region.
    RegionSellMin { region_id: i64 },
    /// Highest buy order anywhere in a region.
    RegionBuyMax { region_id: i64 },
    /// 5th-percentile volume-weighted sell at a location (same logic as
    /// the stock-health report's `p5_sell`).
    SellP5 { location_id: i64 },
    /// Average of `close_lowest_sell` from `market_orders_daily` over
    /// the last N days at a location.
    DailyAvg { location_id: i64, days: i32 },
}

/// Look up the price for a single type_id.
///
/// Returns `None` if no matching orders or data exist.
pub async fn price_for(pool: &PgPool, type_id: i64, basis: &PriceBasis) -> AppResult<Option<f64>> {
    let map = prices_for(pool, &[type_id], basis).await?;
    Ok(map.get(&type_id).copied())
}

/// Batch price lookup. Returns a map from type_id → price. Type_ids with
/// no available price are omitted from the map.
pub async fn prices_for(
    pool: &PgPool,
    type_ids: &[i64],
    basis: &PriceBasis,
) -> AppResult<HashMap<i64, f64>> {
    if type_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows: Vec<(i64, f64)> = match basis {
        PriceBasis::SellMin { location_id } => {
            sqlx::query_as(
                r#"
                SELECT type_id, MIN(price) AS price
                FROM market_orders_current
                WHERE type_id = ANY($1)
                  AND location_id = $2
                  AND is_buy = false
                GROUP BY type_id
                "#,
            )
            .bind(type_ids)
            .bind(location_id)
            .fetch_all(pool)
            .await?
        }

        PriceBasis::BuyMax { location_id } => {
            sqlx::query_as(
                r#"
                SELECT type_id, MAX(price) AS price
                FROM market_orders_current
                WHERE type_id = ANY($1)
                  AND location_id = $2
                  AND is_buy = true
                GROUP BY type_id
                "#,
            )
            .bind(type_ids)
            .bind(location_id)
            .fetch_all(pool)
            .await?
        }

        PriceBasis::RegionSellMin { region_id } => {
            sqlx::query_as(
                r#"
                SELECT type_id, MIN(price) AS price
                FROM market_orders_current
                WHERE type_id = ANY($1)
                  AND region_id = $2
                  AND is_buy = false
                GROUP BY type_id
                "#,
            )
            .bind(type_ids)
            .bind(region_id)
            .fetch_all(pool)
            .await?
        }

        PriceBasis::RegionBuyMax { region_id } => {
            sqlx::query_as(
                r#"
                SELECT type_id, MAX(price) AS price
                FROM market_orders_current
                WHERE type_id = ANY($1)
                  AND region_id = $2
                  AND is_buy = true
                GROUP BY type_id
                "#,
            )
            .bind(type_ids)
            .bind(region_id)
            .fetch_all(pool)
            .await?
        }

        PriceBasis::SellP5 { location_id } => {
            // 5th-percentile: the price at which 5% of total sell volume
            // has been offered or cheaper.
            sqlx::query_as(
                r#"
                WITH ranked AS (
                    SELECT
                        type_id, price,
                        SUM(volume_remain) OVER (
                            PARTITION BY type_id ORDER BY price ASC
                            ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
                        ) AS cum_vol,
                        SUM(volume_remain) OVER (PARTITION BY type_id) AS total_vol
                    FROM market_orders_current
                    WHERE type_id = ANY($1)
                      AND location_id = $2
                      AND is_buy = false
                )
                SELECT type_id, MIN(price) AS price
                FROM ranked
                WHERE total_vol > 0 AND cum_vol >= total_vol * 0.05
                GROUP BY type_id
                "#,
            )
            .bind(type_ids)
            .bind(location_id)
            .fetch_all(pool)
            .await?
        }

        PriceBasis::DailyAvg { location_id, days } => {
            sqlx::query_as(
                r#"
                SELECT type_id, AVG(close_lowest_sell) AS price
                FROM market_orders_daily
                WHERE type_id = ANY($1)
                  AND location_id = $2
                  AND day >= (CURRENT_DATE - $3::INT)
                  AND close_lowest_sell IS NOT NULL
                GROUP BY type_id
                "#,
            )
            .bind(type_ids)
            .bind(location_id)
            .bind(days)
            .fetch_all(pool)
            .await?
        }
    };

    Ok(rows.into_iter().collect())
}

/// Convenience: price a BOM (list of type_id + quantity) and return the
/// total ISK cost. Items without a price are skipped (their cost is 0).
pub async fn price_bom(
    pool: &PgPool,
    items: &[(i64, i64)],
    basis: &PriceBasis,
) -> AppResult<f64> {
    let type_ids: Vec<i64> = items.iter().map(|(t, _)| *t).collect();
    let prices = prices_for(pool, &type_ids, basis).await?;
    let total = items
        .iter()
        .map(|(type_id, qty)| prices.get(type_id).unwrap_or(&0.0) * *qty as f64)
        .sum();
    Ok(total)
}
