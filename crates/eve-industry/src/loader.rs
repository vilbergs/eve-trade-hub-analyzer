//! Fuzzwork CSV loader for EVE Online industry data.
//!
//! Downloads blueprint and PI schematic CSVs from Fuzzwork and upserts
//! them into Postgres. Idempotent: uses a HEAD request Last-Modified
//! header as a version fingerprint and skips loading when unchanged.

use std::time::Duration;

use sha2::{Digest, Sha256};
use sqlx::{Acquire, PgPool};
use tracing::{info, instrument};

use eve_core::{AppError, AppResult};

const BASE_URL: &str = "https://www.fuzzwork.co.uk/dump/latest";

/// Result of a sync attempt.
#[derive(Debug, Clone)]
pub enum IndustryReport {
    UpToDate {
        version: String,
    },
    Loaded {
        version: String,
        blueprints: u64,
        activities: u64,
        materials: u64,
        products: u64,
        pi_schematics: u64,
        pi_types: u64,
    },
}

/// Synchronise industry data from Fuzzwork into Postgres.
///
/// Self-bootstrapping: creates the `eve_industry_meta` tracking table if
/// it does not already exist. Uses the Last-Modified header on
/// `industryActivity.csv` as a version seed — if unchanged since the
/// last successful load, returns [`IndustryReport::UpToDate`].
#[instrument(skip_all)]
pub async fn sync(pool: &PgPool, http: &reqwest::Client) -> AppResult<IndustryReport> {
    // Ensure meta table exists (self-bootstrapping).
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS eve_industry_meta ( \
            id INTEGER PRIMARY KEY, \
            version TEXT NOT NULL, \
            loaded_at TIMESTAMPTZ NOT NULL DEFAULT now() \
        )",
    )
    .execute(pool)
    .await?;

    // Compute version from Last-Modified of a representative file.
    let version_seed =
        fetch_last_modified(http, &format!("{BASE_URL}/industryActivity.csv")).await?;
    let version = version_id(&version_seed);
    info!(version = %version, "computed industry version");

    if current_version(pool).await? == Some(version.clone()) {
        info!("industry data up to date, skipping");
        return Ok(IndustryReport::UpToDate { version });
    }

    // Download all CSVs.
    let blueprints_csv = fetch_bytes(http, &format!("{BASE_URL}/industryBlueprints.csv")).await?;
    let activities_csv = fetch_bytes(http, &format!("{BASE_URL}/industryActivity.csv")).await?;
    let materials_csv =
        fetch_bytes(http, &format!("{BASE_URL}/industryActivityMaterials.csv")).await?;
    let products_csv =
        fetch_bytes(http, &format!("{BASE_URL}/industryActivityProducts.csv")).await?;
    let schematics_csv = fetch_bytes(http, &format!("{BASE_URL}/planetSchematics.csv")).await?;
    let schematic_types_csv =
        fetch_bytes(http, &format!("{BASE_URL}/planetSchematicsTypeMap.csv")).await?;
    info!(
        blueprints = blueprints_csv.len(),
        activities = activities_csv.len(),
        materials = materials_csv.len(),
        products = products_csv.len(),
        schematics = schematics_csv.len(),
        schematic_types = schematic_types_csv.len(),
        "downloaded industry CSVs (bytes)"
    );

    let mut tx = pool.begin().await?;
    let conn = tx.acquire().await?;

    // --- Blueprints ---
    info!("loading sde_blueprints");
    let blueprints = load_blueprints(conn, &blueprints_csv).await?;

    // --- Activities (depends on blueprints) ---
    info!("loading sde_blueprint_activities");
    let activities = load_activities(conn, &activities_csv).await?;

    // --- Materials & Products (depend on activities) ---
    info!("loading sde_blueprint_materials");
    let materials = load_materials(conn, &materials_csv).await?;

    info!("loading sde_blueprint_products");
    let products = load_products(conn, &products_csv).await?;

    // --- PI Schematics ---
    info!("loading sde_planet_schematics");
    let pi_schematics = load_pi_schematics(conn, &schematics_csv).await?;

    // --- PI Schematic Types (depends on schematics) ---
    info!("loading sde_planet_schematic_types");
    let pi_types = load_pi_schematic_types(conn, &schematic_types_csv).await?;

    // Update version marker.
    sqlx::query(
        "INSERT INTO eve_industry_meta (id, version, loaded_at) \
         VALUES (1, $1, now()) \
         ON CONFLICT (id) DO UPDATE SET version = EXCLUDED.version, loaded_at = EXCLUDED.loaded_at",
    )
    .bind(&version)
    .execute(&mut *conn)
    .await?;

    tx.commit().await?;
    info!(
        blueprints,
        activities, materials, products, pi_schematics, pi_types, "industry load complete"
    );

    Ok(IndustryReport::Loaded {
        version,
        blueprints,
        activities,
        materials,
        products,
        pi_schematics,
        pi_types,
    })
}

