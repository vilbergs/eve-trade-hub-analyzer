//! DB-backed tests for the refresh-token side of Phase 3c.
//!
//! These talk to a real Postgres because the code under test reads
//! `characters` rows, persists rotated refresh tokens, and flips
//! `status` to `needs_reauth` on `invalid_grant`. The token + JWKS
//! halves of EVE SSO are mocked via `wiremock`.
//!
//! Tests are skipped when `DATABASE_URL` is unset, so `cargo test` stays
//! green on a host with no Postgres. CI / a local `docker compose up -d`
//! turns them on.
//!
//! Each test scopes its writes to a unique `character_id` and cleans up
//! before/after to keep multi-test runs hermetic.

use std::time::Duration;

use eve_trade_hub_analyzer::db;
use eve_trade_hub_analyzer::esi::auth::{AccessTokenCache, AuthEndpoints, get_access_token};
use eve_trade_hub_analyzer::{AppError, Config};
use serde_json::json;
use sqlx::PgPool;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SEEDED_CHARACTER_REFRESH: i64 = 9_000_001;
const SEEDED_CHARACTER_INVALID: i64 = 9_000_002;

async fn db_pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    // DATABASE_URL is set, so the operator wants to run the test for real.
    // Connection / migration failures past this point should fail loudly
    // rather than silently skip.
    let pool = sqlx::PgPool::connect(&url)
        .await
        .expect("DATABASE_URL set but connection failed");
    db::MIGRATOR
        .run(&pool)
        .await
        .expect("migrations failed against the test database");
    Some(pool)
}

fn endpoints(server: &MockServer) -> AuthEndpoints {
    AuthEndpoints {
        authorize_url: format!("{}/v2/oauth/authorize", server.uri()),
        token_url: format!("{}/v2/oauth/token", server.uri()),
        jwks_url: format!("{}/oauth/jwks", server.uri()),
        esi_base: format!("{}/latest", server.uri()),
        allowed_issuers: vec!["test".into()],
    }
}

fn test_config() -> Config {
    Config {
        database_url: std::env::var("DATABASE_URL").unwrap(),
        eve_client_id: "test_client_id".into(),
        eve_client_secret: "test_client_secret".into(),
        eve_callback_url: "http://localhost:5173/auth/callback".into(),
        eve_user_agent: "eve-trade-hub-analyzer-tests/0.1.0".into(),
        jita_region_id: 10_000_002,
        poll_interval: Duration::from_secs(300),
        google: None,
    }
}

async fn seed_character(pool: &PgPool, character_id: i64, refresh_token: &str) {
    sqlx::query("DELETE FROM characters WHERE character_id = $1")
        .bind(character_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO characters \
            (character_id, character_name, corporation_id, owner_hash, \
             refresh_token, scopes, status, last_refreshed_at) \
         VALUES ($1, $2, $3, $4, $5, $6, 'active', now())",
    )
    .bind(character_id)
    .bind("Test Pilot")
    .bind(100_i64)
    .bind("ownerhash")
    .bind(refresh_token)
    .bind(vec![
        "esi-markets.structure_markets.v1".to_string(),
        "esi-universe.read_structures.v1".to_string(),
    ])
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn refresh_returns_new_token_rotates_refresh_and_caches() {
    let Some(pool) = db_pool().await else {
        eprintln!(
            "DATABASE_URL not set — skipping refresh_returns_new_token_rotates_refresh_and_caches"
        );
        return;
    };
    seed_character(&pool, SEEDED_CHARACTER_REFRESH, "first_refresh_token").await;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/oauth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "fresh_access_token_value",
            "refresh_token": "rotated_refresh_token",
            "expires_in": 1200_i32,
            "token_type": "Bearer"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let cache = AccessTokenCache::new();
    let cfg = test_config();
    let eps = endpoints(&server);
    let http = reqwest::Client::new();

    let token = get_access_token(&cache, &cfg, &eps, &pool, &http, SEEDED_CHARACTER_REFRESH)
        .await
        .unwrap();
    assert_eq!(token, "fresh_access_token_value");

    // The stored refresh token rotated.
    let (current_refresh,): (String,) =
        sqlx::query_as("SELECT refresh_token FROM characters WHERE character_id = $1")
            .bind(SEEDED_CHARACTER_REFRESH)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(current_refresh, "rotated_refresh_token");

    // Second call within TTL is served from cache (still expect 1 token POST).
    let token2 = get_access_token(&cache, &cfg, &eps, &pool, &http, SEEDED_CHARACTER_REFRESH)
        .await
        .unwrap();
    assert_eq!(token2, "fresh_access_token_value");

    // Cleanup.
    sqlx::query("DELETE FROM characters WHERE character_id = $1")
        .bind(SEEDED_CHARACTER_REFRESH)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn invalid_grant_flips_status_to_needs_reauth() {
    let Some(pool) = db_pool().await else {
        eprintln!("DATABASE_URL not set — skipping invalid_grant_flips_status_to_needs_reauth");
        return;
    };
    seed_character(&pool, SEEDED_CHARACTER_INVALID, "expired_refresh").await;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/oauth/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "invalid_grant",
            "error_description": "refresh token has expired"
        })))
        .mount(&server)
        .await;

    let cache = AccessTokenCache::new();
    let cfg = test_config();
    let eps = endpoints(&server);
    let http = reqwest::Client::new();

    let err = get_access_token(&cache, &cfg, &eps, &pool, &http, SEEDED_CHARACTER_INVALID)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Auth(_)), "got: {err:?}");

    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM characters WHERE character_id = $1")
            .bind(SEEDED_CHARACTER_INVALID)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "needs_reauth");

    // Cleanup.
    sqlx::query("DELETE FROM characters WHERE character_id = $1")
        .bind(SEEDED_CHARACTER_INVALID)
        .execute(&pool)
        .await
        .unwrap();
}
