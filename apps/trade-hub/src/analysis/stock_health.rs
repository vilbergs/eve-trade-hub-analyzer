//! Stock-health report.
//!
//! One row per (`tracked_stations.station_id`, `tracked_types.type_id`)
//! pair (optionally filtered to a single station). Reports lowest_sell,
//! highest_buy, the 5th-percentile volume-weighted sell + buy (per
//! ADDENDUM.md §3), usable sell-depth (units within 5% of lowest), the
//! oldest sell-order age, 30d consumption, and days-of-supply.

use serde::Serialize;
use sqlx::PgPool;

use super::output::{Renderable, fmt_int, fmt_money, fmt_opt_f64, fmt_opt_int, fmt_pct};
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StockHealthRow {
    pub type_id: i64,
    pub type_name: String,
    pub station_id: i64,
    pub station_name: Option<String>,
    pub is_stocked: bool,
    pub lowest_sell: Option<f64>,
    pub highest_buy: Option<f64>,
    pub p5_sell: Option<f64>,
    pub p5_buy: Option<f64>,
    pub spread_pct: Option<f64>,
    pub usable_sell_depth_units: i64,
    pub usable_sell_depth_isk: f64,
    pub oldest_sell_age_days: Option<i64>,
    pub consumption_30d_units: i64,
    pub days_of_supply: Option<f64>,
}

impl Renderable for StockHealthRow {
    fn headers() -> Vec<&'static str> {
        vec![
            "type_id",
            "type_name",
            "station_id",
            "station_name",
            "stocked",
            "lowest_sell",
            "highest_buy",
            "p5_sell",
            "p5_buy",
            "spread",
            "usable_depth_units",
            "usable_depth_isk",
            "oldest_sell_age_days",
            "consumption_30d",
            "days_of_supply",
        ]
    }

    fn cells(&self) -> Vec<String> {
        vec![
            self.type_id.to_string(),
            self.type_name.clone(),
            self.station_id.to_string(),
            self.station_name.clone().unwrap_or_default(),
            if self.is_stocked { "yes" } else { "no" }.into(),
            fmt_money(self.lowest_sell),
            fmt_money(self.highest_buy),
            fmt_money(self.p5_sell),
            fmt_money(self.p5_buy),
            fmt_pct(self.spread_pct),
            fmt_int(self.usable_sell_depth_units),
            format!("{:.2}", self.usable_sell_depth_isk),
            fmt_opt_int(self.oldest_sell_age_days),
            fmt_int(self.consumption_30d_units),
            fmt_opt_f64(self.days_of_supply, 1),
        ]
    }
}

