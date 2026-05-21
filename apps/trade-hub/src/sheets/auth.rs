//! Google service account JWT-bearer auth.
//!
//! Headless flow: load a service account JSON key, sign a short-lived
//! JWT with the included RSA private key, exchange it for an access
//! token at Google's OAuth token endpoint, cache in memory. No browser,
//! no refresh tokens, no DB state — ideal for cron / systemd timers.
//!
//! The service account itself has no Drive permissions by default, so
//! the target spreadsheet must be shared with the account's
//! `client_email` (Editor access).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::instrument;

use crate::config::GoogleConfig;
use crate::error::{AppError, AppResult};

const SCOPE: &str = "https://www.googleapis.com/auth/spreadsheets";
const JWT_BEARER_GRANT: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";
const TOKEN_LIFETIME_SECS: u64 = 3600;

/// Subset of the service account JSON we care about.
#[derive(Debug, Clone, Deserialize)]
pub struct ServiceAccountKey {
    pub client_email: String,
    pub private_key: String,
    pub private_key_id: String,
    #[serde(default = "default_token_uri")]
    pub token_uri: String,
}

fn default_token_uri() -> String {
    "https://oauth2.googleapis.com/token".into()
}

impl ServiceAccountKey {
    pub fn load(path: &str) -> AppResult<Self> {
        let raw = std::fs::read_to_string(path).map_err(|e| {
            AppError::Config(format!("read service account key at {path}: {e}"))
        })?;
        serde_json::from_str(&raw)
            .map_err(|e| AppError::Config(format!("parse service account key at {path}: {e}")))
    }
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Clone, Default)]
pub struct AccessTokenCache {
    inner: Arc<Mutex<Option<CachedAccess>>>,
}

#[derive(Clone)]
struct CachedAccess {
    token: String,
    /// SystemTime not Instant — only because it's all we need; reset
    /// across process boundaries doesn't matter, the cache is in-memory.
    expires_at_unix: u64,
}

impl AccessTokenCache {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Return a cached or freshly-minted access token for the configured
/// service account.
#[instrument(skip_all)]
pub async fn get_access_token(
    cache: &AccessTokenCache,
    google: &GoogleConfig,
    http: &reqwest::Client,
) -> AppResult<String> {
    let now = unix_now()?;
    {
        let guard = cache.inner.lock().await;
        if let Some(c) = guard.as_ref() {
            // 60s safety margin so we don't ship a token that expires
            // mid-request.
            if c.expires_at_unix > now + 60 {
                return Ok(c.token.clone());
            }
        }
    }

    let key = ServiceAccountKey::load(&google.service_account_key_path)?;
    let token = mint_token(http, &key, now).await?;
    let cached_until = now + token.expires_in.saturating_sub(60);
    {
        let mut guard = cache.inner.lock().await;
        *guard = Some(CachedAccess {
            token: token.access_token.clone(),
            expires_at_unix: cached_until,
        });
    }
    Ok(token.access_token)
}

async fn mint_token(
    http: &reqwest::Client,
    key: &ServiceAccountKey,
    now: u64,
) -> AppResult<TokenResponse> {
    let claims = Claims {
        iss: &key.client_email,
        scope: SCOPE,
        aud: &key.token_uri,
        iat: now,
        exp: now + TOKEN_LIFETIME_SECS,
    };

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(key.private_key_id.clone());

    let encoding_key = EncodingKey::from_rsa_pem(key.private_key.as_bytes())
        .map_err(|e| AppError::Auth(format!("service account private_key not valid PEM: {e}")))?;
    let assertion = jsonwebtoken::encode(&header, &claims, &encoding_key)?;

    let resp = http
        .post(&key.token_uri)
        .form(&[
            ("grant_type", JWT_BEARER_GRANT),
            ("assertion", &assertion),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Auth(format!(
            "google token exchange failed ({status}): {body}"
        )));
    }
    Ok(resp.json().await?)
}

fn unix_now() -> AppResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|e| AppError::Other(format!("system clock before unix epoch: {e}")))
}
