//! Classifies EVE Online type_ids into node kinds for manufacturing graph visualization.
//!
//! Each type_id is mapped to a [`NodeKind`] based on its SDE group/category
//! and its presence in blueprint or PI schematic tables. Used by the Forgepath
//! UI to colour and group nodes in the manufacturing dependency graph.

use std::collections::HashMap;

use eve_core::AppResult;
use serde::Serialize;
use sqlx::PgPool;

// ─── Types ───────────────────────────────────────────────────────────────────

/// Node classification for manufacturing graph visualization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// Mineral from ore reprocessing (Tritanium, Pyerite, etc.)
    RawMineral,
    /// Harvested from moons (Cadmium, Chromium, etc.)
    RawMoon,
    /// Planetary interaction output
    Pi,
    /// Composite moon reaction product
    Reaction,
    /// Advanced racial component (Magnetometric Sensor Cluster, etc.)
    Component,
    /// T1 base item used as input to T2
    T1Item,
    /// R.A.M. (Robotic Assembly Module)
    Ram,
    /// T2 manufactured product
    T2Product,
    /// Anything else / unknown
    Other,
}

// ─── classify (single) ──────────────────────────────────────────────────────

/// Classify a single type_id into its [`NodeKind`].
///
/// Issues up to 3 queries against the SDE tables. For bulk classification
/// prefer [`classify_batch`].
pub async fn classify(pool: &PgPool, type_id: i64) -> AppResult<NodeKind> {
    // 1. Fetch group/category info.
    let info = sqlx::query_as::<_, (String, i64, String)>(
        r#"
        SELECT g.name, g.category_id, t.name
        FROM sde_types t
        JOIN sde_groups g ON g.group_id = t.group_id
        WHERE t.type_id = $1
        "#,
    )
    .bind(type_id)
    .fetch_optional(pool)
    .await?;

    let Some((group_name, category_id, type_name)) = info else {
        return Ok(NodeKind::Other);
    };

    // Rule 1: RawMineral — category Material (4) AND group 'Mineral'
    if category_id == 4 && group_name == "Mineral" {
        return Ok(NodeKind::RawMineral);
    }

    // Rule 2: RawMoon — category Material (4) AND group 'Moon Materials'
    if category_id == 4 && group_name == "Moon Materials" {
        return Ok(NodeKind::RawMoon);
    }

    // Rule 6: R.A.M. — check before Component since both could match
    if group_name.starts_with("R.A.M.") || type_name.starts_with("R.A.M.") {
        return Ok(NodeKind::Ram);
    }

    // Rule 5: Component — group name contains 'Component'
    if group_name.contains("Component") {
        return Ok(NodeKind::Component);
    }

    // Rule 4: PI — category Planetary Commodities (43) or Planetary Resources (42)
    if category_id == 43 || category_id == 42 {
        return Ok(NodeKind::Pi);
    }

    // Rule 3: Reaction — produced by activity_id = 9
    let is_reaction: bool = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM sde_blueprint_products
            WHERE product_type_id = $1 AND activity_id = 9
        )
        "#,
    )
    .bind(type_id)
    .fetch_one(pool)
    .await?;

    if is_reaction {
        return Ok(NodeKind::Reaction);
    }

    // Rules 7/8: T2Product vs T1Item — check manufacturing
    let is_manufactured: bool = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM sde_blueprint_products
            WHERE product_type_id = $1 AND activity_id = 1
        )
        "#,
    )
    .bind(type_id)
    .fetch_one(pool)
    .await?;

    if is_manufactured {
        // T2 heuristic: at least one manufacturing input also has its own
        // manufacturing recipe (i.e. it's not a raw material but itself built).
        let is_t2: bool = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM sde_blueprint_products bp
                JOIN sde_blueprint_materials bm
                    ON bm.blueprint_type_id = bp.blueprint_type_id
                   AND bm.activity_id = bp.activity_id
                JOIN sde_blueprint_products bp2
                    ON bp2.product_type_id = bm.material_type_id
                   AND bp2.activity_id = 1
                WHERE bp.product_type_id = $1
                  AND bp.activity_id = 1
            )
            "#,
        )
        .bind(type_id)
        .fetch_one(pool)
        .await?;

        return Ok(if is_t2 {
            NodeKind::T2Product
        } else {
            NodeKind::T1Item
        });
    }

    // Rule 9: everything else
    Ok(NodeKind::Other)
}

// ─── classify_batch ─────────────────────────────────────────────────────────

/// Row returned from the bulk type-info query.
#[derive(Debug)]
struct TypeInfo {
    type_id: i64,
    group_name: String,
    category_id: i64,
    type_name: String,
}

impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for TypeInfo {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Self {
            type_id: row.try_get("type_id")?,
            group_name: row.try_get("group_name")?,
            category_id: row.try_get("category_id")?,
            type_name: row.try_get("type_name")?,
        })
    }
}

