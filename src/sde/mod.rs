//! SDE (Static Data Export) sync.
//!
//! Pulls the Fuzzwork CSV dumps and loads the categories / groups /
//! market-groups / types subset into Postgres. Idempotent: the upstream
//! `/dump/latest/checksum` file is hashed and compared to a stored version
//! identifier, so re-running while upstream is unchanged is a no-op.

use std::time::Duration;

use sha2::{Digest, Sha256};
use sqlx::{Acquire, PgPool};
use tracing::{info, instrument};

use crate::error::{AppError, AppResult};

const BASE_URL: &str = "https://www.fuzzwork.co.uk/dump/latest";

#[derive(Debug, Clone)]
pub enum SdeReport {
    UpToDate {
        version: String,
    },
    Loaded {
        version: String,
        categories: u64,
        groups: u64,
        market_groups: u64,
        types: u64,
    },
}

#[instrument(skip_all)]
pub async fn sync(pool: &PgPool, http: &reqwest::Client) -> AppResult<SdeReport> {
    let checksum_body = fetch_text(http, &format!("{BASE_URL}/checksum")).await?;
    let version = version_id(&checksum_body);
    info!(version = %version, "computed SDE version");

    if current_version(pool).await? == Some(version.clone()) {
        info!("SDE up to date, skipping");
        return Ok(SdeReport::UpToDate { version });
    }

    let categories = fetch_bytes(http, &format!("{BASE_URL}/invCategories.csv")).await?;
    let groups = fetch_bytes(http, &format!("{BASE_URL}/invGroups.csv")).await?;
    let market_groups = fetch_bytes(http, &format!("{BASE_URL}/invMarketGroups.csv")).await?;
    let types = fetch_bytes(http, &format!("{BASE_URL}/invTypes.csv")).await?;
    let volumes = fetch_bytes(http, &format!("{BASE_URL}/invVolumes.csv")).await?;
    info!(
        categories = categories.len(),
        groups = groups.len(),
        market_groups = market_groups.len(),
        types = types.len(),
        volumes = volumes.len(),
        "downloaded CSVs"
    );

    let mut tx = pool.begin().await?;
    let counts = load_all(
        &mut tx,
        &categories,
        &groups,
        &market_groups,
        &types,
        &volumes,
    )
    .await?;
    sqlx::query(
        "INSERT INTO eve_sde_meta (id, version, loaded_at) \
         VALUES (1, $1, now()) \
         ON CONFLICT (id) DO UPDATE SET version = EXCLUDED.version, loaded_at = EXCLUDED.loaded_at",
    )
    .bind(&version)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(SdeReport::Loaded {
        version,
        categories: counts.categories,
        groups: counts.groups,
        market_groups: counts.market_groups,
        types: counts.types,
    })
}

async fn current_version(pool: &PgPool) -> AppResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT version FROM eve_sde_meta WHERE id = 1")
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.0))
}

