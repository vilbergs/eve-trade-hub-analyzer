//! Recipe lookup and recursive BOM (bill of materials) expansion for EVE Online manufacturing.

use std::collections::{HashMap, HashSet, VecDeque};

use eve_core::AppResult;
use serde::Serialize;
use sqlx::PgPool;

// ─── Types ───────────────────────────────────────────────────────────────────

/// One input line in a recipe.
#[derive(Debug, Clone, Serialize)]
pub struct RecipeInput {
    pub type_id: i64,
    pub quantity: i64,
}

/// A complete recipe for producing a type_id.
#[derive(Debug, Clone, Serialize)]
pub struct Recipe {
    /// The blueprint (or schematic) that produces this item.
    pub blueprint_type_id: i64,
    /// Activity: 1=manufacturing, 9=reaction, -1=PI
    pub activity_id: i32,
    /// Inputs needed per single run.
    pub inputs: Vec<RecipeInput>,
    /// Quantity produced per run.
    pub output_quantity: i64,
    /// Base time in seconds per run.
    pub time_secs: i64,
}

/// One line in a flattened BOM.
#[derive(Debug, Clone, Serialize)]
pub struct BomLine {
    pub type_id: i64,
    /// Total quantity needed (rounded up to integer).
    pub quantity: i64,
    /// Whether this item will be built (true) or bought (false).
    pub is_built: bool,
}

/// Result of a BOM expansion.
#[derive(Debug, Clone, Serialize)]
pub struct BomResult {
    /// All leaf materials to buy.
    pub buy: Vec<BomLine>,
    /// All intermediate items to build (in dependency order, deepest first).
    pub build: Vec<BomLine>,
}

// ─── recipe_for ──────────────────────────────────────────────────────────────

/// Look up the recipe for producing `product_type_id`.
///
/// Checks manufacturing/reaction blueprints first, then PI schematics.
/// Returns `None` if no recipe exists (raw material).
pub async fn recipe_for(pool: &PgPool, product_type_id: i64) -> AppResult<Option<Recipe>> {
    // 1. Check blueprint products (manufacturing or reaction).
    let bp_row = sqlx::query_as::<_, (i64, i32, i64)>(
        r#"
        SELECT bp.blueprint_type_id, bp.activity_id::INT, bp.quantity::BIGINT
        FROM sde_blueprint_products bp
        WHERE bp.product_type_id = $1
          AND bp.activity_id IN (1, 9)
        LIMIT 1
        "#,
    )
    .bind(product_type_id)
    .fetch_optional(pool)
    .await?;

    if let Some((blueprint_type_id, activity_id, output_quantity)) = bp_row {
        // Fetch materials for this blueprint + activity.
        let mat_rows = sqlx::query_as::<_, (i64, i64)>(
            r#"
            SELECT material_type_id, quantity::BIGINT
            FROM sde_blueprint_materials
            WHERE blueprint_type_id = $1
              AND activity_id = $2
            "#,
        )
        .bind(blueprint_type_id)
        .bind(activity_id)
        .fetch_all(pool)
        .await?;

        let inputs: Vec<RecipeInput> = mat_rows
            .into_iter()
            .map(|(type_id, quantity)| RecipeInput { type_id, quantity })
            .collect();

        // Fetch time.
        let time_secs = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT time_secs::BIGINT
            FROM sde_blueprint_activities
            WHERE blueprint_type_id = $1
              AND activity_id = $2
            "#,
        )
        .bind(blueprint_type_id)
        .bind(activity_id)
        .fetch_optional(pool)
        .await?
        .unwrap_or(0);

        return Ok(Some(Recipe {
            blueprint_type_id,
            activity_id,
            inputs,
            output_quantity,
            time_secs,
        }));
    }

    // 2. Check PI schematics.
    let pi_row = sqlx::query_as::<_, (i64, i64)>(
        r#"
        SELECT schematic_id::BIGINT, quantity::BIGINT
        FROM sde_planet_schematic_types
        WHERE type_id = $1
          AND is_input = false
        LIMIT 1
        "#,
    )
    .bind(product_type_id)
    .fetch_optional(pool)
    .await?;

    if let Some((schematic_id, output_quantity)) = pi_row {
        // Fetch PI inputs.
        let pi_inputs = sqlx::query_as::<_, (i64, i64)>(
            r#"
            SELECT type_id, quantity::BIGINT
            FROM sde_planet_schematic_types
            WHERE schematic_id = $1
              AND is_input = true
            "#,
        )
        .bind(schematic_id)
        .fetch_all(pool)
        .await?;

        let inputs: Vec<RecipeInput> = pi_inputs
            .into_iter()
            .map(|(type_id, quantity)| RecipeInput { type_id, quantity })
            .collect();

        // Fetch cycle time.
        let time_secs = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT cycle_time_secs::BIGINT
            FROM sde_planet_schematics
            WHERE schematic_id = $1
            "#,
        )
        .bind(schematic_id)
        .fetch_optional(pool)
        .await?
        .unwrap_or(0);

        return Ok(Some(Recipe {
            blueprint_type_id: schematic_id,
            activity_id: -1,
            inputs,
            output_quantity,
            time_secs,
        }));
    }

    // No recipe found — raw material.
    Ok(None)
}

