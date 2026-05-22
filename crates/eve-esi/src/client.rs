use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use reqwest::header::HeaderMap;
use serde::de::DeserializeOwned;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::warn;

use eve_core::AppError;

const DEFAULT_BASE_URL: &str = "https://esi.evetech.net/latest";
const ERROR_LIMIT_THRESHOLD: u32 = 10;
const MAX_RETRIES: u32 = 3;
const BASE_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(5);

#[derive(Debug, Error)]
pub enum EsiError {
    #[error("rate limited (429)")]
    RateLimited,
    #[error("unauthorized (401)")]
    Unauthorized,
    #[error("not found (404)")]
    NotFound,
    #[error("server error: {0}")]
    Server(StatusCode),
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("decode error: {0}")]
    Decode(#[from] serde_json::Error),
}

impl From<EsiError> for AppError {
    fn from(e: EsiError) -> Self {
        AppError::Esi(e.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct EsiResponse<T> {
    pub body: T,
    pub expires: Option<DateTime<Utc>>,
    pub last_modified: Option<DateTime<Utc>>,
    pub etag: Option<String>,
    pub pages: Option<u32>,
}

#[derive(Clone)]
pub struct EsiClient {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    base_url: String,
    hold_until: Mutex<Option<Instant>>,
}

impl EsiClient {
    pub fn new(user_agent: &str) -> Result<Self, AppError> {
        let http = reqwest::Client::builder()
            .user_agent(user_agent)
            .gzip(true)
            .build()?;
        Ok(Self::with_http_and_base(http, DEFAULT_BASE_URL.to_string()))
    }

    /// Construct with an arbitrary base URL; the test suite uses this to
    /// point at a `wiremock` server.
    pub fn with_http_and_base(http: reqwest::Client, base_url: String) -> Self {
        Self {
            inner: Arc::new(Inner {
                http,
                base_url,
                hold_until: Mutex::new(None),
            }),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.inner.base_url
    }

    pub async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<EsiResponse<T>, EsiError> {
        self.get_json_with_auth(path, params, None).await
    }

    pub async fn get_json_with_auth<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, &str)],
        access_token: Option<&str>,
    ) -> Result<EsiResponse<T>, EsiError> {
        self.wait_for_error_limit().await;

        let url = format!("{}{}", self.inner.base_url, path);
        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            let mut req = self.inner.http.get(&url).query(params);
            if let Some(token) = access_token {
                req = req.bearer_auth(token);
            }
            let resp = req.send().await?;
            let status = resp.status();
            let headers = resp.headers().clone();

            self.update_error_limit(&headers).await;

            if status.is_success() {
                let pages = header_u32(&headers, "x-pages");
                let expires = header_http_date(&headers, "expires");
                let last_modified = header_http_date(&headers, "last-modified");
                let etag = header_str(&headers, "etag").map(str::to_owned);
                let bytes = resp.bytes().await?;
                let body: T = serde_json::from_slice(&bytes)?;
                return Ok(EsiResponse {
                    body,
                    expires,
                    last_modified,
                    etag,
                    pages,
                });
            }

            match status {
                StatusCode::UNAUTHORIZED => return Err(EsiError::Unauthorized),
                StatusCode::FORBIDDEN => return Err(EsiError::Unauthorized),
                StatusCode::NOT_FOUND => return Err(EsiError::NotFound),
                StatusCode::TOO_MANY_REQUESTS => return Err(EsiError::RateLimited),
                _ => {}
            }

            if matches!(status.as_u16(), 502..=504) && attempt < MAX_RETRIES {
                let backoff = exp_backoff(attempt);
                warn!(
                    status = %status,
                    attempt,
                    sleep_ms = backoff.as_millis() as u64,
                    "ESI transient error, retrying"
                );
                tokio::time::sleep(backoff).await;
                continue;
            }

            return Err(EsiError::Server(status));
        }
    }

    async fn wait_for_error_limit(&self) {
        let deadline = {
            let guard = self.inner.hold_until.lock().await;
            *guard
        };
        if let Some(deadline) = deadline {
            let now = Instant::now();
            if deadline > now {
                let wait = deadline - now;
                warn!(
                    wait_ms = wait.as_millis() as u64,
                    "ESI error-limit threshold tripped, holding requests"
                );
                tokio::time::sleep(wait).await;
            }
        }
    }

    async fn update_error_limit(&self, headers: &HeaderMap) {
        let remain = header_u32(headers, "x-esi-error-limit-remain");
        let reset = header_u32(headers, "x-esi-error-limit-reset");
        if let (Some(r), Some(s)) = (remain, reset) {
            if r < ERROR_LIMIT_THRESHOLD {
                let deadline = Instant::now() + Duration::from_secs(s as u64);
                let mut guard = self.inner.hold_until.lock().await;
                let cur = *guard;
                if cur.is_none_or(|d| d < deadline) {
                    *guard = Some(deadline);
                }
            }
        }
    }
}

fn exp_backoff(attempt: u32) -> Duration {
    // attempt starts at 1: 500ms, 1s, 2s, 4s — capped at MAX_BACKOFF.
    let shift = attempt.saturating_sub(1);
    let scaled = BASE_BACKOFF.checked_mul(1u32 << shift.min(10));
    scaled.unwrap_or(MAX_BACKOFF).min(MAX_BACKOFF)
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

fn header_u32(headers: &HeaderMap, name: &str) -> Option<u32> {
    header_str(headers, name).and_then(|v| v.parse().ok())
}

fn header_http_date(headers: &HeaderMap, name: &str) -> Option<DateTime<Utc>> {
    let raw = header_str(headers, name)?;
    DateTime::parse_from_rfc2822(raw)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_then_caps() {
        assert_eq!(exp_backoff(1), Duration::from_millis(500));
        assert_eq!(exp_backoff(2), Duration::from_secs(1));
        assert_eq!(exp_backoff(3), Duration::from_secs(2));
        assert_eq!(exp_backoff(10), MAX_BACKOFF);
    }
}
