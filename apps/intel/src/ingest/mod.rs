pub mod backfill;
pub mod reader;
pub mod watch;

use std::collections::HashMap;

use sqlx::SqlitePool;

use eve_core::{AppError, AppResult};

use crate::parser::extract::{Lookups, Sighting};

pub struct ChannelCtx {
    pub name: String,
    pub filename_prefix: String,
    pub lookups: Lookups,
}

pub async fn load_enabled_channels(
    pool: &SqlitePool,
    only: Option<&str>,
) -> AppResult<Vec<ChannelCtx>> {
    let rows: Vec<(String, String)> = if let Some(name) = only {
        sqlx::query_as(
            "SELECT name, filename_prefix FROM channels WHERE enabled = 1 AND name = ?",
        )
        .bind(name)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as("SELECT name, filename_prefix FROM channels WHERE enabled = 1")
            .fetch_all(pool)
            .await?
    };

    if rows.is_empty() {
        return Err(AppError::Config(format!(
            "no enabled channels match {:?}",
            only
        )));
    }

    let mut out = Vec::new();
    for (name, prefix) in rows {
        let lookups = load_lookups(pool, &name).await?;
        out.push(ChannelCtx {
            name,
            filename_prefix: prefix,
            lookups,
        });
    }
    Ok(out)
}

async fn load_lookups(pool: &SqlitePool, channel: &str) -> AppResult<Lookups> {
    let systems: Vec<(i64, String)> = sqlx::query_as(
        "SELECT s.system_id, s.name \
           FROM solar_systems s \
           JOIN constellations c ON c.constellation_id = s.constellation_id \
           JOIN channel_regions cr ON cr.region_id = c.region_id \
          WHERE cr.channel_name = ?",
    )
    .bind(channel)
    .fetch_all(pool)
    .await?;

    let ships: Vec<(i64, String)> =
        sqlx::query_as("SELECT type_id, name FROM ship_types").fetch_all(pool).await?;

    let systems_map: HashMap<String, i64> = systems
        .into_iter()
        .map(|(id, name)| (name.to_lowercase(), id))
        .collect();
    let ships_map: HashMap<String, i64> = ships
        .into_iter()
        .map(|(id, name)| (name.to_lowercase(), id))
        .collect();

    Ok(Lookups::new(systems_map, ships_map))
}

/// Insert a batch of sightings inside an open transaction. Idempotent on
/// `(source_file, line_no)` via the UNIQUE constraint — repeated calls
/// for the same file are no-ops.
pub async fn insert_sightings<'c>(
    tx: &mut sqlx::Transaction<'c, sqlx::Sqlite>,
    source_file: &str,
    line_no: i64,
    sightings: &[Sighting],
) -> AppResult<usize> {
    if sightings.is_empty() {
        return Ok(0);
    }
    let mut written = 0;
    for (idx, s) in sightings.iter().enumerate() {
        // When a single line emits N sightings (multi-system), give each a
        // distinct synthetic line_no so the UNIQUE constraint still works:
        // `line_no * 1000 + idx`. line numbers in real files don't approach
        // 1000 within a single line.
        let synthetic = line_no * 1000 + idx as i64;
        let pilots_json = serde_json::to_string(&s.pilots).unwrap_or_else(|_| "[]".into());
        let result = sqlx::query(
            "INSERT OR IGNORE INTO sightings (\
                ts, channel, reporter, system_id, pilots_json, ship_type_id, \
                fleet_count, no_visual, is_clear, parse_confidence, raw_body, \
                source_file, line_no\
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(s.ts.to_rfc3339())
        .bind(&s.channel)
        .bind(&s.reporter)
        .bind(s.system_id)
        .bind(&pilots_json)
        .bind(s.ship_type_id)
        .bind(s.fleet_count.map(|n| n as i64))
        .bind(s.no_visual as i64)
        .bind(s.is_clear as i64)
        .bind(s.parse_confidence as f64)
        .bind(&s.raw_body)
        .bind(source_file)
        .bind(synthetic)
        .execute(&mut **tx)
        .await?;
        written += result.rows_affected() as usize;
    }
    Ok(written)
}
