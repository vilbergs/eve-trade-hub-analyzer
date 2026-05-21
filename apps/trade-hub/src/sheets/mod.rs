//! Google Sheets export.
//!
//! Consumes typed report rows from `analysis::*` and pushes them into a
//! single spreadsheet, one tab per report. Auth is interactive Google
//! OAuth (mirroring the EVE SSO flow in `esi::auth`); refresh tokens are
//! persisted in `google_accounts`.
//!
//! Kept deliberately decoupled from the data layer: it accepts anything
//! that implements [`analysis::output::Renderable`], so a future Sheets
//! service or some other consumer can call `push_report` with new row
//! types without changing this module.

pub mod api;
pub mod auth;

use serde_json::Value;

use crate::analysis::output::Renderable;
use eve_core::AppResult;

/// Push the given rows to `tab_name` inside `spreadsheet_id`:
/// 1. Ensures the tab exists (creates it if missing).
/// 2. Clears the existing range.
/// 3. Writes one header row followed by `rows.len()` data rows.
///
/// Idempotent: re-running overwrites the tab's contents.
pub async fn push_report<T: Renderable>(
    http: &reqwest::Client,
    access_token: &str,
    spreadsheet_id: &str,
    tab_name: &str,
    rows: &[T],
) -> AppResult<()> {
    api::ensure_tab(http, access_token, spreadsheet_id, tab_name).await?;
    api::clear_tab(http, access_token, spreadsheet_id, tab_name).await?;

    let mut values: Vec<Vec<Value>> = Vec::with_capacity(rows.len() + 1);
    values.push(T::headers().iter().map(|h| Value::String((*h).into())).collect());
    for r in rows {
        values.push(r.cells().into_iter().map(Value::String).collect());
    }

    api::write_values(http, access_token, spreadsheet_id, tab_name, values).await
}
