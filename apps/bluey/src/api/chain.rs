//! GET /chain/:type_id — full manufacturing dependency graph for a product.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use eve_industry::{NodeKind, classify_batch, recipe_for};

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
    let mut queue: VecDeque<i64> = VecDeque::new();
    let mut edges: Vec<ChainEdge> = Vec::new();
    let mut recipe_info: HashMap<i64, (i32, i64, i64)> = HashMap::new();

    // Types that should never have their recipes expanded (treated as leaf nodes)
    let raw_moon_ids: HashSet<i64> = sqlx::query_scalar::<_, i64>(
        "SELECT t.type_id FROM sde_types t JOIN sde_groups g ON g.group_id = t.group_id WHERE g.category_id = 4 AND g.name = 'Moon Materials'"
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .collect();

    queue.push_back(type_id);
    visited.insert(type_id);

    while let Some(current) = queue.pop_front() {
        // Skip recipe lookup for raw moon materials
        if raw_moon_ids.contains(&current) {
            continue;
        }
        match recipe_for(pool, current).await {
            Ok(Some(recipe)) => {
                recipe_info.insert(
                    current,
                    (recipe.activity_id, recipe.output_quantity, recipe.time_secs),
                );
                for input in &recipe.inputs {
                    edges.push(ChainEdge {
                        from_type_id: input.type_id,
                        to_type_id: current,
                        quantity: input.quantity,
                    });
                    if visited.insert(input.type_id) {
                        queue.push_back(input.type_id);
                    }
                }
            }
            Ok(None) => {}
            Err(e) => {
                tracing::error!("recipe_for({current}) failed: {e}");
                return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
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
