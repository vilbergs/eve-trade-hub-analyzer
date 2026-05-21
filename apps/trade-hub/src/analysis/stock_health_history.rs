//! Stock-health history report.
//!
//! Time series of days-of-supply (plus lowest_sell and usable depth) for a
//! given type, optionally filtered to a single station.  Data comes from
//! `stock_health_daily`, which the rollup populates once per day.

use serde::Serialize;
use sqlx::PgPool;

use super::output::{Renderable, fmt_int, fmt_money, fmt_opt_f64};
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StockHealthHistoryRow {
    pub day: chrono::NaiveDate,
    pub station_id: i64,
    pub station_name: Option<String>,
    pub type_id: i64,
    pub type_name: String,
    pub lowest_sell: Option<f64>,
    pub usable_depth_units: i64,
    pub days_of_supply: Option<f64>,
}

impl Renderable for StockHealthHistoryRow {
    fn headers() -> Vec<&'static str> {
        vec![
            "day",
            "station_id",
            "station_name",
            "type_id",
            "type_name",
            "lowest_sell",
            "usable_depth_units",
            "days_of_supply",
        ]
    }

    fn cells(&self) -> Vec<String> {
        vec![
            self.day.to_string(),
            self.station_id.to_string(),
            self.station_name.clone().unwrap_or_default(),
            self.type_id.to_string(),
            self.type_name.clone(),
            fmt_money(self.lowest_sell),
            fmt_int(self.usable_depth_units),
            fmt_opt_f64(self.days_of_supply, 1),
        ]
    }
}

pub async fn run(
    pool: &PgPool,
    type_id: i64,
    station: Option<i64>,
    days: i64,
) -> AppResult<Vec<StockHealthHistoryRow>> {
    let rows: Vec<StockHealthHistoryRow> = sqlx::query_as(
        r#"
        SELECT
            shd.day,
            shd.location_id  AS station_id,
            ts.name           AS station_name,
            shd.type_id,
            COALESCE(st.name, '?') AS type_name,
            shd.lowest_sell,
            shd.usable_depth_units,
            shd.days_of_supply
        FROM stock_health_daily shd
        JOIN tracked_stations ts ON ts.station_id = shd.location_id
        LEFT JOIN sde_types st   ON st.type_id = shd.type_id
        WHERE shd.type_id = $1
          AND ($2::BIGINT IS NULL OR shd.location_id = $2)
          AND shd.day >= (now() - ($3::BIGINT || ' days')::INTERVAL)::DATE
        ORDER BY shd.day ASC, shd.location_id
        "#,
    )
    .bind(type_id)
    .bind(station)
    .bind(days)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
