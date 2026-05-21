//! EVE SSO (PKCE) login and access-token refresh.
//!
//! Talks to three EVE endpoints — authorize, token, JWKS — plus
//! `GET /characters/{id}/` to recover the corporation_id (which isn't in
//! the JWT). Access tokens are cached in process memory keyed by
//! character_id; refresh tokens are persisted plaintext per
//! ADDENDUM.md §4.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{DecodingKey, Validation};
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointNotSet, EndpointSet,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use serde::Deserialize;
use sqlx::PgPool;
use tokio::sync::Mutex;
use tracing::{info, instrument, warn};

use crate::Config;
use eve_core::{AppError, AppResult};

/// Two scopes — and only these two — per PROMPT.md §6.
const SCOPES: [&str; 2] = [
    "esi-universe.read_structures.v1",
    "esi-markets.structure_markets.v1",
];

const PRODUCTION_AUTHORIZE_URL: &str = "https://login.eveonline.com/v2/oauth/authorize";
const PRODUCTION_TOKEN_URL: &str = "https://login.eveonline.com/v2/oauth/token";
const PRODUCTION_JWKS_URL: &str = "https://login.eveonline.com/oauth/jwks";
const ESI_BASE: &str = "https://esi.evetech.net/latest";

/// Endpoint set; production() yields the real EVE URLs, tests substitute a
/// wiremock host.
#[derive(Debug, Clone)]
pub struct AuthEndpoints {
    pub authorize_url: String,
    pub token_url: String,
    pub jwks_url: String,
    pub esi_base: String,
    pub allowed_issuers: Vec<String>,
}

impl AuthEndpoints {
    pub fn production() -> Self {
        Self {
            authorize_url: PRODUCTION_AUTHORIZE_URL.into(),
            token_url: PRODUCTION_TOKEN_URL.into(),
            jwks_url: PRODUCTION_JWKS_URL.into(),
            esi_base: ESI_BASE.into(),
            allowed_issuers: vec![
                "login.eveonline.com".into(),
                "https://login.eveonline.com".into(),
            ],
        }
    }
}

type OauthClient = BasicClient<
    EndpointSet,    // auth_uri
    EndpointNotSet, // device_auth_uri
    EndpointNotSet, // introspection_uri
    EndpointNotSet, // revocation_uri
    EndpointSet,    // token_uri
>;

fn build_oauth(config: &Config, endpoints: &AuthEndpoints) -> AppResult<OauthClient> {
    let auth = AuthUrl::new(endpoints.authorize_url.clone())
        .map_err(|e| AppError::Auth(format!("authorize_url invalid: {e}")))?;
    let token = TokenUrl::new(endpoints.token_url.clone())
        .map_err(|e| AppError::Auth(format!("token_url invalid: {e}")))?;
    let redirect = RedirectUrl::new(config.eve_callback_url.clone())
        .map_err(|e| AppError::Auth(format!("redirect_url invalid: {e}")))?;
    Ok(
        BasicClient::new(ClientId::new(config.eve_client_id.clone()))
            .set_client_secret(ClientSecret::new(config.eve_client_secret.clone()))
            .set_auth_uri(auth)
            .set_token_uri(token)
            .set_redirect_uri(redirect),
    )
}

#[derive(Debug)]
pub struct LoginStart {
    pub authorize_url: url::Url,
    pub verifier: PkceCodeVerifier,
    pub state: CsrfToken,
}

/// Step 1: generate a PKCE verifier + CSRF token and build the authorize URL
/// the user opens in their browser.
pub fn start_login(config: &Config, endpoints: &AuthEndpoints) -> AppResult<LoginStart> {
    let client = build_oauth(config, endpoints)?;
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let (url, csrf) = client
        .authorize_url(CsrfToken::new_random)
        .add_scopes(SCOPES.iter().map(|s| Scope::new((*s).to_owned())))
        .set_pkce_challenge(challenge)
        .url();
    Ok(LoginStart {
        authorize_url: url,
        verifier,
        state: csrf,
    })
}

#[derive(Debug, Clone)]
pub struct CharacterRow {
    pub character_id: i64,
    pub character_name: String,
    pub corporation_id: i64,
    pub owner_hash: String,
    pub scopes: Vec<String>,
}

