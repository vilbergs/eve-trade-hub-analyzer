//! GET /chain/:type_id — full manufacturing dependency graph for a product.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use eve_industry::{NodeKind, classify_batch, recipes_for_batch};

use super::AppState;

#[derive(Debug, Serialize)]
pub struct ChainResponse {
    pub focal_type_id: i64,
    pub nodes: Vec<ChainNode>,
    pub edges: Vec<ChainEdge>,
}

#[derive(Debug, Serialize)]
pub struct ChainNode {
    pub type_id: i64,
    pub name: String,
    pub kind: NodeKind,
    pub has_recipe: bool,
    pub activity_id: i32,
    pub output_quantity: i64,
    pub time_secs: i64,
}

#[derive(Debug, Serialize)]
pub struct ChainEdge {
    pub from_type_id: i64,
    pub to_type_id: i64,
    pub quantity: i64,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/chain/:type_id", get(get_chain))
        .with_state(state)
}

async fn get_chain(
    State(state): State<Arc<AppState>>,
    Path(type_id): Path<i64>,
) -> Result<Json<ChainResponse>, axum::http::StatusCode> {
    let pool = &state.pool;

    // BFS to discover all nodes in the manufacturing tree.
    let mut visited: HashSet<i64> = HashSet::new();
    let mut edges: Vec<ChainEdge> = Vec::new();
    let mut recipe_info: HashMap<i64, (i32, i64, i64)> = HashMap::new();

    // Use cached raw moon IDs from AppState.
    let raw_moon_ids = &state.raw_moon_ids;

    // Level-based BFS: process one frontier at a time with batch recipe fetches.
    let mut frontier: Vec<i64> = vec![type_id];
    visited.insert(type_id);

    while !frontier.is_empty() {
        // Filter out raw moon materials — they are leaf nodes.
        let to_fetch: Vec<i64> = frontier
            .iter()
            .filter(|id| !raw_moon_ids.contains(id))
            .copied()
            .collect();

        // Batch-fetch all recipes for this frontier level.
        let recipes = recipes_for_batch(pool, &to_fetch).await.map_err(|e| {
            tracing::error!("recipes_for_batch failed: {e}");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let mut next_frontier: Vec<i64> = Vec::new();

        for current in &frontier {
            if raw_moon_ids.contains(current) {
                continue;
            }
            if let Some(recipe) = recipes.get(current) {
                recipe_info.insert(
                    *current,
                    (recipe.activity_id, recipe.output_quantity, recipe.time_secs),
                );
                for input in &recipe.inputs {
                    edges.push(ChainEdge {
                        from_type_id: input.type_id,
                        to_type_id: *current,
                        quantity: input.quantity,
                    });
                    if visited.insert(input.type_id) {
                        next_frontier.push(input.type_id);
                    }
                }
            }
        }

        frontier = next_frontier;
    }

    // Classify all discovered type_ids.
    let all_type_ids: Vec<i64> = visited.iter().copied().collect();
    let classifications = classify_batch(pool, &all_type_ids).await.map_err(|e| {
        tracing::error!("classify_batch failed: {e}");
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Fetch names.
    let names: HashMap<i64, String> = sqlx::query_as::<_, (i64, String)>(
        "SELECT type_id, name FROM sde_types WHERE type_id = ANY($1)",
    )
    .bind(&all_type_ids)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("name fetch failed: {e}");
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?
    .into_iter()
    .collect();

    let nodes: Vec<ChainNode> = all_type_ids
        .iter()
        .map(|&tid| {
            let (activity_id, output_quantity, time_secs) =
                recipe_info.get(&tid).copied().unwrap_or((0, 0, 0));
            ChainNode {
                type_id: tid,
                name: names
                    .get(&tid)
                    .cloned()
                    .unwrap_or_else(|| format!("Type {tid}")),
                kind: classifications
                    .get(&tid)
                    .copied()
                    .unwrap_or(NodeKind::Other),
                has_recipe: recipe_info.contains_key(&tid),
                activity_id,
                output_quantity,
                time_secs,
            }
        })
        .collect();

    Ok(Json(ChainResponse {
        focal_type_id: type_id,
        nodes,
        edges,
    }))
}
