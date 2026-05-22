//! Market snapshot pollers (trade-hub specific).
//!
//! Two flavours: `hub` (loops over every `tracked_stations` row and pulls
//! `/markets/structures/{id}/` with a character access token) and `jita`
//! (pulls `/markets/{JITA_REGION_ID}/orders/`, no auth). Both filter the
//! ESI response down to `tracked_types` before persisting, per
//! ADDENDUM.md §2.
//!
//! Storage primitives (`ingest_orders`, `ensure_partitions`,
//! `drop_old_partitions`) live in `eve-market`. This module just holds
//! the trade-hub-specific drivers and the whitelist queries.

pub mod hub;
pub mod jita;

use std::collections::HashSet;

use sqlx::PgPool;

use eve_core::AppResult;

pub use eve_market::{RunSummary, drop_old_partitions, ensure_partitions};

pub(crate) async fn tracked_types(pool: &PgPool) -> AppResult<HashSet<i64>> {
    let rows: Vec<i64> = sqlx::query_scalar("SELECT type_id FROM tracked_types")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().collect())
}