/// Classify a batch of type_ids efficiently using bulk queries.
///
/// Returns a map from each requested type_id to its [`NodeKind`]. Type_ids not
/// found in the SDE are classified as [`NodeKind::Other`].
pub async fn classify_batch(pool: &PgPool, type_ids: &[i64]) -> AppResult<HashMap<i64, NodeKind>> {
    if type_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut results: HashMap<i64, NodeKind> = HashMap::with_capacity(type_ids.len());

    // 1. Bulk fetch group/category info for all type_ids.
    let infos = sqlx::query_as::<_, TypeInfo>(
        r#"
        SELECT t.type_id,
               g.name AS group_name,
               g.category_id,
               t.name AS type_name
        FROM sde_types t
        JOIN sde_groups g ON g.group_id = t.group_id
        WHERE t.type_id = ANY($1)
        "#,
    )
    .bind(type_ids)
    .fetch_all(pool)
    .await?;

    let info_map: HashMap<i64, &TypeInfo> = infos.iter().map(|i| (i.type_id, i)).collect();

    // Collect type_ids that need further DB lookups.
    let mut need_reaction_check: Vec<i64> = Vec::new();
    let mut need_manufacturing_check: Vec<i64> = Vec::new();

    for &tid in type_ids {
        let Some(info) = info_map.get(&tid) else {
            results.insert(tid, NodeKind::Other);
            continue;
        };

        // Rule 1: RawMineral
        if info.category_id == 4 && info.group_name == "Mineral" {
            results.insert(tid, NodeKind::RawMineral);
            continue;
        }

        // Rule 2: RawMoon
        if info.category_id == 4 && info.group_name == "Moon Materials" {
            results.insert(tid, NodeKind::RawMoon);
            continue;
        }

        // Rule 6: R.A.M.
        if info.group_name.starts_with("R.A.M.") || info.type_name.starts_with("R.A.M.") {
            results.insert(tid, NodeKind::Ram);
            continue;
        }

        // Rule 5: Component
        if info.group_name.contains("Component") {
            results.insert(tid, NodeKind::Component);
            continue;
        }

        // Rule 4: PI
        if info.category_id == 43 || info.category_id == 42 {
            results.insert(tid, NodeKind::Pi);
            continue;
        }

        // Needs reaction check first, then potentially manufacturing check.
        need_reaction_check.push(tid);
    }

    // 2. Bulk check reaction products (activity_id = 9).
    if !need_reaction_check.is_empty() {
        let reaction_type_ids: Vec<i64> = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT DISTINCT product_type_id
            FROM sde_blueprint_products
            WHERE product_type_id = ANY($1) AND activity_id = 9
            "#,
        )
        .bind(&need_reaction_check)
        .fetch_all(pool)
        .await?;

        let reaction_set: std::collections::HashSet<i64> = reaction_type_ids.into_iter().collect();

        for &tid in &need_reaction_check {
            if reaction_set.contains(&tid) {
                results.insert(tid, NodeKind::Reaction);
            } else {
                need_manufacturing_check.push(tid);
            }
        }
    }

    // 3. Bulk check manufacturing (activity_id = 1) and T2 heuristic.
    if !need_manufacturing_check.is_empty() {
        // Find which of these are manufactured at all.
        let manufactured_type_ids: Vec<i64> = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT DISTINCT product_type_id
            FROM sde_blueprint_products
            WHERE product_type_id = ANY($1) AND activity_id = 1
            "#,
        )
        .bind(&need_manufacturing_check)
        .fetch_all(pool)
        .await?;

        let manufactured_set: std::collections::HashSet<i64> =
            manufactured_type_ids.into_iter().collect();

        // For manufactured items, check T2 heuristic: does at least one of their
        // manufacturing inputs also have a manufacturing recipe?
        let t2_type_ids: Vec<i64> = if manufactured_set.is_empty() {
            Vec::new()
        } else {
            let manufactured_vec: Vec<i64> = manufactured_set.iter().copied().collect();
            sqlx::query_scalar::<_, i64>(
                r#"
                SELECT DISTINCT bp.product_type_id
                FROM sde_blueprint_products bp
                JOIN sde_blueprint_materials bm
                    ON bm.blueprint_type_id = bp.blueprint_type_id
                   AND bm.activity_id = bp.activity_id
                JOIN sde_blueprint_products bp2
                    ON bp2.product_type_id = bm.material_type_id
                   AND bp2.activity_id = 1
                WHERE bp.product_type_id = ANY($1)
                  AND bp.activity_id = 1
                "#,
            )
            .bind(&manufactured_vec)
            .fetch_all(pool)
            .await?
        };

        let t2_set: std::collections::HashSet<i64> = t2_type_ids.into_iter().collect();

        for &tid in &need_manufacturing_check {
            if manufactured_set.contains(&tid) {
                if t2_set.contains(&tid) {
                    results.insert(tid, NodeKind::T2Product);
                } else {
                    results.insert(tid, NodeKind::T1Item);
                }
            } else {
                results.insert(tid, NodeKind::Other);
            }
        }
    }

    Ok(results)
}
