//! `intel report heatmap` — weekday × hour-of-day TSV for the whole channel.
//! Cell = average number of systems simultaneously dirty during that
//! bucket, computed as `sum(dirty-minutes across systems) / observed-minutes`.
//!
//! "Is anything dirty right now?" saturates fast in an active region
//! (almost always yes), so the more useful safety signal is *intensity* —
//! how many threats are typically active at once during this hour-of-day.

use std::io::Write;

use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;

use eve_core::{AppError, AppResult};

use super::iter_hour_chunks;

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

pub async fn run(pool: &SqlitePool, channel: &str, weeks: u32) -> AppResult<()> {
    let cutoff = Utc::now() - Duration::weeks(weeks as i64);
    let mut obs = [[0.0_f64; 24]; 7];
    let mut dirty = [[0.0_f64; 24]; 7];

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
        for (wd, h, m) in iter_hour_chunks(s, e) {
            obs[wd as usize][h as usize] += m;
        }
    }

    let intervals: Vec<(String, String)> = sqlx::query_as(
        "SELECT started_at, ended_at FROM dirty_intervals \
          WHERE channel = ? AND ended_at >= ?",
    )
    .bind(channel)
    .bind(cutoff.to_rfc3339())
    .fetch_all(pool)
    .await?;
    for (s, e) in intervals {
        let (s, e) = clip(parse(&s)?, parse(&e)?, cutoff);
        for (wd, h, m) in iter_hour_chunks(s, e) {
            dirty[wd as usize][h as usize] += m;
        }
    }

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    write!(out, "weekday")?;
    for h in 0..24 {
        write!(out, "\t{:02}", h)?;
    }
    writeln!(out)?;
    for wd in 0..7 {
        write!(out, "{}", WEEKDAYS[wd])?;
        for h in 0..24 {
            let o = obs[wd][h];
            if o <= 0.0 {
                write!(out, "\t")?;
            } else {
                let avg = dirty[wd][h] / o;
                write!(out, "\t{:.2}", avg)?;
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
