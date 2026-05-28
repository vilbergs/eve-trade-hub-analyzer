//! `intel report systems` — per-system sighting/dirty-minute totals.

use std::io::Write;

use chrono::{Duration, Utc};
use sqlx::SqlitePool;

use eve_core::AppResult;

pub async fn run(pool: &SqlitePool, channel: &str, weeks: u32) -> AppResult<()> {
    let cutoff = (Utc::now() - Duration::weeks(weeks as i64)).to_rfc3339();

    let rows: Vec<(String, i64, i64, f64)> = sqlx::query_as(
        "WITH inv AS (\
            SELECT system_id, COUNT(*) AS intervals, \
                   SUM((julianday(ended_at) - julianday(started_at)) * 1440.0) AS dirty_min \
              FROM dirty_intervals \
             WHERE channel = ? AND ended_at >= ? \
             GROUP BY system_id\
         ), sgt AS (\
            SELECT system_id, COUNT(*) AS sightings \
              FROM sightings \
             WHERE channel = ? AND ts >= ? AND system_id IS NOT NULL \
             GROUP BY system_id\
         ) \
         SELECT ss.name, COALESCE(sgt.sightings, 0), inv.intervals, inv.dirty_min \
           FROM inv \
           JOIN solar_systems ss ON ss.system_id = inv.system_id \
           LEFT JOIN sgt ON sgt.system_id = inv.system_id \
          WHERE inv.dirty_min > 0 \
          ORDER BY inv.dirty_min DESC",
    )
    .bind(channel)
    .bind(&cutoff)
    .bind(channel)
    .bind(&cutoff)
    .fetch_all(pool)
    .await?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "system\tsightings\tintervals\tdirty_hours")?;
    for (name, sightings, intervals, dirty_min) in rows {
        writeln!(
            out,
            "{}\t{}\t{}\t{:.1}",
            name,
            sightings,
            intervals,
            dirty_min / 60.0
        )?;
    }
    Ok(())
}