// ---------------------------------------------------------------------------
// Loaders for each table
// ---------------------------------------------------------------------------

async fn load_blueprints(conn: &mut sqlx::PgConnection, csv: &[u8]) -> AppResult<u64> {
    let mut count: u64 = 0;
    for line in csv_lines(csv) {
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 2 {
            continue;
        }
        let type_id: i64 = match cols[0].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let max_prod_limit: i32 = match cols[1].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        sqlx::query(
            "INSERT INTO sde_blueprints (blueprint_type_id, max_production_limit) \
             VALUES ($1, $2) \
             ON CONFLICT (blueprint_type_id) DO UPDATE SET \
                max_production_limit = EXCLUDED.max_production_limit",
        )
        .bind(type_id)
        .bind(max_prod_limit)
        .execute(&mut *conn)
        .await?;
        count += 1;
    }
    Ok(count)
}

async fn load_activities(conn: &mut sqlx::PgConnection, csv: &[u8]) -> AppResult<u64> {
    let mut count: u64 = 0;
    for line in csv_lines(csv) {
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 3 {
            continue;
        }
        let type_id: i64 = match cols[0].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let activity_id: i32 = match cols[1].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let time: i32 = match cols[2].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        sqlx::query(
            "INSERT INTO sde_blueprint_activities (blueprint_type_id, activity_id, time_secs) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (blueprint_type_id, activity_id) DO UPDATE SET \
                time_secs = EXCLUDED.time_secs",
        )
        .bind(type_id)
        .bind(activity_id)
        .bind(time)
        .execute(&mut *conn)
        .await?;
        count += 1;
    }
    Ok(count)
}

async fn load_materials(conn: &mut sqlx::PgConnection, csv: &[u8]) -> AppResult<u64> {
    let mut count: u64 = 0;
    for line in csv_lines(csv) {
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 4 {
            continue;
        }
        let type_id: i64 = match cols[0].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let activity_id: i32 = match cols[1].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let material_type_id: i64 = match cols[2].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let quantity: i32 = match cols[3].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        sqlx::query(
            "INSERT INTO sde_blueprint_materials (blueprint_type_id, activity_id, material_type_id, quantity) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (blueprint_type_id, activity_id, material_type_id) DO UPDATE SET \
                quantity = EXCLUDED.quantity",
        )
        .bind(type_id)
        .bind(activity_id)
        .bind(material_type_id)
        .bind(quantity)
        .execute(&mut *conn)
        .await?;
        count += 1;
    }
    Ok(count)
}

async fn load_products(conn: &mut sqlx::PgConnection, csv: &[u8]) -> AppResult<u64> {
    let mut count: u64 = 0;
    for line in csv_lines(csv) {
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 4 {
            continue;
        }
        let type_id: i64 = match cols[0].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let activity_id: i32 = match cols[1].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let product_type_id: i64 = match cols[2].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let quantity: i32 = match cols[3].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        sqlx::query(
            "INSERT INTO sde_blueprint_products (blueprint_type_id, activity_id, product_type_id, quantity) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (blueprint_type_id, activity_id, product_type_id) DO UPDATE SET \
                quantity = EXCLUDED.quantity",
        )
        .bind(type_id)
        .bind(activity_id)
        .bind(product_type_id)
        .bind(quantity)
        .execute(&mut *conn)
        .await?;
        count += 1;
    }
    Ok(count)
}