/// Step 2: exchange the authorization code for tokens, verify the access JWT
/// against JWKS, fetch the corporation_id, and upsert `characters`.
#[instrument(skip_all)]
pub async fn complete_login(
    config: &Config,
    endpoints: &AuthEndpoints,
    pool: &PgPool,
    http: &reqwest::Client,
    verifier: PkceCodeVerifier,
    code: String,
) -> AppResult<CharacterRow> {
    let client = build_oauth(config, endpoints)?;
    let token = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(verifier)
        .request_async(http)
        .await
        .map_err(|e| AppError::Auth(format!("token exchange: {e}")))?;

    let access = token.access_token().secret().to_owned();
    let refresh = token
        .refresh_token()
        .ok_or_else(|| AppError::Auth("token response missing refresh_token".into()))?
        .secret()
        .to_owned();

    let claims = verify_jwt(http, endpoints, &access).await?;
    let character_id = parse_character_id(&claims.sub)?;
    let scopes_vec = claims.scp.into_vec();
    let corporation_id = fetch_corporation_id(http, endpoints, character_id).await?;

    sqlx::query(
        "INSERT INTO characters \
            (character_id, character_name, corporation_id, owner_hash, \
             refresh_token, scopes, status, last_refreshed_at) \
         VALUES ($1, $2, $3, $4, $5, $6, 'active', now()) \
         ON CONFLICT (character_id) DO UPDATE SET \
            character_name = EXCLUDED.character_name, \
            corporation_id = EXCLUDED.corporation_id, \
            owner_hash = EXCLUDED.owner_hash, \
            refresh_token = EXCLUDED.refresh_token, \
            scopes = EXCLUDED.scopes, \
            status = 'active', \
            last_refreshed_at = now()",
    )
    .bind(character_id)
    .bind(&claims.name)
    .bind(corporation_id)
    .bind(&claims.owner)
    .bind(&refresh)
    .bind(&scopes_vec)
    .execute(pool)
    .await?;

    info!(character_id, character_name = %claims.name, "linked character");

    Ok(CharacterRow {
        character_id,
        character_name: claims.name,
        corporation_id,
        owner_hash: claims.owner,
        scopes: scopes_vec,
    })
}

#[derive(Debug, Deserialize)]
struct EveClaims {
    sub: String,
    name: String,
    owner: String,
    #[serde(default)]
    scp: ScopeClaim,
    #[allow(dead_code)]
    exp: i64,
}

#[derive(Debug, Deserialize, Default)]
#[serde(untagged)]
enum ScopeClaim {
    Many(Vec<String>),
    One(String),
    #[default]
    None,
}

impl ScopeClaim {
    fn into_vec(self) -> Vec<String> {
        match self {
            ScopeClaim::Many(v) => v,
            ScopeClaim::One(s) => s.split_whitespace().map(str::to_owned).collect(),
            ScopeClaim::None => Vec::new(),
        }
    }
}

fn parse_character_id(sub: &str) -> AppResult<i64> {
    let raw = sub.strip_prefix("CHARACTER:EVE:").ok_or_else(|| {
        AppError::Auth(format!(
            "unexpected sub format (expected CHARACTER:EVE:N): {sub}"
        ))
    })?;
    raw.parse::<i64>()
        .map_err(|e| AppError::Auth(format!("character_id parse: {e}")))
}

