use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing_subscriber::EnvFilter;

mod api;
mod config;

pub use config::Config;

#[tokio::main]
async fn main() -> eve_core::AppResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::from_env()?;

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;

    // Cache raw moon material type IDs at startup.
    let raw_moon_ids: HashSet<i64> = sqlx::query_scalar::<_, i64>(
        "SELECT t.type_id FROM sde_types t JOIN sde_groups g ON g.group_id = t.group_id WHERE g.category_id = 4 AND g.name = 'Moon Materials'"
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .collect();

    tracing::info!("Cached {} raw moon material IDs", raw_moon_ids.len());

    let state = Arc::new(api::AppState {
        pool,
        http: reqwest::Client::new(),
        raw_moon_ids,
    });

    let api_routes = api::router(state.clone());

    // Serve the frontend SPA — in development, use Vite dev server instead.
    let frontend_dir =
        std::env::var("FRONTEND_DIR").unwrap_or_else(|_| "./frontend/dist".to_string());

    let app = Router::new()
        .nest("/api", api_routes)
        .fallback_service(ServeDir::new(&frontend_dir).append_index_html_on_directories(true))
        .layer(CorsLayer::permissive());

    let addr: SocketAddr = config
        .listen_addr
        .parse()
        .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], 3001)));

    tracing::info!("Bluey listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
