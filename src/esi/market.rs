use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use super::client::{EsiClient, EsiError, EsiResponse};

const PAGE_CONCURRENCY: usize = 4;

/// One market order, as returned by either
/// `GET /markets/{region_id}/orders/` or
/// `GET /markets/structures/{structure_id}/`.
///
/// The structure variant omits `system_id`, so it's `Option`al here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketOrder {
    pub order_id: i64,
    pub type_id: i64,
    pub location_id: i64,
    #[serde(default)]
    pub system_id: Option<i64>,
    pub volume_total: i64,
    pub volume_remain: i64,
    pub min_volume: i64,
    pub price: f64,
    pub is_buy_order: bool,
    pub duration: i32,
    pub issued: DateTime<Utc>,
    pub range: String,
}

/// All current orders for a public region (e.g. The Forge / Jita).
pub async fn region_orders(
    client: &EsiClient,
    region_id: i64,
) -> Result<Vec<MarketOrder>, EsiError> {
    paged(
        client,
        &format!("/markets/{region_id}/orders/"),
        &[("order_type", "all")],
        None,
    )
    .await
}

/// All current orders for a player-owned structure. Requires an access token
/// with `esi-markets.structure_markets.v1`.
pub async fn structure_orders(
    client: &EsiClient,
    structure_id: i64,
    access_token: &str,
) -> Result<Vec<MarketOrder>, EsiError> {
    paged(
        client,
        &format!("/markets/structures/{structure_id}/"),
        &[],
        Some(access_token.to_string()),
    )
    .await
}

async fn paged<T: serde::de::DeserializeOwned + Send + 'static>(
    client: &EsiClient,
    path: &str,
    base_params: &[(&str, &str)],
    access_token: Option<String>,
) -> Result<Vec<T>, EsiError> {
    let owned: Vec<(String, String)> = base_params
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();

    let first: EsiResponse<Vec<T>> =
        fetch_page(client, path, &owned, access_token.as_deref(), 1).await?;
    let total = first.pages.unwrap_or(1);
    if total <= 1 {
        return Ok(first.body);
    }

    let mut all = first.body;
    let path_owned = path.to_string();

    let stream = futures::stream::iter(2..=total).map(|page| {
        let client = client.clone();
        let path = path_owned.clone();
        let owned = owned.clone();
        let tok = access_token.clone();
        async move { fetch_page::<T>(&client, &path, &owned, tok.as_deref(), page).await }
    });

    let results: Vec<Result<EsiResponse<Vec<T>>, EsiError>> =
        stream.buffer_unordered(PAGE_CONCURRENCY).collect().await;

    for r in results {
        all.extend(r?.body);
    }
    Ok(all)
}

async fn fetch_page<T: serde::de::DeserializeOwned>(
    client: &EsiClient,
    path: &str,
    base_params: &[(String, String)],
    access_token: Option<&str>,
    page: u32,
) -> Result<EsiResponse<Vec<T>>, EsiError> {
    let page_str = page.to_string();
    let mut q: Vec<(&str, &str)> = base_params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    q.push(("page", &page_str));
    client.get_json_with_auth(path, &q, access_token).await
}