// ─── bom_for ─────────────────────────────────────────────────────────────────

/// Work-queue item for iterative BOM expansion.
struct WorkItem {
    type_id: i64,
    quantity_needed: f64,
}

/// Expand the bill of materials for `product_type_id` × `runs`.
///
/// - `me_percent`: material efficiency bonus (0–10 for blueprints). Applied as
///   `quantity * (1.0 - me_percent / 100.0)`, result ceiled, minimum 1.
/// - `built_set`: type_ids the caller wants to manufacture rather than buy.
///   The root product is always built regardless of membership.
pub async fn bom_for(
    pool: &PgPool,
    product_type_id: i64,
    runs: i64,
    me_percent: f64,
    built_set: &HashSet<i64>,
) -> AppResult<BomResult> {
    let me_factor = 1.0 - (me_percent / 100.0);

    // Cache of recipes fetched during expansion.
    let mut recipe_cache: HashMap<i64, Option<Recipe>> = HashMap::new();

    // Accumulated quantities: type_id → total quantity needed.
    let mut buy_totals: HashMap<i64, f64> = HashMap::new();
    let mut build_totals: HashMap<i64, f64> = HashMap::new();
    // Track build order depth (deepest first for topological sort).
    let mut build_depth: HashMap<i64, usize> = HashMap::new();

    // BFS work queue: (work_item, depth, is_root).
    let mut queue: VecDeque<(WorkItem, usize, bool)> = VecDeque::new();

    // Seed with the root product.
    queue.push_back((
        WorkItem {
            type_id: product_type_id,
            quantity_needed: runs as f64,
        },
        0,
        true,
    ));

    while let Some((item, depth, is_root)) = queue.pop_front() {
        let should_build = is_root || built_set.contains(&item.type_id);

        if !should_build {
            // This is a buy item — accumulate and stop.
            *buy_totals.entry(item.type_id).or_insert(0.0) += item.quantity_needed;
            continue;
        }

        // Look up recipe (cached).
        if let std::collections::hash_map::Entry::Vacant(e) = recipe_cache.entry(item.type_id) {
            let recipe = recipe_for(pool, item.type_id).await?;
            e.insert(recipe);
        }

        let recipe = recipe_cache.get(&item.type_id).unwrap();

        match recipe {
            None => {
                // No recipe exists — must buy even if in built_set.
                *buy_totals.entry(item.type_id).or_insert(0.0) += item.quantity_needed;
            }
            Some(recipe) => {
                // Record as a build item.
                if !is_root {
                    *build_totals.entry(item.type_id).or_insert(0.0) += item.quantity_needed;
                    let entry = build_depth.entry(item.type_id).or_insert(0);
                    if depth > *entry {
                        *entry = depth;
                    }
                }

                // Calculate how many runs of this recipe we need.
                let runs_needed = (item.quantity_needed / recipe.output_quantity as f64).ceil();

                // Expand inputs.
                for input in &recipe.inputs {
                    let raw_qty = input.quantity as f64 * runs_needed;
                    let me_qty = (raw_qty * me_factor).ceil().max(1.0);

                    queue.push_back((
                        WorkItem {
                            type_id: input.type_id,
                            quantity_needed: me_qty,
                        },
                        depth + 1,
                        false,
                    ));
                }
            }
        }
    }

    // Assemble results.
    let mut buy: Vec<BomLine> = buy_totals
        .into_iter()
        .map(|(type_id, qty)| BomLine {
            type_id,
            quantity: (qty.ceil() as i64).max(1),
            is_built: false,
        })
        .collect();

    // Sort buy list by type_id for deterministic output.
    buy.sort_by_key(|b| b.type_id);

    // Sort build list by depth descending (deepest dependencies first).
    let mut build: Vec<BomLine> = build_totals
        .into_iter()
        .map(|(type_id, qty)| BomLine {
            type_id,
            quantity: (qty.ceil() as i64).max(1),
            is_built: true,
        })
        .collect();

    build.sort_by(|a, b| {
        let da = build_depth.get(&a.type_id).copied().unwrap_or(0);
        let db = build_depth.get(&b.type_id).copied().unwrap_or(0);
        db.cmp(&da).then(a.type_id.cmp(&b.type_id))
    });

    Ok(BomResult { buy, build })
}
