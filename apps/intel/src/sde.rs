//! Fuzzwork SDE sync (subset). Pulls the map + ship type CSVs directly into
//! the app's SQLite DB. Idempotent via the concatenation of upstream
//! Last-Modified headers, hashed and compared to the stored version.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use eve_core::{AppError, AppResult};

const BASE: &str = "https://www.fuzzwork.co.uk/dump/latest";
const SHIP_CATEGORY_ID: i64 = 6;

const FILES: &[&str] = &[
    "mapRegions.csv",
    "mapConstellations.csv",
    "mapSolarSystems.csv",
    "invGroups.csv",
    "invTypes.csv",
];

pub async fn sync(pool: &SqlitePool) -> AppResult<()> {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("eve-intel/0.1 (+local tool)")
        .build()?;

    let mut version_seed = String::new();
    for f in FILES {
        let lm = head_last_modified(&http, &format!("{BASE}/{f}")).await?;
        version_seed.push_str(f);
        version_seed.push('=');
        version_seed.push_str(&lm);
        version_seed.push('\n');
    }
    let version = sha256_hex(&version_seed);
    tracing::info!(version = %version, "computed SDE version");

    if current_version(pool).await? == Some(version.clone()) {
        tracing::info!("SDE already up to date");
        return Ok(());
    }

    tracing::info!("downloading SDE CSVs");
    let regions_csv = get(&http, "mapRegions.csv").await?;
    let constellations_csv = get(&http, "mapConstellations.csv").await?;
    let systems_csv = get(&http, "mapSolarSystems.csv").await?;
    let groups_csv = get(&http, "invGroups.csv").await?;
    let types_csv = get(&http, "invTypes.csv").await?;

    let regions = parse_regions(&regions_csv)?;
    let constellations = parse_constellations(&constellations_csv)?;
    let systems = parse_systems(&systems_csv)?;
    let ship_group_ids = parse_ship_group_ids(&groups_csv)?;
    let ship_types = parse_ship_types(&types_csv, &ship_group_ids)?;

    tracing::info!(
        regions = regions.len(),
        constellations = constellations.len(),
        systems = systems.len(),
        ship_groups = ship_group_ids.len(),
        ship_types = ship_types.len(),
        "parsed CSVs"
    );

    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM solar_systems").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM constellations").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM regions").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM ship_types").execute(&mut *tx).await?;

    for r in &regions {
        sqlx::query("INSERT INTO regions (region_id, name) VALUES (?, ?)")
            .bind(r.id)
            .bind(&r.name)
            .execute(&mut *tx)
            .await?;
    }
    for c in &constellations {
        sqlx::query(
            "INSERT INTO constellations (constellation_id, region_id, name) VALUES (?, ?, ?)",
        )
        .bind(c.id)
        .bind(c.region_id)
        .bind(&c.name)
        .execute(&mut *tx)
        .await?;
    }
    for s in &systems {
        sqlx::query(
            "INSERT INTO solar_systems (system_id, constellation_id, name, security) VALUES (?, ?, ?, ?)",
        )
        .bind(s.id)
        .bind(s.constellation_id)
        .bind(&s.name)
        .bind(s.security)
        .execute(&mut *tx)
        .await?;
    }
    for t in &ship_types {
        sqlx::query("INSERT INTO ship_types (type_id, name) VALUES (?, ?)")
            .bind(t.id)
            .bind(&t.name)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query(
        "INSERT INTO sde_meta (id, version, loaded_at) VALUES (1, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET version = excluded.version, loaded_at = excluded.loaded_at",
    )
    .bind(&version)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    tracing::info!("SDE sync complete");
    Ok(())
}

async fn head_last_modified(http: &reqwest::Client, url: &str) -> AppResult<String> {
    let resp = http.head(url).send().await?.error_for_status()?;
    let lm = resp
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Other(format!("no Last-Modified header on {url}")))?;
    Ok(lm.to_owned())
}

async fn get(http: &reqwest::Client, name: &str) -> AppResult<Vec<u8>> {
    let url = format!("{BASE}/{name}");
    Ok(http
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec())
}

async fn current_version(pool: &SqlitePool) -> AppResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT version FROM sde_meta WHERE id = 1")
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.0))
}

fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    format!("sha256:{:x}", h.finalize())
}

struct Region { id: i64, name: String }
struct Constellation { id: i64, region_id: i64, name: String }
struct System { id: i64, constellation_id: i64, name: String, security: Option<f64> }
struct ShipType { id: i64, name: String }

type Row = HashMap<String, String>;

fn rows(bytes: &[u8], label: &str) -> AppResult<Vec<Row>> {
    let mut rdr = csv::Reader::from_reader(bytes);
    let mut out = Vec::new();
    for rec in rdr.deserialize::<Row>() {
        out.push(rec.map_err(|e| AppError::Other(format!("{label} csv: {e}")))?);
    }
    Ok(out)
}

fn parse_i64(row: &Row, key: &str) -> Option<i64> {
    row.get(key).and_then(|s| s.parse().ok())
}

fn parse_regions(bytes: &[u8]) -> AppResult<Vec<Region>> {
    let mut out = Vec::new();
    for r in rows(bytes, "regions")? {
        if let (Some(id), Some(name)) = (parse_i64(&r, "regionID"), r.get("regionName").cloned()) {
            out.push(Region { id, name });
        }
    }
    Ok(out)
}

fn parse_constellations(bytes: &[u8]) -> AppResult<Vec<Constellation>> {
    let mut out = Vec::new();
    for r in rows(bytes, "constellations")? {
        if let (Some(id), Some(region_id), Some(name)) = (
            parse_i64(&r, "constellationID"),
            parse_i64(&r, "regionID"),
            r.get("constellationName").cloned(),
        ) {
            out.push(Constellation { id, region_id, name });
        }
    }
    Ok(out)
}

fn parse_systems(bytes: &[u8]) -> AppResult<Vec<System>> {
    let mut out = Vec::new();
    for r in rows(bytes, "systems")? {
        if let (Some(id), Some(constellation_id), Some(name)) = (
            parse_i64(&r, "solarSystemID"),
            parse_i64(&r, "constellationID"),
            r.get("solarSystemName").cloned(),
        ) {
            let security = r.get("security").and_then(|s| s.parse().ok());
            out.push(System { id, constellation_id, name, security });
        }
    }
    Ok(out)
}

fn parse_ship_group_ids(bytes: &[u8]) -> AppResult<HashSet<i64>> {
    let mut out = HashSet::new();
    for r in rows(bytes, "groups")? {
        if parse_i64(&r, "categoryID") == Some(SHIP_CATEGORY_ID) {
            if let Some(gid) = parse_i64(&r, "groupID") {
                out.insert(gid);
            }
        }
    }
    Ok(out)
}

fn parse_ship_types(bytes: &[u8], ship_groups: &HashSet<i64>) -> AppResult<Vec<ShipType>> {
    let mut out = Vec::new();
    for r in rows(bytes, "types")? {
        let Some(gid) = parse_i64(&r, "groupID") else { continue };
        if !ship_groups.contains(&gid) {
            continue;
        }
        if r.get("published").map(|s| s.as_str()) != Some("1") {
            continue;
        }
        let Some(id) = parse_i64(&r, "typeID") else { continue };
        let Some(name) = r.get("typeName").cloned() else { continue };
        if name.is_empty() {
            continue;
        }
        out.push(ShipType { id, name });
    }
    Ok(out)
}
