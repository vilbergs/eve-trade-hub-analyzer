//! GET /products — list of buildable products for the blueprint picker.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::AppState;

#[derive(Debug, Deserialize)]
pub struct ProductsQuery {
    pub q: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ProductEntry {
    pub type_id: i64,
    pub name: String,
    pub group_name: String,
    pub category_name: String,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/products", get(list_products))
        .with_state(state)
}

async fn list_products(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ProductsQuery>,
) -> Result<Json<Vec<ProductEntry>>, axum::http::StatusCode> {
    let limit = params.limit.unwrap_or(50).min(200);
    let search = params.q.unwrap_or_default();

    let rows = if search.is_empty() {
        sqlx::query_as::<_, (i64, String, String, String)>(
            r#"
            SELECT DISTINCT bp.product_type_id,
                   t.name AS type_name,
                   g.name AS group_name,
                   c.name AS category_name
            FROM sde_blueprint_products bp
            JOIN sde_types t ON t.type_id = bp.product_type_id
            JOIN sde_groups g ON g.group_id = t.group_id
            JOIN sde_categories c ON c.category_id = g.category_id
            WHERE bp.activity_id = 1
            ORDER BY t.name
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&state.pool)
        .await
    } else {
        sqlx::query_as::<_, (i64, String, String, String)>(
            r#"
            SELECT DISTINCT bp.product_type_id,
                   t.name AS type_name,
                   g.name AS group_name,
                   c.name AS category_name
            FROM sde_blueprint_products bp
            JOIN sde_types t ON t.type_id = bp.product_type_id
            JOIN sde_groups g ON g.group_id = t.group_id
            JOIN sde_categories c ON c.category_id = g.category_id
            WHERE bp.activity_id = 1
              AND t.name ILIKE '%' || $1 || '%'
            ORDER BY t.name
            LIMIT $2
            "#,
        )
        .bind(&search)
        .bind(limit)
        .fetch_all(&state.pool)
        .await
    };

    match rows {
        Ok(rows) => Ok(Json(
            rows.into_iter()
                .map(|(type_id, name, group_name, category_name)| ProductEntry {
                    type_id,
                    name,
                    group_name,
                    category_name,
                })
                .collect(),
        )),
        Err(e) => {
            tracing::error!("products query failed: {e}");
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
