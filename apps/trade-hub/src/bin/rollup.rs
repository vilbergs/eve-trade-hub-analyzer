//! `rollup` binary — fold a day of snapshots into market_daily_agg and
//! stock_health_daily.
//!
//! Usage:
//!   rollup [--day YYYY-MM-DD]          # roll up one day (default: yesterday)
//!   rollup --backfill-stock-health     # backfill stock_health_daily for all
//!                                        days that have snapshot data

use chrono::{Duration, NaiveDate, Utc};
use clap::Parser;
use eve_core::{AppError, AppResult};
use eve_trade_hub_analyzer::{Config, db};
use eve_core::telemetry;
use sqlx::PgPool;
use tracing::info;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// The day to roll up (YYYY-MM-DD). Defaults to yesterday UTC.
    #[arg(long)]
    day: Option<String>,

    /// Back-fill stock_health_daily for every day that has snapshot data.
    #[arg(long, default_value_t = false)]
    backfill_stock_health: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init();
    let args = Args::parse();
    let config = Config::from_env()?;
    let pool = db::build_pool(&config).await?;

    if args.backfill_stock_health {
        backfill_stock_health(&pool).await?;
        return Ok(());
    }

    let day: NaiveDate = match args.day {
        Some(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d")
            .map_err(|e| AppError::Other(format!("--day must be YYYY-MM-DD: {e}")))?,
        None => (Utc::now() - Duration::days(1)).date_naive(),
    };
    info!(%day, "rolling up");

    let aggregated = roll_day(&pool, day).await?;
    info!(%day, rows = aggregated, "market_daily_agg upserted");

    let health_rows = roll_stock_health(&pool, day).await?;
    info!(%day, rows = health_rows, "stock_health_daily upserted");

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

/// Use the last snapshot of `day` to compute close-of-day sell depth and
/// days-of-supply (using rolling 30-day consumption from market_daily_agg),
/// then upsert into stock_health_daily for every tracked (station, type).
async fn roll_stock_health(pool: &PgPool, day: NaiveDate) -> AppResult<u64> {
    let day_start = day
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| AppError::Other("invalid day".into()))?
        .and_utc();
    let day_end = day_start + Duration::days(1);

    let res = sqlx::query(
        r#"
        WITH
        last_ts AS (
            SELECT MAX(snapshot_ts) AS ts
            FROM market_orders_snapshots
            WHERE snapshot_ts >= $1 AND snapshot_ts < $2
        ),
        close_sells AS (
            SELECT location_id, type_id, price, volume_remain
            FROM market_orders_snapshots
            WHERE snapshot_ts = (SELECT ts FROM last_ts)
              AND is_buy = false
        ),
        sell_lowest AS (
            SELECT location_id, type_id, MIN(price) AS lowest_sell
            FROM close_sells
            GROUP BY location_id, type_id
        ),
        sell_depth AS (
            SELECT
                s.location_id,
                s.type_id,
                SUM(s.volume_remain)::BIGINT AS usable_units
            FROM close_sells s
            JOIN sell_lowest l
                ON l.location_id = s.location_id AND l.type_id = s.type_id
            WHERE s.price <= l.lowest_sell * 1.05
            GROUP BY s.location_id, s.type_id
        ),
        consumption_30d AS (
            SELECT location_id, type_id,
                   SUM(units_consumed)::BIGINT AS units_30d
            FROM market_daily_agg
            WHERE day > ($3::DATE - INTERVAL '30 days')::DATE
              AND day <= $3::DATE
            GROUP BY location_id, type_id
        )
        INSERT INTO stock_health_daily
            (day, location_id, type_id, lowest_sell, usable_depth_units, days_of_supply)
        SELECT
            $3::DATE,
            ts.station_id,
            tt.type_id,
            sl.lowest_sell,
            COALESCE(sd.usable_units, 0),
            CASE WHEN COALESCE(c.units_30d, 0) > 0
                 THEN COALESCE(sd.usable_units, 0)::DOUBLE PRECISION
                      / (c.units_30d::DOUBLE PRECISION / 30.0)
                 ELSE NULL
            END
        FROM tracked_types tt
        CROSS JOIN tracked_stations ts
        LEFT JOIN sell_lowest sl
            ON sl.type_id = tt.type_id AND sl.location_id = ts.station_id
        LEFT JOIN sell_depth sd
            ON sd.type_id = tt.type_id AND sd.location_id = ts.station_id
        LEFT JOIN consumption_30d c
            ON c.type_id = tt.type_id AND c.location_id = ts.station_id
        ON CONFLICT (day, location_id, type_id) DO UPDATE SET
            lowest_sell        = EXCLUDED.lowest_sell,
            usable_depth_units = EXCLUDED.usable_depth_units,
            days_of_supply     = EXCLUDED.days_of_supply
        "#,
    )
    .bind(day_start)
    .bind(day_end)
    .bind(day)
    .execute(pool)
    .await?;

    Ok(res.rows_affected())
}

/// Back-fill stock_health_daily for every day that has snapshot data.
/// Runs roll_day first (to ensure market_daily_agg is populated), then
/// roll_stock_health.
async fn backfill_stock_health(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let days: Vec<NaiveDate> = sqlx::query_scalar(
        "SELECT DISTINCT snapshot_ts::DATE AS day \
         FROM market_orders_snapshots \
         ORDER BY day",
    )
    .fetch_all(pool)
    .await?;

    info!(days = days.len(), "backfilling stock_health_daily");

    for day in &days {
        let agg = roll_day(pool, *day).await?;
        let health = roll_stock_health(pool, *day).await?;
        info!(
            %day,
            agg_rows = agg,
            health_rows = health,
            "backfill day complete"
        );
    }

    info!("backfill complete");
    Ok(())
}
