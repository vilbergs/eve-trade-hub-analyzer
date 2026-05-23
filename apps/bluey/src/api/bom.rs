//! POST /bom — compute bill of materials with prices.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use eve_industry::bom_for;
use eve_pricing::{PriceBasis, prices_for};

use super::AppState;

#[derive(Debug, Deserialize)]
pub struct BomRequest {
    pub product_type_id: i64,
    pub runs: i64,
    pub me_percent: f64,
    pub built_type_ids: Vec<i64>,
    pub price_basis: Option<PriceBasis>,
}

#[derive(Debug, Serialize)]
pub struct BomResponse {
    pub buy: Vec<BomLineWithPrice>,
    pub build: Vec<BomLineWithPrice>,
    pub total_cost: f64,
}

#[derive(Debug, Serialize)]
pub struct BomLineWithPrice {
    pub type_id: i64,
    pub quantity: i64,
    pub is_built: bool,
    pub unit_price: Option<f64>,
    pub line_cost: Option<f64>,
    pub name: Option<String>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/bom", post(compute_bom))
        .with_state(state)
}

async fn compute_bom(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BomRequest>,
) -> Result<Json<BomResponse>, axum::http::StatusCode> {
    let pool = &state.pool;
    let built_set: HashSet<i64> = req.built_type_ids.into_iter().collect();

    let bom = bom_for(pool, req.product_type_id, req.runs, req.me_percent, &built_set)
        .await
        .map_err(|e| {
            tracing::error!("bom_for failed: {e}");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Default: Jita sell min.
    let basis = req.price_basis.unwrap_or(PriceBasis::SellMin {
        location_id: 60003760,
    });

    let all_type_ids: Vec<i64> = bom
        .buy
        .iter()
        .chain(bom.build.iter())
        .map(|l| l.type_id)
        .collect();

    let prices = prices_for(pool, &all_type_ids, &basis)
        .await
        .unwrap_or_default();

    let names: HashMap<i64, String> = if !all_type_ids.is_empty() {
        sqlx::query_as::<_, (i64, String)>(
            "SELECT type_id, name FROM sde_types WHERE type_id = ANY($1)",
        )
        .bind(&all_type_ids)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect()
    } else {
        HashMap::new()
    };

    let mut total_cost = 0.0;

    let mut to_line = |line: &eve_industry::BomLine| -> BomLineWithPrice {
        let unit_price = prices.get(&line.type_id).copied();
        let line_cost = unit_price.map(|p| p * line.quantity as f64);
        if let Some(c) = line_cost {
            total_cost += c;
        }
        BomLineWithPrice {
            type_id: line.type_id,
            quantity: line.quantity,
            is_built: line.is_built,
            unit_price,
            line_cost,
            name: names.get(&line.type_id).cloned(),
        }
    };

    let buy: Vec<BomLineWithPrice> = bom.buy.iter().map(|l| to_line(l)).collect();
    let build: Vec<BomLineWithPrice> = bom.build.iter().map(|l| to_line(l)).collect();

    Ok(Json(BomResponse {
        buy,
        build,
        total_cost,
    }))
}
