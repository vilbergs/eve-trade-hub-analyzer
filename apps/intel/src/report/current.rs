//! `intel current` + `intel channels` — short human-readable status output
//! using `comfy-table`.

use chrono::{DateTime, Duration, Utc};
use comfy_table::{Cell, Table};
use sqlx::SqlitePool;

use eve_core::{AppError, AppResult};

pub async fn run(pool: &SqlitePool, channel: Option<&str>) -> AppResult<()> {
    let timeout = Duration::minutes(
        std::env::var("INTEL_DIRTY_TIMEOUT_MIN")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(20),
    );
    let now = Utc::now();
    let lookback = now - timeout;

    let rows: Vec<(String, String, String, String)> = if let Some(c) = channel {
        sqlx::query_as(
            "SELECT di.channel, ss.name, di.started_at, di.ended_at \
               FROM dirty_intervals di \
               JOIN solar_systems ss ON ss.system_id = di.system_id \
              WHERE di.channel = ? AND di.ended_at >= ? \
              ORDER BY di.ended_at DESC",
        )
        .bind(c)
        .bind(lookback.to_rfc3339())
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as(
            "SELECT di.channel, ss.name, di.started_at, di.ended_at \
               FROM dirty_intervals di \
               JOIN solar_systems ss ON ss.system_id = di.system_id \
              WHERE di.ended_at >= ? \
              ORDER BY di.ended_at DESC",
        )
        .bind(lookback.to_rfc3339())
        .fetch_all(pool)
        .await?
    };

    let mut table = Table::new();
    table.set_header(vec!["channel", "system", "started", "last activity"]);
    for (ch, sys, started, ended) in rows {
        let started = parse(&started)?;
        let ended = parse(&ended)?;
        table.add_row(vec![
            Cell::new(ch),
            Cell::new(sys),
            Cell::new(ago(now, started)),
            Cell::new(ago(now, ended)),
        ]);
    }
    if table.row_count() == 0 {
        println!("No active threats (no dirty intervals in the last {} minutes).", timeout.num_minutes());
    } else {
        println!("{table}");
    }
    Ok(())
}

pub async fn list_channels(pool: &SqlitePool) -> AppResult<()> {
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT name, filename_prefix, enabled FROM channels ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    let mut table = Table::new();
    table.set_header(vec!["name", "filename prefix", "enabled", "regions"]);
    for (name, prefix, enabled) in rows {
        let regions: Vec<(i64, String)> = sqlx::query_as(
            "SELECT cr.region_id, COALESCE(r.name, '?') FROM channel_regions cr \
              LEFT JOIN regions r ON r.region_id = cr.region_id \
              WHERE cr.channel_name = ? ORDER BY r.name",
        )
        .bind(&name)
        .fetch_all(pool)
        .await?;
        let region_names = regions
            .iter()
            .map(|(_, n)| n.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        table.add_row(vec![
            Cell::new(name),
            Cell::new(prefix),
            Cell::new(if enabled != 0 { "yes" } else { "no" }),
            Cell::new(region_names),
        ]);
    }
    println!("{table}");
    Ok(())
}

fn parse(s: &str) -> AppResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(s)
        .map_err(|e| AppError::Other(format!("bad timestamp {s}: {e}")))?
        .with_timezone(&Utc))
}

fn ago(now: DateTime<Utc>, t: DateTime<Utc>) -> String {
    let d = now - t;
    let m = d.num_minutes();
    if m < 1 {
        "just now".into()
    } else if m < 60 {
        format!("{m}m ago")
    } else if m < 60 * 24 {
        format!("{}h {}m ago", m / 60, m % 60)
    } else {
        format!("{}d ago", m / (60 * 24))
    }
}
