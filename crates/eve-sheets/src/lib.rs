//! Google Sheets export adapter.
//!
//! Headless service-account auth, then four Sheets v4 operations:
//! ensure tab, clear range, write values, list tabs. The public
//! surface is intentionally a pure data interface — caller provides
//! `&[&str]` headers and `Vec<Vec<String>>` rows. Translation from
//! typed report rows lives in the caller, so this crate stays free of
//! any report-shape assumptions.

pub mod api;
pub mod auth;

use serde_json::Value;

use eve_core::AppResult;

/// Caller-supplied configuration for Google Sheets export.
///
/// Auth is headless via a service account; share the target
/// spreadsheet with the service account's `client_email`.
#[derive(Debug, Clone)]
pub struct GoogleConfig {
    /// Path to the service account JSON key file on disk.
    pub service_account_key_path: String,
    /// Spreadsheet ID (the long string between `/d/` and `/edit` in a
    /// Sheets URL).
    pub spreadsheet_id: String,
}

/// Push the given rows to `tab_name` inside `spreadsheet_id`:
/// 1. Ensures the tab exists (creates it if missing).
/// 2. Clears the existing range.
/// 3. Writes one header row followed by `rows.len()` data rows.
///
/// Idempotent: re-running overwrites the tab's contents.
pub async fn push_report(
    http: &reqwest::Client,
    access_token: &str,
    spreadsheet_id: &str,
    tab_name: &str,
    headers: &[&str],
    rows: Vec<Vec<String>>,
) -> AppResult<()> {
    api::ensure_tab(http, access_token, spreadsheet_id, tab_name).await?;
    api::clear_tab(http, access_token, spreadsheet_id, tab_name).await?;

    let mut values: Vec<Vec<Value>> = Vec::with_capacity(rows.len() + 1);
    values.push(headers.iter().map(|h| Value::String((*h).into())).collect());
    for r in rows {
        values.push(r.into_iter().map(Value::String).collect());
    }

    api::write_values(http, access_token, spreadsheet_id, tab_name, values).await
}