async fn verify_jwt(
    http: &reqwest::Client,
    endpoints: &AuthEndpoints,
    access_token: &str,
) -> AppResult<EveClaims> {
    let jwks: JwkSet = http
        .get(&endpoints.jwks_url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let header = jsonwebtoken::decode_header(access_token)?;
    let kid = header
        .kid
        .ok_or_else(|| AppError::Auth("JWT header missing kid".into()))?;
    let jwk = jwks
        .find(&kid)
        .ok_or_else(|| AppError::Auth(format!("no JWKS key for kid={kid}")))?;
    let key = DecodingKey::from_jwk(jwk).map_err(|e| AppError::Auth(format!("decode JWK: {e}")))?;

    let alg = header.alg;
    let mut validation = Validation::new(alg);
    validation.set_audience(&["EVE Online"]);
    let issuers: Vec<&str> = endpoints
        .allowed_issuers
        .iter()
        .map(String::as_str)
        .collect();
    validation.set_issuer(&issuers);

    let data = jsonwebtoken::decode::<EveClaims>(access_token, &key, &validation)?;
    Ok(data.claims)
}

async fn fetch_corporation_id(
    http: &reqwest::Client,
    endpoints: &AuthEndpoints,
    character_id: i64,
) -> AppResult<i64> {
    #[derive(Deserialize)]
    struct CharacterInfo {
        corporation_id: i64,
    }
    let url = format!("{}/characters/{character_id}/", endpoints.esi_base);
    let info: CharacterInfo = http
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(info.corporation_id)
}

// ---------------------------------------------------------------------------
// Access token cache + refresh

#[derive(Clone, Default)]
pub struct AccessTokenCache {
    inner: Arc<Mutex<HashMap<i64, CachedAccess>>>,
}

#[derive(Clone)]
struct CachedAccess {
    token: String,
    expires_at: Instant,
}

impl AccessTokenCache {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Return a fresh access token for `character_id`, refreshing via the stored
/// refresh_token when the cache entry is stale (or absent). On
/// `invalid_grant`, marks the character `needs_reauth` and surfaces
/// `EsiError::Unauthorized` via `AppError::Auth`.
#[instrument(skip_all, fields(character_id))]
pub async fn get_access_token(
    cache: &AccessTokenCache,
    config: &Config,
    endpoints: &AuthEndpoints,
    pool: &PgPool,
    http: &reqwest::Client,
    character_id: i64,
) -> AppResult<String> {
    {
        let guard = cache.inner.lock().await;
        if let Some(c) = guard.get(&character_id) {
            if c.expires_at > Instant::now() {
                return Ok(c.token.clone());
            }
        }
    }

    let (refresh_token, status): (String, String) =
        sqlx::query_as("SELECT refresh_token, status FROM characters WHERE character_id = $1")
            .bind(character_id)
            .fetch_one(pool)
            .await?;
    if status != "active" {
        return Err(AppError::Auth(format!(
            "character {character_id} status={status}, re-run auth"
        )));
    }

    let client = build_oauth(config, endpoints)?;
    let result = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token))
        .request_async(http)
        .await;

    let token = match result {
        Ok(t) => t,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("invalid_grant") {
                warn!(character_id, "refresh token rejected (invalid_grant)");
                sqlx::query(
                    "UPDATE characters SET status = 'needs_reauth' WHERE character_id = $1",
                )
                .bind(character_id)
                .execute(pool)
                .await?;
            }
            return Err(AppError::Auth(format!("refresh failed: {e}")));
        }
    };

    let access = token.access_token().secret().to_owned();
    let new_refresh = token.refresh_token().map(|r| r.secret().to_owned());
    let expires_in = token.expires_in().unwrap_or(Duration::from_secs(1200));

    if let Some(rt) = new_refresh {
        sqlx::query(
            "UPDATE characters SET refresh_token = $1, last_refreshed_at = now() \
             WHERE character_id = $2",
        )
        .bind(&rt)
        .bind(character_id)
        .execute(pool)
        .await?;
    }

    let cached_until = Instant::now() + expires_in.saturating_sub(Duration::from_secs(60));
    {
        let mut guard = cache.inner.lock().await;
        guard.insert(
            character_id,
            CachedAccess {
                token: access.clone(),
                expires_at: cached_until,
            },
        );
    }

    Ok(access)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_character_id_extracts_trailing_int() {
        assert_eq!(parse_character_id("CHARACTER:EVE:12345").unwrap(), 12345);
    }

    #[test]
    fn parse_character_id_rejects_unknown_prefix() {
        assert!(parse_character_id("EVE:12345").is_err());
        assert!(parse_character_id("CHARACTER:OTHER:1").is_err());
    }

    #[test]
    fn parse_character_id_rejects_non_numeric() {
        assert!(parse_character_id("CHARACTER:EVE:abc").is_err());
    }

    #[test]
    fn scope_claim_handles_string_array_and_missing() {
        let one: ScopeClaim = serde_json::from_value(serde_json::json!("a b c")).unwrap();
        assert_eq!(one.into_vec(), vec!["a", "b", "c"]);

        let many: ScopeClaim = serde_json::from_value(serde_json::json!(["x", "y"])).unwrap();
        assert_eq!(many.into_vec(), vec!["x", "y"]);

        let none = ScopeClaim::None;
        assert!(none.into_vec().is_empty());
    }
}
