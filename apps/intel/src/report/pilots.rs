//! `intel report pilots` — pilot rap sheet across all sightings for the channel.

use std::collections::BTreeMap;
use std::io::Write;

use sqlx::SqlitePool;

use eve_core::AppResult;

pub async fn run(pool: &SqlitePool, channel: &str, top: u32) -> AppResult<()> {
    let rows: Vec<(String, String, Option<i64>)> = sqlx::query_as(
        "SELECT ts, pilots_json, system_id FROM sightings WHERE channel = ?",
    )
    .bind(channel)
    .fetch_all(pool)
    .await?;

    #[derive(Default)]
    struct Entry {
        count: u64,
        systems: std::collections::BTreeSet<i64>,
        last_seen: String,
    }
    let mut agg: BTreeMap<String, Entry> = BTreeMap::new();
    for (ts, pilots_json, sys) in rows {
        let pilots: Vec<String> = serde_json::from_str(&pilots_json).unwrap_or_default();
        for pilot in pilots {
            if pilot.is_empty() {
                continue;
            }
            let e = agg.entry(pilot).or_default();
            e.count += 1;
            if let Some(sid) = sys {
                e.systems.insert(sid);
            }
            if ts > e.last_seen {
                e.last_seen = ts.clone();
            }
        }
    }

    let mut entries: Vec<(String, Entry)> = agg.into_iter().collect();
    entries.sort_by(|a, b| b.1.count.cmp(&a.1.count));
    entries.truncate(top as usize);

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "pilot\tsightings\tdistinct_systems\tlast_seen")?;
    for (name, e) in entries {
        writeln!(
            out,
            "{}\t{}\t{}\t{}",
            name,
            e.count,
            e.systems.len(),
            e.last_seen
        )?;
    }
    Ok(())
}
