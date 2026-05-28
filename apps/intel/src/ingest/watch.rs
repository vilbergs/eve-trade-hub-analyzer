//! `intel watch` — tail the chatlog directory and re-ingest matching files
//! as they're modified. Idempotent: UNIQUE(source_file, line_no) means
//! repeated re-reads of a growing file just append new rows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use sqlx::SqlitePool;

use eve_core::{AppError, AppResult};

use crate::config::Config;
use crate::ingest::{ChannelCtx, insert_sightings, load_enabled_channels, reader};
use crate::parser::{extract, raw};
use crate::state;

pub async fn run(pool: &SqlitePool, cfg: &Config, channel: Option<&str>) -> AppResult<()> {
    let channels = load_enabled_channels(pool, channel).await?;
    let dir = cfg.chatlog_dir.clone();
    if !dir.exists() {
        return Err(AppError::Config(format!(
            "chatlog dir does not exist: {}",
            dir.display()
        )));
    }

    tracing::info!(
        chatlog_dir = %dir.display(),
        channels = channels.iter().map(|c| c.name.clone()).collect::<Vec<_>>().join(","),
        "watching"
    );

    // Bridge notify's synchronous channel to a tokio task that drives the DB.
    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(250), tx)
        .map_err(|e| AppError::Other(format!("debouncer init: {e}")))?;
    debouncer
        .watcher()
        .watch(&dir, RecursiveMode::NonRecursive)
        .map_err(|e| AppError::Other(format!("watch start: {e}")))?;

    // Initial backfill of files currently in the directory so the live tail
    // picks up an already-open session log.
    let mut prev_counts: HashMap<PathBuf, u64> = HashMap::new();
    for ctx in &channels {
        let prefix = format!("{}_", ctx.filename_prefix);
        for entry in std::fs::read_dir(&dir).map_err(AppError::Io)? {
            let entry = entry.map_err(AppError::Io)?;
            let path = entry.path();
            let Some(fname) = path.file_name().and_then(|s| s.to_str()) else { continue };
            if fname.starts_with(&prefix) && fname.ends_with(".txt") {
                let inserted = ingest_one(pool, ctx, &path).await?;
                prev_counts.insert(path, inserted);
            }
        }
        state::rebuild_channel(pool, &ctx.name, cfg.dirty_timeout_min).await?;
    }
    tracing::info!("initial state synced; live tailing");

    let mut last_state_rebuild = Instant::now();
    let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));

    loop {
        let rx_for_recv = rx.clone();
        let evt = tokio::task::spawn_blocking(move || rx_for_recv.lock().unwrap().recv())
            .await
            .map_err(|e| AppError::Other(format!("join: {e}")))?;

        let Ok(events) = evt else { break };
        let events = match events {
            Ok(evs) => evs,
            Err(errs) => {
                tracing::warn!(?errs, "watcher errors");
                continue;
            }
        };

        let mut touched_channels = std::collections::BTreeSet::new();
        for ev in events {
            let path = ev.path;
            let Some(fname) = path.file_name().and_then(|s| s.to_str()) else { continue };
            for ctx in &channels {
                let prefix = format!("{}_", ctx.filename_prefix);
                if !fname.starts_with(&prefix) || !fname.ends_with(".txt") {
                    continue;
                }
                let inserted = match ingest_one(pool, ctx, &path).await {
                    Ok(n) => n,
                    Err(e) => {
                        tracing::warn!(file=%fname, "ingest failed: {e}");
                        continue;
                    }
                };
                let prev = prev_counts.insert(path.clone(), inserted).unwrap_or(0);
                let new_rows = inserted.saturating_sub(prev);
                if new_rows > 0 {
                    print_new_sightings(pool, ctx, &fname, new_rows as i64).await;
                    touched_channels.insert(ctx.name.clone());
                }
                break;
            }
        }

        // Rebuild dirty intervals lazily — at most once every 15s — for any
        // channel that saw new rows.
        if !touched_channels.is_empty()
            && last_state_rebuild.elapsed() > Duration::from_secs(15)
        {
            for ch in &touched_channels {
                if let Err(e) = state::rebuild_channel(pool, ch, cfg.dirty_timeout_min).await {
                    tracing::warn!(channel=%ch, "rebuild failed: {e}");
                }
            }
            last_state_rebuild = Instant::now();
        }
    }

    Ok(())
}

/// Ingest a file fully and return the *total* number of sighting rows
/// for that source_file after the operation (cumulative count). Caller
/// diffs against its previous value to print only new sightings.
async fn ingest_one(pool: &SqlitePool, ctx: &ChannelCtx, path: &Path) -> AppResult<u64> {
    let decoded = reader::read_chatlog(path)?;
    let source_file = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();

    let mut tx = pool.begin().await?;
    let mut last_ts: Option<chrono::DateTime<chrono::Utc>> = None;
    for nl in &decoded.lines {
        let Some(rl) = raw::parse_line(&nl.text) else { continue };
        let sightings = extract::extract(rl.ts, &ctx.name, &rl.author, &rl.body, &ctx.lookups);
        last_ts = Some(rl.ts);
        insert_sightings(&mut tx, &source_file, nl.line_no, &sightings).await?;
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

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM sightings WHERE source_file = ?")
            .bind(&source_file)
            .fetch_one(pool)
            .await?;
    Ok(count.0 as u64)
}

async fn print_new_sightings(pool: &SqlitePool, ctx: &ChannelCtx, fname: &str, n: i64) {
    let rows = sqlx::query_as::<_, (String, String, Option<String>, String, i64, i64)>(
        "SELECT s.ts, s.reporter, ss.name, s.pilots_json, s.no_visual, s.is_clear \
           FROM sightings s LEFT JOIN solar_systems ss ON ss.system_id = s.system_id \
          WHERE s.source_file = ? \
          ORDER BY s.id DESC LIMIT ?",
    )
    .bind(fname)
    .bind(n)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (ts, reporter, system, pilots, nv, clr) in rows.into_iter().rev() {
        let pilots: Vec<String> = serde_json::from_str(&pilots).unwrap_or_default();
        let suffix = if clr != 0 {
            " [CLEAR]".to_string()
        } else if nv != 0 {
            " [nv]".to_string()
        } else {
            String::new()
        };
        println!(
            "{} [{}] {} > {} {}{}",
            ts,
            ctx.name,
            reporter,
            system.unwrap_or_else(|| "?".into()),
            pilots.join(", "),
            suffix,
        );
    }
}
