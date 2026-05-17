//! ESI (EVE Online Swagger Interface) client.
//!
//! Wraps `reqwest::Client` with the conventions ESI requires of well-behaved
//! consumers: a meaningful `User-Agent`, response-header backoff when
//! `X-ESI-Error-Limit-Remain` drops below 10, and short retries on transient
//! 5xx. The public surface is `EsiClient::get_json` (and an
//! `_with_auth` variant for endpoints that need a bearer token); typed
//! endpoint wrappers live in submodules.

pub mod client;
pub mod market;

pub use client::{EsiClient, EsiError, EsiResponse};
