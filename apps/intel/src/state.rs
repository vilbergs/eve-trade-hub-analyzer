//! Build `dirty_intervals` from `sightings` + `observation_windows`.
//!
//! Algorithm: per session (one observation window), per system, walk
//! sightings in time order with a tiny state machine:
//!
//! - Non-clear sighting → opens an interval if none open; resets the
//!   "last activity" timer.
//! - Clear sighting → closes the open interval at the sighting timestamp
//!   (`ended_by = 'clear'`).
//! - If the next event is more than `DIRTY_TIMEOUT_MIN` minutes after the
//!   last activity → close the open interval at `last_activity + timeout`
//!   (`ended_by = 'timeout'`), then continue.
//! - End of session → close any still-open interval at
//!   `min(last_activity + timeout, window.ended_at)`
//!   (`ended_by = 'session-end'`).

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;

use eve_core::AppResult;

pub async fn rebuild_channel(
    pool: &SqlitePool,
    channel: &str,
    timeout_min: i64,
) -> AppResult<()> {
    let timeout = Duration::minutes(timeout_min);

    let windows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT source_file, started_at, ended_at FROM observation_windows \
          WHERE channel = ? ORDER BY started_at",
    )
    .bind(channel)
    .fetch_all(pool)
    .await?;

    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM dirty_intervals WHERE channel = ?")
        .bind(channel)
        .execute(&mut *tx)
        .await?;

    let mut total_intervals: usize = 0;

    for (source_file, started, ended) in &windows {
        let win_start = parse(started)?;
        let win_end = parse(ended)?;

        let sightings: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT ts, system_id, is_clear FROM sightings \
              WHERE channel = ? AND source_file = ? AND system_id IS NOT NULL \
              ORDER BY ts, id",
        )
        .bind(channel)
        .bind(source_file)
        .fetch_all(pool)
        .await?;

        // Bucket sightings by system, preserving ts order.
        let mut by_system: BTreeMap<i64, Vec<(DateTime<Utc>, bool)>> = BTreeMap::new();
        for (ts, sys, clr) in sightings {
            let ts = parse(&ts)?;
            by_system.entry(sys).or_default().push((ts, clr != 0));
        }

        for (system_id, events) in by_system {
            let intervals = build_intervals(&events, win_start, win_end, timeout);
            for iv in intervals {
                sqlx::query(
                    "INSERT INTO dirty_intervals (channel, system_id, started_at, ended_at, ended_by) \
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(channel)
                .bind(system_id)
                .bind(iv.started_at.to_rfc3339())
                .bind(iv.ended_at.to_rfc3339())
                .bind(iv.ended_by)
                .execute(&mut *tx)
                .await?;
                total_intervals += 1;
            }
        }
    }

    tx.commit().await?;
    tracing::info!(channel = %channel, sessions = windows.len(), intervals = total_intervals, "rebuilt dirty_intervals");
    Ok(())
}

fn parse(s: &str) -> AppResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(s)
        .map_err(|e| eve_core::AppError::Other(format!("bad timestamp {s}: {e}")))?
        .with_timezone(&Utc))
}

#[derive(Debug, Clone, PartialEq)]
struct Interval {
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    ended_by: &'static str,
}

fn build_intervals(
    events: &[(DateTime<Utc>, bool)], // (ts, is_clear), ordered ascending
    win_start: DateTime<Utc>,
    win_end: DateTime<Utc>,
    timeout: Duration,
) -> Vec<Interval> {
    let mut out = Vec::new();
    let mut open: Option<DateTime<Utc>> = None;
    let mut last_activity: Option<DateTime<Utc>> = None;

    for &(ts, is_clear) in events {
        // Discard events outside the observation window (safety net; usually
        // every sighting is within its session).
        if ts < win_start || ts > win_end {
            continue;
        }
        // Check for timeout before consuming this event.
        if let (Some(start), Some(la)) = (open, last_activity) {
            let expires = la + timeout;
            if ts > expires {
                out.push(Interval {
                    started_at: start,
                    ended_at: expires,
                    ended_by: "timeout",
                });
                open = None;
                last_activity = None;
            }
        }
        if is_clear {
            if let Some(start) = open {
                out.push(Interval {
                    started_at: start,
                    ended_at: ts,
                    ended_by: "clear",
                });
                open = None;
                last_activity = None;
            }
        } else {
            if open.is_none() {
                open = Some(ts);
            }
            last_activity = Some(ts);
        }
    }

    // End-of-session close-out.
    if let (Some(start), Some(la)) = (open, last_activity) {
        let timeout_end = la + timeout;
        let session_end = win_end;
        let (end, kind) = if timeout_end <= session_end {
            (timeout_end, "timeout")
        } else {
            (session_end, "session-end")
        };
        out.push(Interval {
            started_at: start,
            ended_at: end,
            ended_by: kind,
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn dt(min: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 8, 11, 17, 0, 0).unwrap() + Duration::minutes(min)
    }

    #[test]
    fn opens_then_clears() {
        let events = vec![(dt(0), false), (dt(5), true)];
        let intervals = build_intervals(&events, dt(-10), dt(120), Duration::minutes(20));
        assert_eq!(intervals.len(), 1);
        assert_eq!(intervals[0].started_at, dt(0));
        assert_eq!(intervals[0].ended_at, dt(5));
        assert_eq!(intervals[0].ended_by, "clear");
    }

    #[test]
    fn opens_then_times_out_between_events() {
        let events = vec![(dt(0), false), (dt(40), false)];
        let intervals = build_intervals(&events, dt(-10), dt(120), Duration::minutes(20));
        // First sighting opens interval, no further activity for 40 min → timeout
        // at 0 + 20. Second sighting opens a new interval at 40, then closed
        // at session end (no further events) → session-end at min(60, 120) = 60.
        assert_eq!(intervals.len(), 2);
        assert_eq!(intervals[0].ended_by, "timeout");
        assert_eq!(intervals[0].ended_at, dt(20));
        assert_eq!(intervals[1].started_at, dt(40));
        assert_eq!(intervals[1].ended_by, "timeout");
        assert_eq!(intervals[1].ended_at, dt(60));
    }

    #[test]
    fn session_end_closes_open_interval() {
        let events = vec![(dt(115), false)];
        let intervals = build_intervals(&events, dt(0), dt(120), Duration::minutes(20));
        assert_eq!(intervals.len(), 1);
        // timeout would be 115+20=135, beyond session_end=120, so session-end wins.
        assert_eq!(intervals[0].ended_at, dt(120));
        assert_eq!(intervals[0].ended_by, "session-end");
    }

    #[test]
    fn repeated_sightings_extend_activity() {
        // 0, 10, 20 (each within 20-min window) → single interval until last+20.
        let events = vec![(dt(0), false), (dt(10), false), (dt(20), false)];
        let intervals = build_intervals(&events, dt(-5), dt(120), Duration::minutes(20));
        assert_eq!(intervals.len(), 1);
        assert_eq!(intervals[0].started_at, dt(0));
        assert_eq!(intervals[0].ended_at, dt(40));
        assert_eq!(intervals[0].ended_by, "timeout");
    }
}
