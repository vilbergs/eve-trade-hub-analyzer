pub mod current;
pub mod heatmap;
pub mod pilots;
pub mod safety;
pub mod systems;

use chrono::{DateTime, Datelike, Duration, Timelike, Utc};

/// Yield `(weekday, hour, minutes)` chunks for an interval, splitting at
/// each hour boundary. `weekday` is 0=Mon … 6=Sun (matches chrono's
/// `num_days_from_monday`). All times are UTC.
pub fn iter_hour_chunks(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> impl Iterator<Item = (u32, u32, f64)> {
    HourChunks { cur: start, end }
}

struct HourChunks {
    cur: DateTime<Utc>,
    end: DateTime<Utc>,
}

impl Iterator for HourChunks {
    type Item = (u32, u32, f64);
    fn next(&mut self) -> Option<Self::Item> {
        if self.cur >= self.end {
            return None;
        }
        let weekday = self.cur.weekday().num_days_from_monday();
        let hour = self.cur.hour();
        let next_hour = (self.cur + Duration::hours(1))
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap();
        let chunk_end = next_hour.min(self.end);
        let mins = (chunk_end - self.cur).num_milliseconds() as f64 / 60_000.0;
        self.cur = chunk_end;
        Some((weekday, hour, mins))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn splits_at_hour_boundary() {
        let start = Utc.with_ymd_and_hms(2025, 8, 11, 17, 55, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2025, 8, 11, 18, 10, 0).unwrap();
        let chunks: Vec<_> = iter_hour_chunks(start, end).collect();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].1, 17);
        assert!((chunks[0].2 - 5.0).abs() < 1e-6);
        assert_eq!(chunks[1].1, 18);
        assert!((chunks[1].2 - 10.0).abs() < 1e-6);
    }

    #[test]
    fn single_hour() {
        let start = Utc.with_ymd_and_hms(2025, 8, 11, 17, 23, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2025, 8, 11, 17, 45, 0).unwrap();
        let chunks: Vec<_> = iter_hour_chunks(start, end).collect();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].1, 17);
        assert!((chunks[0].2 - 22.0).abs() < 1e-6);
    }
}