pub async fn run(
    pool: &PgPool,
    station: Option<i64>,
    limit: i64,
) -> AppResult<Vec<StockHealthRow>> {
    let rows: Vec<StockHealthRow> = sqlx::query_as(STOCK_HEALTH_SQL)
        .bind(station)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

const STOCK_HEALTH_SQL: &str = r#"
WITH
sell_orders AS (
    SELECT order_id, location_id, type_id, price, volume_remain, issued
    FROM market_orders_current
    WHERE is_buy = false
      AND ($1::BIGINT IS NULL OR location_id = $1)
      AND location_id IN (SELECT station_id FROM tracked_stations)
),
buy_orders AS (
    SELECT order_id, location_id, type_id, price, volume_remain
    FROM market_orders_current
    WHERE is_buy = true
      AND ($1::BIGINT IS NULL OR location_id = $1)
      AND location_id IN (SELECT station_id FROM tracked_stations)
),
sell_lowest AS (
    SELECT type_id, location_id,
           MIN(price) AS lowest_sell,
           MIN(issued) AS oldest_issued
    FROM sell_orders
    GROUP BY type_id, location_id
),
buy_highest AS (
    SELECT type_id, location_id, MAX(price) AS highest_buy
    FROM buy_orders
    GROUP BY type_id, location_id
),
sell_depth AS (
    SELECT
        s.type_id,
        s.location_id,
        SUM(s.volume_remain)::BIGINT                AS usable_units,
        SUM(s.volume_remain * s.price)              AS usable_isk
    FROM sell_orders s
    JOIN sell_lowest l ON l.type_id = s.type_id AND l.location_id = s.location_id
    WHERE s.price <= l.lowest_sell * 1.05
    GROUP BY s.type_id, s.location_id
),
sell_ranked AS (
    SELECT
        type_id, location_id, price, volume_remain,
        SUM(volume_remain) OVER (
            PARTITION BY type_id, location_id ORDER BY price ASC
            ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
        ) AS cum_vol,
        SUM(volume_remain) OVER (PARTITION BY type_id, location_id) AS total_vol
    FROM sell_orders
),
sell_p5 AS (
    SELECT type_id, location_id, MIN(price) AS p5_sell
    FROM sell_ranked
    WHERE total_vol > 0 AND cum_vol >= total_vol * 0.05
    GROUP BY type_id, location_id
),
buy_ranked AS (
    SELECT
        type_id, location_id, price, volume_remain,
        SUM(volume_remain) OVER (
            PARTITION BY type_id, location_id ORDER BY price DESC
            ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
        ) AS cum_vol,
        SUM(volume_remain) OVER (PARTITION BY type_id, location_id) AS total_vol
    FROM buy_orders
),
buy_p5 AS (
    SELECT type_id, location_id, MAX(price) AS p5_buy
    FROM buy_ranked
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
    COALESCE(st.name, '?')                                                AS type_name,
    ts.station_id,
    ts.name                                                               AS station_name,
    (sl.lowest_sell IS NOT NULL)                                          AS is_stocked,
    sl.lowest_sell                                                        AS lowest_sell,
    bh.highest_buy                                                        AS highest_buy,
    sp.p5_sell                                                            AS p5_sell,
    bp.p5_buy                                                             AS p5_buy,
    CASE
        WHEN bh.highest_buy IS NOT NULL AND bh.highest_buy > 0 AND sl.lowest_sell IS NOT NULL
        THEN (sl.lowest_sell - bh.highest_buy) / bh.highest_buy
        ELSE NULL
    END                                                                    AS spread_pct,
    COALESCE(sd.usable_units, 0)                                          AS usable_sell_depth_units,
    COALESCE(sd.usable_isk, 0)                                            AS usable_sell_depth_isk,
    CASE WHEN sl.oldest_issued IS NOT NULL
         THEN EXTRACT(DAY FROM now() - sl.oldest_issued)::BIGINT
         ELSE NULL END                                                    AS oldest_sell_age_days,
    COALESCE(c.units_30d, 0)                                              AS consumption_30d_units,
    CASE WHEN COALESCE(c.units_30d, 0) > 0
         THEN COALESCE(sd.usable_units, 0)::DOUBLE PRECISION / (c.units_30d::DOUBLE PRECISION / 30.0)
         ELSE NULL END                                                    AS days_of_supply
FROM tracked_types tt
CROSS JOIN tracked_stations ts
LEFT JOIN sde_types       st ON st.type_id = tt.type_id
LEFT JOIN sell_lowest     sl ON sl.type_id = tt.type_id AND sl.location_id = ts.station_id
LEFT JOIN buy_highest     bh ON bh.type_id = tt.type_id AND bh.location_id = ts.station_id
LEFT JOIN sell_p5         sp ON sp.type_id = tt.type_id AND sp.location_id = ts.station_id
LEFT JOIN buy_p5          bp ON bp.type_id = tt.type_id AND bp.location_id = ts.station_id
LEFT JOIN sell_depth      sd ON sd.type_id = tt.type_id AND sd.location_id = ts.station_id
LEFT JOIN consumption      c ON c.type_id  = tt.type_id AND c.location_id  = ts.station_id
WHERE $1::BIGINT IS NULL OR ts.station_id = $1
ORDER BY
    -- Missing-but-consumed first, then ascending days_of_supply.
    (sl.lowest_sell IS NULL AND COALESCE(c.units_30d, 0) > 0) DESC,
    days_of_supply ASC NULLS LAST,
    type_name ASC
LIMIT $2
"#;
