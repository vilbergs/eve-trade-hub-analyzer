//! Axum JSON API for the intel dashboard.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::report::iter_hour_chunks;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

pub type AppState = Arc<SqlitePool>;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/channels", get(channels))
        .route("/heatmap", get(heatmap))
        .route("/safety", get(safety))
        .route("/systems", get(systems))
        .route("/pilots", get(pilots))
        .route("/current", get(current))
        .route("/stats", get(stats))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn parse(s: &str) -> Result<DateTime<Utc>, ApiError> {
    Ok(DateTime::parse_from_rfc3339(s)
        .map_err(|e| ApiError::Internal(format!("bad timestamp {s}: {e}")))?
        .with_timezone(&Utc))
}

fn clip(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    cutoff: DateTime<Utc>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    (start.max(cutoff), end)
}

/// weeks=0 means "all time" — return a cutoff far in the past.
fn cutoff_for(weeks: u32) -> DateTime<Utc> {
    if weeks == 0 {
        chrono::DateTime::UNIX_EPOCH
    } else {
        Utc::now() - Duration::weeks(weeks as i64)
    }
}

// ---------------------------------------------------------------------------
// Error type → JSON 4xx / 5xx
// ---------------------------------------------------------------------------

enum ApiError {
    BadRequest(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        };
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        ApiError::Internal(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Query param structs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ChannelWeeksQuery {
    channel: Option<String>,
    /// 0 or absent = all time.
    #[serde(default)]
    weeks: u32,
}

#[derive(Deserialize)]
struct ChannelQuery {
    channel: Option<String>,
}

#[derive(Deserialize)]
struct PilotsQuery {
    channel: Option<String>,
    #[serde(default = "default_top")]
    top: u32,
}

fn default_top() -> u32 {
    50
}

fn require_channel(ch: &Option<String>) -> Result<String, ApiError> {
    ch.as_ref()
        .filter(|s| !s.is_empty())
        .cloned()
        .ok_or_else(|| ApiError::BadRequest("missing required query parameter: channel".into()))
}

// ---------------------------------------------------------------------------
// GET /api/channels
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ChannelRow {
    name: String,
    filename_prefix: String,
    enabled: bool,
    regions: Vec<String>,
}

async fn channels(State(pool): State<AppState>) -> Result<Json<Vec<ChannelRow>>, ApiError> {
    let rows: Vec<(String, String, i64)> =
        sqlx::query_as("SELECT name, filename_prefix, enabled FROM channels ORDER BY name")
            .fetch_all(pool.as_ref())
            .await?;

    let mut out = Vec::with_capacity(rows.len());
    for (name, prefix, enabled) in rows {
        let regions: Vec<(String,)> = sqlx::query_as(
            "SELECT COALESCE(r.name, '?') FROM channel_regions cr \
             LEFT JOIN regions r ON r.region_id = cr.region_id \
             WHERE cr.channel_name = ? ORDER BY r.name",
        )
        .bind(&name)
        .fetch_all(pool.as_ref())
        .await?;

        out.push(ChannelRow {
            name,
            filename_prefix: prefix,
            enabled: enabled != 0,
            regions: regions.into_iter().map(|(n,)| n).collect(),
        });
    }
    Ok(Json(out))
}

// ---------------------------------------------------------------------------
// GET /api/heatmap?channel=…&weeks=…
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HeatmapResponse {
    channel: String,
    weeks: u32,
    weekdays: Vec<&'static str>,
    hours: Vec<u32>,
    data: Vec<Vec<f64>>,
    observed: Vec<Vec<f64>>,
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

async fn heatmap(
    State(pool): State<AppState>,
    Query(q): Query<ChannelWeeksQuery>,
) -> Result<Json<HeatmapResponse>, ApiError> {
    let channel = require_channel(&q.channel)?;
    let cutoff = cutoff_for(q.weeks);

    let mut obs = [[0.0_f64; 24]; 7];
    let mut dirty = [[0.0_f64; 24]; 7];

    let windows: Vec<(String, String)> = sqlx::query_as(
        "SELECT started_at, ended_at FROM observation_windows \
         WHERE channel = ? AND ended_at >= ?",
    )
    .bind(&channel)
    .bind(cutoff.to_rfc3339())
    .fetch_all(pool.as_ref())
    .await?;
    for (s, e) in &windows {
        let (s, e) = clip(parse(s)?, parse(e)?, cutoff);
        for (wd, h, m) in iter_hour_chunks(s, e) {
            obs[wd as usize][h as usize] += m;
        }
    }

    let intervals: Vec<(String, String)> = sqlx::query_as(
        "SELECT started_at, ended_at FROM dirty_intervals \
         WHERE channel = ? AND ended_at >= ?",
    )
    .bind(&channel)
    .bind(cutoff.to_rfc3339())
    .fetch_all(pool.as_ref())
    .await?;
    for (s, e) in &intervals {
        let (s, e) = clip(parse(s)?, parse(e)?, cutoff);
        for (wd, h, m) in iter_hour_chunks(s, e) {
            dirty[wd as usize][h as usize] += m;
        }
    }

    let data: Vec<Vec<f64>> = (0..7)
        .map(|wd| {
            (0..24)
                .map(|h| {
                    let o = obs[wd][h];
                    if o <= 0.0 { 0.0 } else { dirty[wd][h] / o }
                })
                .collect()
        })
        .collect();

    let observed: Vec<Vec<f64>> = (0..7)
        .map(|wd| (0..24).map(|h| obs[wd][h] / 60.0).collect())
        .collect();

    Ok(Json(HeatmapResponse {
        channel,
        weeks: q.weeks,
        weekdays: WEEKDAYS.to_vec(),
        hours: (0..24).collect(),
        data,
        observed,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/safety?channel=…&weeks=…
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct SafetyResponse {
    channel: String,
    weeks: u32,
    observed_hours: Vec<f64>,
    systems: Vec<SystemSafety>,
}

#[derive(Serialize)]
struct SystemSafety {
    name: String,
    buckets: Vec<f64>,
}

async fn safety(
    State(pool): State<AppState>,
    Query(q): Query<ChannelWeeksQuery>,
) -> Result<Json<SafetyResponse>, ApiError> {
    let channel = require_channel(&q.channel)?;
    let cutoff = cutoff_for(q.weeks);

    // Observed minutes per hour-of-day.
    let mut obs_by_hour = [0.0_f64; 24];
    let windows: Vec<(String, String)> = sqlx::query_as(
        "SELECT started_at, ended_at FROM observation_windows \
         WHERE channel = ? AND ended_at >= ?",
    )
    .bind(&channel)
    .bind(cutoff.to_rfc3339())
    .fetch_all(pool.as_ref())
    .await?;
    for (s, e) in &windows {
        let (s, e) = clip(parse(s)?, parse(e)?, cutoff);
        for (_wd, h, m) in iter_hour_chunks(s, e) {
            obs_by_hour[h as usize] += m;
        }
    }

    // Dirty minutes per (system, hour-of-day).
    let dirty_rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT di.system_id, ss.name, di.started_at, di.ended_at \
         FROM dirty_intervals di \
         JOIN solar_systems ss ON ss.system_id = di.system_id \
         WHERE di.channel = ? AND di.ended_at >= ? \
         ORDER BY ss.name",
    )
    .bind(&channel)
    .bind(cutoff.to_rfc3339())
    .fetch_all(pool.as_ref())
    .await?;

    let mut by_system: BTreeMap<(String, i64), [f64; 24]> = BTreeMap::new();
    for (sys_id, sys_name, s, e) in &dirty_rows {
        let (s, e) = clip(parse(s)?, parse(e)?, cutoff);
        let buckets = by_system
            .entry((sys_name.clone(), *sys_id))
            .or_insert([0.0; 24]);
        for (_wd, h, m) in iter_hour_chunks(s, e) {
            buckets[h as usize] += m;
        }
    }

    let systems: Vec<SystemSafety> = by_system
        .iter()
        .map(|((name, _), buckets)| {
            let pcts: Vec<f64> = (0..24)
                .map(|h| {
                    let obs = obs_by_hour[h];
                    if obs <= 0.0 {
                        0.0
                    } else {
                        (buckets[h] / obs) * 100.0
                    }
                })
                .collect();
            SystemSafety {
                name: name.clone(),
                buckets: pcts,
            }
        })
        .collect();

    Ok(Json(SafetyResponse {
        channel,
        weeks: q.weeks,
        observed_hours: obs_by_hour.iter().map(|m| m / 60.0).collect(),
        systems,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/systems?channel=…&weeks=…
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct SystemsResponse {
    systems: Vec<SystemEntry>,
}

#[derive(Serialize)]
struct SystemEntry {
    name: String,
    sightings: i64,
    intervals: i64,
    dirty_hours: f64,
}

async fn systems(
    State(pool): State<AppState>,
    Query(q): Query<ChannelWeeksQuery>,
) -> Result<Json<SystemsResponse>, ApiError> {
    let channel = require_channel(&q.channel)?;
    let cutoff = cutoff_for(q.weeks).to_rfc3339();

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
    .bind(&channel)
    .bind(&cutoff)
    .bind(&channel)
    .bind(&cutoff)
    .fetch_all(pool.as_ref())
    .await?;

    let systems = rows
        .into_iter()
        .map(|(name, sightings, intervals, dirty_min)| SystemEntry {
            name,
            sightings,
            intervals,
            dirty_hours: dirty_min / 60.0,
        })
        .collect();

    Ok(Json(SystemsResponse { systems }))
}

// ---------------------------------------------------------------------------
// GET /api/pilots?channel=…&top=…
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct PilotsResponse {
    pilots: Vec<PilotEntry>,
}

#[derive(Serialize)]
struct PilotEntry {
    name: String,
    sightings: u64,
    distinct_systems: usize,
    last_seen: String,
}

async fn pilots(
    State(pool): State<AppState>,
    Query(q): Query<PilotsQuery>,
) -> Result<Json<PilotsResponse>, ApiError> {
    let channel = require_channel(&q.channel)?;

    let rows: Vec<(String, String, Option<i64>)> =
        sqlx::query_as("SELECT ts, pilots_json, system_id FROM sightings WHERE channel = ?")
            .bind(&channel)
            .fetch_all(pool.as_ref())
            .await?;

    #[derive(Default)]
    struct Entry {
        count: u64,
        systems: std::collections::BTreeSet<i64>,
        last_seen: String,
    }

    let mut agg: BTreeMap<String, Entry> = BTreeMap::new();
    for (ts, pilots_json, sys) in &rows {
        let pilots: Vec<String> = serde_json::from_str(pilots_json).unwrap_or_default();
        for pilot in pilots {
            if pilot.is_empty() {
                continue;
            }
            let e = agg.entry(pilot).or_default();
            e.count += 1;
            if let Some(sid) = sys {
                e.systems.insert(*sid);
            }
            if ts.as_str() > e.last_seen.as_str() {
                e.last_seen = ts.clone();
            }
        }
    }

    let mut entries: Vec<(String, Entry)> = agg.into_iter().collect();
    entries.sort_by(|a, b| b.1.count.cmp(&a.1.count));
    entries.truncate(q.top as usize);

    let pilots = entries
        .into_iter()
        .map(|(name, e)| PilotEntry {
            name,
            sightings: e.count,
            distinct_systems: e.systems.len(),
            last_seen: e.last_seen,
        })
        .collect();

    Ok(Json(PilotsResponse { pilots }))
}

// ---------------------------------------------------------------------------
// GET /api/current?channel=…
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct CurrentResponse {
    threats: Vec<ThreatEntry>,
}

#[derive(Serialize)]
struct ThreatEntry {
    channel: String,
    system: String,
    started_at: String,
    ended_at: String,
}

async fn current(
    State(pool): State<AppState>,
    Query(q): Query<ChannelQuery>,
) -> Result<Json<CurrentResponse>, ApiError> {
    let timeout_min: i64 = std::env::var("INTEL_DIRTY_TIMEOUT_MIN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let now = Utc::now();
    let lookback = (now - Duration::minutes(timeout_min)).to_rfc3339();

    let rows: Vec<(String, String, String, String)> = if let Some(ch) = q.channel.as_deref() {
        if ch.is_empty() {
            return Err(ApiError::BadRequest(
                "missing required query parameter: channel".into(),
            ));
        }
        sqlx::query_as(
            "SELECT di.channel, ss.name, di.started_at, di.ended_at \
             FROM dirty_intervals di \
             JOIN solar_systems ss ON ss.system_id = di.system_id \
             WHERE di.channel = ? AND di.ended_at >= ? \
             ORDER BY di.ended_at DESC",
        )
        .bind(ch)
        .bind(&lookback)
        .fetch_all(pool.as_ref())
        .await?
    } else {
        sqlx::query_as(
            "SELECT di.channel, ss.name, di.started_at, di.ended_at \
             FROM dirty_intervals di \
             JOIN solar_systems ss ON ss.system_id = di.system_id \
             WHERE di.ended_at >= ? \
             ORDER BY di.ended_at DESC",
        )
        .bind(&lookback)
        .fetch_all(pool.as_ref())
        .await?
    };

    let threats = rows
        .into_iter()
        .map(|(channel, system, started_at, ended_at)| ThreatEntry {
            channel,
            system,
            started_at,
            ended_at,
        })
        .collect();

    Ok(Json(CurrentResponse { threats }))
}

// ---------------------------------------------------------------------------
// GET /api/stats?channel=…&weeks=…
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct StatsResponse {
    total_sightings: i64,
    total_systems_hit: i64,
    total_dirty_hours: f64,
    observation_hours: f64,
    top_system: Option<String>,
    top_pilot: Option<String>,
}

async fn stats(
    State(pool): State<AppState>,
    Query(q): Query<ChannelWeeksQuery>,
) -> Result<Json<StatsResponse>, ApiError> {
    let channel = require_channel(&q.channel)?;
    let cutoff = cutoff_for(q.weeks).to_rfc3339();

    // Total sightings
    let (total_sightings,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM sightings \
         WHERE channel = ? AND ts >= ? AND system_id IS NOT NULL",
    )
    .bind(&channel)
    .bind(&cutoff)
    .fetch_one(pool.as_ref())
    .await?;

    // Total distinct systems hit
    let (total_systems_hit,): (i64,) = sqlx::query_as(
        "SELECT COUNT(DISTINCT system_id) FROM dirty_intervals \
         WHERE channel = ? AND ended_at >= ?",
    )
    .bind(&channel)
    .bind(&cutoff)
    .fetch_one(pool.as_ref())
    .await?;

    // Total dirty hours
    let total_dirty_hours: f64 = {
        let row: (Option<f64>,) = sqlx::query_as(
            "SELECT SUM((julianday(ended_at) - julianday(started_at)) * 24.0) \
             FROM dirty_intervals WHERE channel = ? AND ended_at >= ?",
        )
        .bind(&channel)
        .bind(&cutoff)
        .fetch_one(pool.as_ref())
        .await?;
        row.0.unwrap_or(0.0)
    };

    // Total observation hours
    let observation_hours: f64 = {
        let row: (Option<f64>,) = sqlx::query_as(
            "SELECT SUM((julianday(ended_at) - julianday(started_at)) * 24.0) \
             FROM observation_windows WHERE channel = ? AND ended_at >= ?",
        )
        .bind(&channel)
        .bind(&cutoff)
        .fetch_one(pool.as_ref())
        .await?;
        row.0.unwrap_or(0.0)
    };

    // Top system by dirty hours
    let top_system: Option<String> = {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT ss.name FROM dirty_intervals di \
             JOIN solar_systems ss ON ss.system_id = di.system_id \
             WHERE di.channel = ? AND di.ended_at >= ? \
             GROUP BY di.system_id \
             ORDER BY SUM(julianday(di.ended_at) - julianday(di.started_at)) DESC \
             LIMIT 1",
        )
        .bind(&channel)
        .bind(&cutoff)
        .fetch_optional(pool.as_ref())
        .await?;
        row.map(|(n,)| n)
    };

    // Top pilot by sighting count
    let top_pilot: Option<String> = {
        let sighting_rows: Vec<(String,)> =
            sqlx::query_as("SELECT pilots_json FROM sightings WHERE channel = ? AND ts >= ?")
                .bind(&channel)
                .bind(&cutoff)
                .fetch_all(pool.as_ref())
                .await?;

        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        for (pj,) in &sighting_rows {
            let pilots: Vec<String> = serde_json::from_str(pj).unwrap_or_default();
            for p in pilots {
                if !p.is_empty() {
                    *counts.entry(p).or_default() += 1;
                }
            }
        }
        counts
            .into_iter()
            .max_by_key(|(_, c)| *c)
            .map(|(name, _)| name)
    };

    Ok(Json(StatsResponse {
        total_sightings,
        total_systems_hit,
        total_dirty_hours,
        observation_hours,
        top_system,
        top_pilot,
    }))
}