async fn load_pi_schematics(conn: &mut sqlx::PgConnection, csv: &[u8]) -> AppResult<u64> {
    let mut count: u64 = 0;
    for line in csv_lines(csv) {
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 3 {
            continue;
        }
        let schematic_id: i32 = match cols[0].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let name = cols[1];
        let cycle_time: i32 = match cols[2].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        sqlx::query(
            "INSERT INTO sde_planet_schematics (schematic_id, schematic_name, cycle_time_secs) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (schematic_id) DO UPDATE SET \
                schematic_name = EXCLUDED.schematic_name, \
                cycle_time_secs = EXCLUDED.cycle_time_secs",
        )
        .bind(schematic_id)
        .bind(name)
        .bind(cycle_time)
        .execute(&mut *conn)
        .await?;
        count += 1;
    }
    Ok(count)
}

async fn load_pi_schematic_types(conn: &mut sqlx::PgConnection, csv: &[u8]) -> AppResult<u64> {
    let mut count: u64 = 0;
    for line in csv_lines(csv) {
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 4 {
            continue;
        }
        let schematic_id: i32 = match cols[0].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let type_id: i64 = match cols[1].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let quantity: i32 = match cols[2].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let is_input: bool = match cols[3].trim() {
            "1" | "true" | "True" | "t" => true,
            _ => false,
        };
        sqlx::query(
            "INSERT INTO sde_planet_schematic_types (schematic_id, type_id, quantity, is_input) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (schematic_id, type_id) DO UPDATE SET \
                quantity = EXCLUDED.quantity, \
                is_input = EXCLUDED.is_input",
        )
        .bind(schematic_id)
        .bind(type_id)
        .bind(quantity)
        .bind(is_input)
        .execute(&mut *conn)
        .await?;
        count += 1;
    }
    Ok(count)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse CSV bytes into lines, skipping the header row and empty lines.
fn csv_lines(data: &[u8]) -> impl Iterator<Item = &str> {
    let text = std::str::from_utf8(data).unwrap_or("");
    text.lines().skip(1).filter(|l| !l.trim().is_empty())
}

async fn current_version(pool: &PgPool) -> AppResult<Option<String>> {
    // Table might not exist yet on first run; catch the error gracefully.
    let row: Result<Option<(String,)>, _> =
        sqlx::query_as("SELECT version FROM eve_industry_meta WHERE id = 1")
            .fetch_optional(pool)
            .await;
    match row {
        Ok(Some(r)) => Ok(Some(r.0)),
        Ok(None) => Ok(None),
        Err(_) => Ok(None), // table doesn't exist yet
    }
}

async fn fetch_last_modified(http: &reqwest::Client, url: &str) -> AppResult<String> {
    let resp = http
        .head(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await?
        .error_for_status()?;
    let lm = resp
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Other(format!("no Last-Modified header on {url}")))?;
    Ok(lm.to_owned())
}

async fn fetch_bytes(http: &reqwest::Client, url: &str) -> AppResult<Vec<u8>> {
    let resp = http
        .get(url)
        .timeout(Duration::from_secs(120))
        .send()
        .await?
        .error_for_status()?;
    Ok(resp.bytes().await?.to_vec())
}

fn version_id(seed: &str) -> String {
    let mut h = Sha256::new();
    h.update(seed.as_bytes());
    let digest = h.finalize();
    format!("sha256:{digest:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_id_is_deterministic() {
        let a = version_id("Mon, 19 May 2025 08:00:00 GMT");
        let b = version_id("Mon, 19 May 2025 08:00:00 GMT");
        assert_eq!(a, b);
        assert!(a.starts_with("sha256:"));
    }

    #[test]
    fn version_id_changes_with_content() {
        assert_ne!(version_id("abc"), version_id("abd"));
    }

    #[test]
    fn csv_lines_skips_header() {
        let data = b"typeID,maxProductionLimit\n34,10\n35,20\n";
        let lines: Vec<&str> = csv_lines(data).collect();
        assert_eq!(lines, vec!["34,10", "35,20"]);
    }

    #[test]
    fn csv_lines_handles_crlf() {
        let data = b"a,b\r\n1,2\r\n3,4\r\n";
        let lines: Vec<&str> = csv_lines(data).collect();
        assert_eq!(lines.len(), 2);
        // \r may remain at the end — splits on \n, trims aren't applied to content
        assert!(lines[0].starts_with("1,"));
    }

    #[test]
    fn csv_lines_empty_input() {
        let data = b"";
        let lines: Vec<&str> = csv_lines(data).collect();
        assert!(lines.is_empty());
    }
}
