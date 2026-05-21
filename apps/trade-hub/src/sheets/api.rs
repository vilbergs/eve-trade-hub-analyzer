//! Thin Google Sheets v4 client.
//!
//! Only the four operations the exporter needs: list tabs, add a tab,
//! clear a range, write values. Uses raw reqwest against the REST API
//! rather than pulling in a generated SDK.

use serde::Deserialize;
use serde_json::json;

use crate::error::{AppError, AppResult};

const SHEETS_BASE: &str = "https://sheets.googleapis.com/v4/spreadsheets";

#[derive(Debug, Deserialize)]
struct SpreadsheetMeta {
    sheets: Vec<SheetMeta>,
}

#[derive(Debug, Deserialize)]
struct SheetMeta {
    properties: SheetProperties,
}

#[derive(Debug, Deserialize)]
struct SheetProperties {
    title: String,
}

async fn list_tabs(
    http: &reqwest::Client,
    access_token: &str,
    spreadsheet_id: &str,
) -> AppResult<Vec<String>> {
    let url = format!("{SHEETS_BASE}/{spreadsheet_id}?fields=sheets.properties.title");
    let meta: SpreadsheetMeta = http
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()
        .map_err(|e| AppError::Other(format!("sheets list_tabs: {e}")))?
        .json()
        .await?;
    Ok(meta.sheets.into_iter().map(|s| s.properties.title).collect())
}

/// Add `tab_name` if it doesn't already exist. Idempotent.
pub async fn ensure_tab(
    http: &reqwest::Client,
    access_token: &str,
    spreadsheet_id: &str,
    tab_name: &str,
) -> AppResult<()> {
    let existing = list_tabs(http, access_token, spreadsheet_id).await?;
    if existing.iter().any(|t| t == tab_name) {
        return Ok(());
    }

    let url = format!("{SHEETS_BASE}/{spreadsheet_id}:batchUpdate");
    let body = json!({
        "requests": [{
            "addSheet": { "properties": { "title": tab_name } }
        }]
    });
    http.post(&url)
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .await?
        .error_for_status()
        .map_err(|e| AppError::Other(format!("sheets addSheet: {e}")))?;
    Ok(())
}

/// Clear every cell in `tab_name`.
pub async fn clear_tab(
    http: &reqwest::Client,
    access_token: &str,
    spreadsheet_id: &str,
    tab_name: &str,
) -> AppResult<()> {
    let range = encode_range(tab_name);
    let url = format!("{SHEETS_BASE}/{spreadsheet_id}/values/{range}:clear");
    http.post(&url)
        .bearer_auth(access_token)
        .json(&json!({}))
        .send()
        .await?
        .error_for_status()
        .map_err(|e| AppError::Other(format!("sheets clear: {e}")))?;
    Ok(())
}

/// Write `values` starting at A1 of `tab_name`.
pub async fn write_values(
    http: &reqwest::Client,
    access_token: &str,
    spreadsheet_id: &str,
    tab_name: &str,
    values: Vec<Vec<serde_json::Value>>,
) -> AppResult<()> {
    let range = encode_range(tab_name);
    let url = format!(
        "{SHEETS_BASE}/{spreadsheet_id}/values/{range}?valueInputOption=USER_ENTERED"
    );
    http.put(&url)
        .bearer_auth(access_token)
        .json(&json!({ "values": values }))
        .send()
        .await?
        .error_for_status()
        .map_err(|e| AppError::Other(format!("sheets write: {e}")))?;
    Ok(())
}

/// URL-encode a tab name for use in a Sheets A1 range. Quote the tab
/// name (Sheets requires `'Tab Name'!A1` when the name has spaces or
/// punctuation), then percent-encode the whole thing.
fn encode_range(tab_name: &str) -> String {
    let quoted = format!("'{}'", tab_name.replace('\'', "''"));
    urlencoding_encode(&quoted)
}

fn urlencoding_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}
