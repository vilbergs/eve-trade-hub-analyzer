//! `intel backfill` — walk the chatlog dir, parse every matching file
//! into the DB. Idempotent: existing (source_file, line_no) rows are kept,
//! and observation_windows use INSERT OR REPLACE so re-runs refresh the
//! end-time if a session log was appended after a previous backfill.

use std::collections::BTreeMap;
use std::path::Path;

use chrono::NaiveDate;
use sqlx::SqlitePool;

use eve_core::{AppError, AppResult};

use crate::config::Config;
use crate::ingest::{ChannelCtx, insert_sightings, load_enabled_channels, reader};
use crate::parser::{extract, raw};
use crate::state;

pub async fn run(
    pool: &SqlitePool,
    cfg: &Config,
    channel: Option<&str>,
    since: Option<&str>,
) -> AppResult<()> {
    let since = since.map(parse_since).transpose()?;
    let channels = load_enabled_channels(pool, channel).await?;

    // Group files by channel based on filename prefix.
    let mut by_channel: BTreeMap<String, Vec<std::path::PathBuf>> = BTreeMap::new();
    let dir = &cfg.chatlog_dir;
    if !dir.exists() {
        return Err(AppError::Config(format!(
            "chatlog dir does not exist: {}",
            dir.display()
        )));
    }
    for entry in std::fs::read_dir(dir).map_err(AppError::Io)? {
        let entry = entry.map_err(AppError::Io)?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("txt") {
            continue;
        }
        let Some(fname) = path.file_name().and_then(|s| s.to_str()) else { continue };
        for ctx in &channels {
            let prefix = format!("{}_", ctx.filename_prefix);
            if fname.starts_with(&prefix) {
                if let Some(since) = since {
                    if !file_on_or_after(fname, since) {
                        break;
                    }
                }
                by_channel.entry(ctx.name.clone()).or_default().push(path.clone());
                break;
            }
        }
    }

    for ctx in &channels {
        let files = by_channel.remove(&ctx.name).unwrap_or_default();
        tracing::info!(channel = %ctx.name, files = files.len(), "backfilling");
        for path in files {
            ingest_one(pool, ctx, &path).await?;
        }
        state::rebuild_channel(pool, &ctx.name, cfg.dirty_timeout_min).await?;
    }

    Ok(())
}

async fn ingest_one(pool: &SqlitePool, ctx: &ChannelCtx, path: &Path) -> AppResult<()> {
    let decoded = reader::read_chatlog(path)?;
    let source_file = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();

    let mut tx = pool.begin().await?;
    let mut last_ts: Option<chrono::DateTime<chrono::Utc>> = None;
    let mut total_sightings = 0usize;

    for nl in &decoded.lines {
        let Some(rl) = raw::parse_line(&nl.text) else { continue };
        let sightings = extract::extract(rl.ts, &ctx.name, &rl.author, &rl.body, &ctx.lookups);
        last_ts = Some(rl.ts);
        let n = insert_sightings(&mut tx, &source_file, nl.line_no, &sightings).await?;
        total_sightings += n;
    }

    if let Some(start) = decoded.session_started {
        let end = last_ts.unwrap_or(start);
        sqlx::query(
            "INSERT INTO observation_windows (channel, source_file, started_at, ended_at) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT(source_file) DO UPDATE SET \
                started_at = excluded.started_at, ended_at = excluded.ended_at",
        )
        .bind(&ctx.name)
        .bind(&source_file)
        .bind(start.to_rfc3339())
        .bind(end.to_rfc3339())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    if total_sightings > 0 {
        tracing::debug!(file = %source_file, sightings = total_sightings, "ingested");
    }
    Ok(())
}

fn parse_since(s: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| AppError::Config(format!("--since: {e}; expected YYYY-MM-DD")))
}

/// Chatlog filenames look like `wc.north_20251227_103631_93349646.txt`.
/// Compare the YYYYMMDD chunk against `since`.
fn file_on_or_after(fname: &str, since: NaiveDate) -> bool {
    // skip channel prefix to the first `_YYYYMMDD_`
    let Some(rest) = fname.find('_').map(|i| &fname[i + 1..]) else {
        return true;
    };
    let date_str = &rest[..rest.len().min(8)];
    NaiveDate::parse_from_str(date_str, "%Y%m%d")
        .map(|d| d >= since)
        .unwrap_or(true)
}