async fn fetch_text(http: &reqwest::Client, url: &str) -> AppResult<String> {
    let resp = http
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await?
        .error_for_status()?;
    Ok(resp.text().await?)
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

fn version_id(body: &str) -> String {
    let mut h = Sha256::new();
    h.update(body.as_bytes());
    let digest = h.finalize();
    format!("sha256:{:x}", digest)
}

struct Counts {
    categories: u64,
    groups: u64,
    market_groups: u64,
    types: u64,
}

async fn load_all<'c>(
    tx: &mut sqlx::Transaction<'c, sqlx::Postgres>,
    categories_csv: &[u8],
    groups_csv: &[u8],
    market_groups_csv: &[u8],
    types_csv: &[u8],
    volumes_csv: &[u8],
) -> AppResult<Counts> {
    let conn = tx.acquire().await?;

    // 1. Stage every CSV into a temp TEXT-only table.
    sqlx::query(
        "CREATE TEMP TABLE tmp_categories ( \
            category_id TEXT, name TEXT, icon_id TEXT, published TEXT \
        ) ON COMMIT DROP",
    )
    .execute(&mut *conn)
    .await?;
    copy_csv(
        conn,
        "COPY tmp_categories FROM STDIN WITH (FORMAT csv, HEADER true)",
        categories_csv,
    )
    .await?;

    sqlx::query(
        "CREATE TEMP TABLE tmp_groups ( \
            group_id TEXT, category_id TEXT, name TEXT, icon_id TEXT, \
            use_base_price TEXT, anchored TEXT, anchorable TEXT, \
            fittable_non_singleton TEXT, published TEXT \
        ) ON COMMIT DROP",
    )
    .execute(&mut *conn)
    .await?;
    copy_csv(
        conn,
        "COPY tmp_groups FROM STDIN WITH (FORMAT csv, HEADER true)",
        groups_csv,
    )
    .await?;

    sqlx::query(
        "CREATE TEMP TABLE tmp_market_groups ( \
            market_group_id TEXT, parent_group_id TEXT, name TEXT, \
            description TEXT, icon_id TEXT, has_types TEXT \
        ) ON COMMIT DROP",
    )
    .execute(&mut *conn)
    .await?;
    copy_csv(
        conn,
        "COPY tmp_market_groups FROM STDIN WITH (FORMAT csv, HEADER true)",
        market_groups_csv,
    )
    .await?;

    sqlx::query(
        "CREATE TEMP TABLE tmp_types ( \
            type_id TEXT, group_id TEXT, name TEXT, description TEXT, \
            mass TEXT, volume TEXT, capacity TEXT, portion_size TEXT, \
            race_id TEXT, base_price TEXT, published TEXT, market_group_id TEXT, \
            icon_id TEXT, sound_id TEXT, graphic_id TEXT \
        ) ON COMMIT DROP",
    )
    .execute(&mut *conn)
    .await?;
    copy_csv(
        conn,
        "COPY tmp_types FROM STDIN WITH (FORMAT csv, HEADER true)",
        types_csv,
    )
    .await?;

    sqlx::query(
        "CREATE TEMP TABLE tmp_volumes ( \
            type_id TEXT, volume TEXT \
        ) ON COMMIT DROP",
    )
    .execute(&mut *conn)
    .await?;
    copy_csv(
        conn,
        "COPY tmp_volumes FROM STDIN WITH (FORMAT csv, HEADER true)",
        volumes_csv,
    )
    .await?;

    // 2. Wipe real tables in FK-safe order, then re-populate.
    sqlx::query("TRUNCATE eve_types, eve_market_groups, eve_groups, eve_categories")
        .execute(&mut *conn)
        .await?;

    let categories = sqlx::query(
        "INSERT INTO eve_categories (category_id, name, published) \
         SELECT category_id::BIGINT, name, to_bool(published) \
         FROM tmp_categories WHERE category_id IS NOT NULL AND category_id <> ''",
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();

    let groups = sqlx::query(
        "INSERT INTO eve_groups (group_id, category_id, name, published) \
         SELECT group_id::BIGINT, category_id::BIGINT, name, to_bool(published) \
         FROM tmp_groups \
         WHERE group_id IS NOT NULL AND group_id <> '' \
           AND EXISTS (SELECT 1 FROM eve_categories c WHERE c.category_id = tmp_groups.category_id::BIGINT)",
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();

    // Self-referential FK: insert with NULL parents first, then UPDATE.
    sqlx::query(
        "INSERT INTO eve_market_groups (market_group_id, name, parent_id) \
         SELECT market_group_id::BIGINT, name, NULL::BIGINT \
         FROM tmp_market_groups WHERE market_group_id IS NOT NULL AND market_group_id <> ''",
    )
    .execute(&mut *conn)
    .await?;
    let market_groups = sqlx::query(
        "UPDATE eve_market_groups mg \
         SET parent_id = NULLIF(NULLIF(t.parent_group_id, ''), 'None')::BIGINT \
         FROM tmp_market_groups t \
         WHERE mg.market_group_id = t.market_group_id::BIGINT \
           AND t.parent_group_id IS NOT NULL \
           AND t.parent_group_id NOT IN ('', 'None')",
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();

    let types = sqlx::query(
        "INSERT INTO eve_types ( \
            type_id, name, group_id, market_group_id, \
            volume, packaged_volume, published \
         ) \
         SELECT \
            t.type_id::BIGINT, \
            t.name, \
            t.group_id::BIGINT, \
            NULLIF(NULLIF(t.market_group_id, ''), 'None')::BIGINT, \
            COALESCE(NULLIF(NULLIF(t.volume, ''), 'None')::DOUBLE PRECISION, 0), \
            COALESCE( \
                NULLIF(NULLIF(v.volume, ''), 'None')::DOUBLE PRECISION, \
                NULLIF(NULLIF(t.volume, ''), 'None')::DOUBLE PRECISION, \
                0 \
            ), \
            to_bool(t.published) \
         FROM tmp_types t \
         LEFT JOIN tmp_volumes v ON v.type_id = t.type_id \
         WHERE t.type_id IS NOT NULL AND t.type_id <> '' \
           AND t.group_id IS NOT NULL AND t.group_id <> '' \
           AND EXISTS (SELECT 1 FROM eve_groups g WHERE g.group_id = t.group_id::BIGINT)",
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();

    Ok(Counts {
        categories,
        groups,
        market_groups,
        types,
    })
}

async fn copy_csv(conn: &mut sqlx::PgConnection, sql: &str, body: &[u8]) -> AppResult<()> {
    let mut stream = conn
        .copy_in_raw(sql)
        .await
        .map_err(|e| AppError::Other(format!("COPY start: {e}")))?;
    stream
        .send(body)
        .await
        .map_err(|e| AppError::Other(format!("COPY send: {e}")))?;
    stream
        .finish()
        .await
        .map_err(|e| AppError::Other(format!("COPY finish: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_id_is_deterministic() {
        let a = version_id("abc\n");
        let b = version_id("abc\n");
        assert_eq!(a, b);
        assert!(a.starts_with("sha256:"));
    }

    #[test]
    fn version_id_changes_with_content() {
        assert_ne!(version_id("abc"), version_id("abd"));
    }
}
