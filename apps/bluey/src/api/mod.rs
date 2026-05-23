use std::sync::Arc;

use axum::Router;
use sqlx::PgPool;

pub mod bom;
pub mod chain;
pub mod products;

pub struct AppState {
    pub pool: PgPool,
    pub http: reqwest::Client,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(products::router(state.clone()))
        .merge(chain::router(state.clone()))
        .merge(bom::router(state))
}
