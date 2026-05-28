//! `intel report safety` — system × hour-of-day TSV. Cell = % of observed
//! minutes that the system was reported dirty.

use std::collections::BTreeMap;
use std::io::Write;

use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;

use eve_core::{AppError, AppResult};

use super::iter_hour_chunks;

pub async fn run(pool: &SqlitePool, channel: &str, weeks: u32) -> AppResult<()> {
    let cutoff = Utc::now() - Duration::weeks(weeks as i64);

    // 1. Observed minutes per hour-of-day.
    let mut obs_by_hour = [0.0_f64; 24];
    let windows: Vec<(String, String)> = sqlx::query_as(
        "SELECT started_at, ended_at FROM observation_windows \
          WHERE channel = ? AND ended_at >= ?",
    )
    .bind(channel)
    .bind(cutoff.to_rfc3339())
    .fetch_all(pool)
    .await?;
    for (s, e) in windows {
        let (s, e) = clip(parse(&s)?, parse(&e)?, cutoff);
        for (_wd, h, m) in iter_hour_chunks(s, e) {
            obs_by_hour[h as usize] += m;
        }
    }

    // 2. Dirty minutes per (system, hour-of-day).
    let dirty: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT di.system_id, ss.name, di.started_at, di.ended_at \
           FROM dirty_intervals di \
           JOIN solar_systems ss ON ss.system_id = di.system_id \
          WHERE di.channel = ? AND di.ended_at >= ? \
          ORDER BY ss.name",
    )
    .bind(channel)
    .bind(cutoff.to_rfc3339())
    .fetch_all(pool)
    .await?;

    let mut by_system: BTreeMap<(String, i64), [f64; 24]> = BTreeMap::new();
    for (sys_id, sys_name, s, e) in dirty {
        let (s, e) = clip(parse(&s)?, parse(&e)?, cutoff);
        let buckets = by_system.entry((sys_name, sys_id)).or_insert([0.0; 24]);
        for (_wd, h, m) in iter_hour_chunks(s, e) {
            buckets[h as usize] += m;
        }
    }

    // 3. Render TSV.
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    write!(out, "system")?;
    for h in 0..24 {
        write!(out, "\t{:02}", h)?;
    }
    writeln!(out)?;

    // Observed-minutes row so the user can see denominator coverage.
    write!(out, "(observed h)")?;
    for h in 0..24 {
        write!(out, "\t{:.1}", obs_by_hour[h] / 60.0)?;
    }
    writeln!(out)?;

    for ((sys_name, _), buckets) in &by_system {
        write!(out, "{}", sys_name)?;
        for h in 0..24 {
            let obs = obs_by_hour[h];
            if obs <= 0.0 {
                write!(out, "\t")?;
            } else {
                let pct = (buckets[h] / obs) * 100.0;
                if pct < 0.05 {
                    write!(out, "\t")?;
                } else {
                    write!(out, "\t{:.1}", pct)?;
                }
            }
        }
        writeln!(out)?;
    }

    Ok(())
}

fn parse(s: &str) -> AppResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(s)
        .map_err(|e| AppError::Other(format!("bad timestamp {s}: {e}")))?
        .with_timezone(&Utc))
}

fn clip(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    cutoff: DateTime<Utc>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    (start.max(cutoff), end)
}
