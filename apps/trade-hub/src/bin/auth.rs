//! `auth` binary — one-shot EVE SSO flow.
//!
//! Boots a tiny axum server on the host:port from `EVE_CALLBACK_URL`,
//! prints the URL to visit, walks the PKCE login when the user opens it,
//! persists the linked character, then shuts down. Two routes:
//!
//! - `GET /`              — start a fresh login (new PKCE + CSRF), redirect.
//! - `GET /auth/callback` — validate state, exchange code, persist.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Query, State};
use axum::response::{Html, Redirect};
use axum::routing::get;
use clap::Parser;
use eve_core::{AppError, AppResult};
use eve_trade_hub_analyzer::esi::auth::{
    AuthEndpoints, CharacterRow, LoginStart, complete_login, start_login,
};
use eve_trade_hub_analyzer::{Config, db};
use eve_core::telemetry;
use oauth2::{CsrfToken, PkceCodeVerifier};
use serde::Deserialize;
use sqlx::PgPool;
use tokio::sync::{Mutex, oneshot};
use tracing::{error, info};

/// Run the EVE SSO login flow and persist a refresh token.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init();
    let _args = Args::parse();
    let config = Config::from_env()?;
    let pool = db::build_pool(&config).await?;
    let http = reqwest::Client::builder()
        .user_agent(&config.eve_user_agent)
        .gzip(true)
        .build()?;

    let addr = parse_callback_addr(&config.eve_callback_url)?;
    let (completion_tx, completion_rx) = oneshot::channel::<AppResult<CharacterRow>>();

    let state = AppState {
        config: Arc::new(config),
        endpoints: Arc::new(AuthEndpoints::production()),
        pool: pool.clone(),
        http: http.clone(),
        pending: Arc::new(Mutex::new(None)),
        completion_tx: Arc::new(Mutex::new(Some(completion_tx))),
    };

    let app = Router::new()
        .route("/", get(root_handler))
        .route("/auth/callback", get(callback_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    println!();
    println!("Open this URL in your browser to begin the EVE SSO flow:");
    println!("    http://{addr}/");
    println!();

    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                shutdown_rx.await.ok();
            })
            .await
    });

    let outcome = completion_rx
        .await
        .map_err(|_| AppError::Auth("callback completed without sending result".into()))??;
    let _ = shutdown_tx.send(());
    server.await??;

    println!();
    println!(
        "Linked character: {} (id={}, corp={})",
        outcome.character_name, outcome.character_id, outcome.corporation_id
    );

    Ok(())
}

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
    endpoints: Arc<AuthEndpoints>,
    pool: PgPool,
    http: reqwest::Client,
    pending: Arc<Mutex<Option<Pending>>>,
    completion_tx: Arc<Mutex<Option<oneshot::Sender<AppResult<CharacterRow>>>>>,
}

struct Pending {
    verifier: PkceCodeVerifier,
    csrf: CsrfToken,
}

async fn root_handler(State(state): State<AppState>) -> Result<Redirect, Html<String>> {
    let LoginStart {
        authorize_url,
        verifier,
        state: csrf,
    } = match start_login(&state.config, &state.endpoints) {
        Ok(v) => v,
        Err(e) => return Err(error_page(&format!("Failed to start login: {e}"))),
    };

    {
        let mut guard = state.pending.lock().await;
        *guard = Some(Pending { verifier, csrf });
    }
    Ok(Redirect::temporary(authorize_url.as_str()))
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

async fn callback_handler(
    State(state): State<AppState>,
    Query(q): Query<CallbackQuery>,
) -> Html<String> {
    let pending = state.pending.lock().await.take();
    let Some(Pending { verifier, csrf }) = pending else {
        return error_page("No pending login. Visit / first.");
    };

    if csrf.secret() != &q.state {
        return error_page("CSRF state mismatch. Restart the flow from /.");
    }

    let result = complete_login(
        &state.config,
        &state.endpoints,
        &state.pool,
        &state.http,
        verifier,
        q.code,
    )
    .await;

    let page = match &result {
        Ok(row) => Html(format!(
            "<!doctype html><html><body style='font-family: system-ui; max-width: 36rem; margin: 4rem auto'>\
                <h1>Linked.</h1>\
                <p>Linked character <strong>{}</strong> (id={}, corp={}). You can close this tab.</p>\
             </body></html>",
            html_escape(&row.character_name),
            row.character_id,
            row.corporation_id,
        )),
        Err(e) => {
            error!(error = %e, "complete_login failed");
            error_page(&format!("Login failed: {e}"))
        }
    };

    if let Some(tx) = state.completion_tx.lock().await.take() {
        let _ = tx.send(result);
    } else {
        info!("ignoring extra callback");
    }
    page
}

fn error_page(msg: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><html><body style='font-family: system-ui; max-width: 36rem; margin: 4rem auto'>\
            <h1>Login error</h1><p>{}</p></body></html>",
        html_escape(msg)
    ))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn parse_callback_addr(url: &str) -> AppResult<SocketAddr> {
    let parsed =
        url::Url::parse(url).map_err(|e| AppError::Config(format!("EVE_CALLBACK_URL: {e}")))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::Config("EVE_CALLBACK_URL has no host".into()))?;
    let port = parsed.port().unwrap_or(5173);
    format!("{host}:{port}")
        .parse::<SocketAddr>()
        .map_err(|e| AppError::Config(format!("EVE_CALLBACK_URL not a socket addr: {e}")))
}
