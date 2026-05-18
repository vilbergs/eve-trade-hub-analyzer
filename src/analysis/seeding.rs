//! Seeding-opportunity report.
//!
//! One row per (tracked_stations.station_id, tracked_types.type_id) pair
//! where the hub's 5th-percentile sell exceeds Jita's by a positive
//! margin and there is non-zero recent consumption. Sort: descending
//! expected_isk_per_day. ADDENDUM.md §3 removes haul costs and uses
//! gross margin: `(hub_p5_sell - jita_p5_sell) * (consumption_30d / 30)`.

use serde::Serialize;
use sqlx::PgPool;

use super::output::{Renderable, fmt_int, fmt_money, fmt_opt_f64};
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SeedingRow {
    pub type_id: i64,
    pub type_name: String,
    pub station_id: i64,
    pub station_name: Option<String>,
    pub jita_p5_sell: Option<f64>,
    pub hub_p5_sell: Option<f64>,
    pub gross_margin_per_unit: Option<f64>,
    pub consumption_30d_units: i64,
    pub expected_isk_per_day: Option<f64>,
}

impl Renderable for SeedingRow {
    fn headers() -> Vec<&'static str> {
        vec![
            "type_id",
            "type_name",
            "station_id",
            "station_name",
            "jita_p5_sell",
            "hub_p5_sell",
            "gross_margin_per_unit",
            "consumption_30d",
            "expected_isk_per_day",
        ]
    }

    fn cells(&self) -> Vec<String> {
        vec![
            self.type_id.to_string(),
            self.type_name.clone(),
            self.station_id.to_string(),
            self.station_name.clone().unwrap_or_default(),
            fmt_money(self.jita_p5_sell),
            fmt_money(self.hub_p5_sell),
            fmt_money(self.gross_margin_per_unit),
            fmt_int(self.consumption_30d_units),
            fmt_opt_f64(self.expected_isk_per_day, 0),
        ]
    }
}

pub async fn run(
    pool: &PgPool,
    jita_region_id: i64,
    station: Option<i64>,
    min_profit_per_day: f64,
    limit: i64,
) -> AppResult<Vec<SeedingRow>> {
    let rows: Vec<SeedingRow> = sqlx::query_as(SEEDING_SQL)
        .bind(station)
        .bind(jita_region_id)
        .bind(min_profit_per_day)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

const SEEDING_SQL: &str = r#"
WITH
sell_orders_jita AS (
    SELECT type_id, price, volume_remain
    FROM market_orders_current
    WHERE is_buy = false AND region_id = $2
),
sell_orders_hub AS (
    SELECT type_id, location_id, price, volume_remain
    FROM market_orders_current
    WHERE is_buy = false
      AND location_id IN (SELECT station_id FROM tracked_stations)
      AND ($1::BIGINT IS NULL OR location_id = $1)
),
jita_ranked AS (
    SELECT
        type_id, price, volume_remain,
        SUM(volume_remain) OVER (
            PARTITION BY type_id ORDER BY price ASC
            ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
        ) AS cum_vol,
        SUM(volume_remain) OVER (PARTITION BY type_id) AS total_vol
    FROM sell_orders_jita
),
jita_p5 AS (
    SELECT type_id, MIN(price) AS p5_sell
    FROM jita_ranked
    WHERE total_vol > 0 AND cum_vol >= total_vol * 0.05
    GROUP BY type_id
),
hub_ranked AS (
    SELECT
        type_id, location_id, price, volume_remain,
        SUM(volume_remain) OVER (
            PARTITION BY type_id, location_id ORDER BY price ASC
            ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
        ) AS cum_vol,
        SUM(volume_remain) OVER (PARTITION BY type_id, location_id) AS total_vol
    FROM sell_orders_hub
),
hub_p5 AS (
    SELECT type_id, location_id, MIN(price) AS p5_sell
    FROM hub_ranked
    WHERE total_vol > 0 AND cum_vol >= total_vol * 0.05
    GROUP BY type_id, location_id
),
consumption AS (
    SELECT type_id, location_id, SUM(units_consumed)::BIGINT AS units_30d
    FROM market_daily_agg
    WHERE day >= (now() - INTERVAL '30 days')::DATE
    GROUP BY type_id, location_id
)
SELECT
    tt.type_id,
    COALESCE(st.name, '?')                              AS type_name,
    ts.station_id,
    ts.name                                             AS station_name,
    jp.p5_sell                                          AS jita_p5_sell,
    hp.p5_sell                                          AS hub_p5_sell,
    (hp.p5_sell - jp.p5_sell)                           AS gross_margin_per_unit,
    COALESCE(c.units_30d, 0)                            AS consumption_30d_units,
    (hp.p5_sell - jp.p5_sell) * (c.units_30d::DOUBLE PRECISION / 30.0) AS expected_isk_per_day
FROM tracked_types tt
CROSS JOIN tracked_stations ts
LEFT JOIN sde_types  st ON st.type_id = tt.type_id
LEFT JOIN jita_p5    jp ON jp.type_id = tt.type_id
LEFT JOIN hub_p5     hp ON hp.type_id = tt.type_id AND hp.location_id = ts.station_id
LEFT JOIN consumption c ON c.type_id  = tt.type_id AND c.location_id  = ts.station_id
WHERE ($1::BIGINT IS NULL OR ts.station_id = $1)
  AND jp.p5_sell IS NOT NULL
  AND hp.p5_sell IS NOT NULL
  AND hp.p5_sell > jp.p5_sell
  AND COALESCE(c.units_30d, 0) > 0
  AND (hp.p5_sell - jp.p5_sell) * (c.units_30d::DOUBLE PRECISION / 30.0) >= $3
ORDER BY expected_isk_per_day DESC NULLS LAST
LIMIT $4
"#;
