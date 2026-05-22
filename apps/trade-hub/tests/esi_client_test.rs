//! Integration tests for the ESI client. Uses `wiremock` to stand in for ESI.
//!
//! Covers the four behaviors PROMPT.md §7 Phase 2 requires:
//! - User-Agent injection
//! - Error-limit backoff
//! - Retry on 503
//! - Pagination follow

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use eve_esi::{EsiClient, EsiError, market};
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

fn client_for(server: &MockServer) -> EsiClient {
    let http = reqwest::Client::builder()
        .user_agent("eve-trade-hub-analyzer/test (vilberg@example.com)")
        .build()
        .unwrap();
    EsiClient::with_http_and_base(http, server.uri())
}

#[tokio::test]
async fn injects_user_agent_on_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ping"))
        .and(header(
            "user-agent",
            "eve-trade-hub-analyzer/test (vilberg@example.com)",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let resp: eve_esi::EsiResponse<serde_json::Value> =
        client.get_json("/ping", &[]).await.unwrap();
    assert_eq!(resp.body["ok"], json!(true));
}

#[tokio::test]
async fn retries_on_503_then_succeeds() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicU32::new(0));

    struct Flaky {
        calls: Arc<AtomicU32>,
    }
    impl Respond for Flaky {
        fn respond(&self, _: &Request) -> ResponseTemplate {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n < 2 {
                ResponseTemplate::new(503)
            } else {
                ResponseTemplate::new(200).set_body_json(json!({"ok": true}))
            }
        }
    }

    Mock::given(method("GET"))
        .and(path("/flaky"))
        .respond_with(Flaky {
            calls: calls.clone(),
        })
        .mount(&server)
        .await;

    let client = client_for(&server);
    let resp: eve_esi::EsiResponse<serde_json::Value> =
        client.get_json("/flaky", &[]).await.unwrap();
    assert_eq!(resp.body["ok"], json!(true));
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn gives_up_after_max_retries_on_503() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/never"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let err = client
        .get_json::<serde_json::Value>("/never", &[])
        .await
        .unwrap_err();
    assert!(matches!(err, EsiError::Server(_)));
}

#[tokio::test]
async fn error_limit_low_triggers_hold_before_next_request() {
    let server = MockServer::start().await;

    // First call returns 200 but with x-esi-error-limit-remain=2 (below
    // threshold) and reset=1s — that should arm a hold until ~1s later.
    Mock::given(method("GET"))
        .and(path("/first"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"ok": true}))
                .insert_header("x-esi-error-limit-remain", "2")
                .insert_header("x-esi-error-limit-reset", "1"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/second"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;

    let client = client_for(&server);
    client
        .get_json::<serde_json::Value>("/first", &[])
        .await
        .unwrap();

    let start = Instant::now();
    client
        .get_json::<serde_json::Value>("/second", &[])
        .await
        .unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed >= Duration::from_millis(900),
        "expected backoff to delay request by ~1s, only waited {elapsed:?}"
    );
}

#[tokio::test]
async fn paginated_region_orders_fetches_every_page() {
    let server = MockServer::start().await;

    let order_json = |id: i64, type_id: i64| {
        json!({
            "order_id": id,
            "type_id": type_id,
            "location_id": 60003760,
            "system_id": 30000142,
            "volume_total": 100,
            "volume_remain": 100,
            "min_volume": 1,
            "price": 1000.0,
            "is_buy_order": false,
            "duration": 90,
            "issued": "2026-05-01T12:00:00Z",
            "range": "region"
        })
    };

    // Page 1 advertises x-pages: 3.
    Mock::given(method("GET"))
        .and(path("/markets/10000002/orders/"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([order_json(1, 34), order_json(2, 35)]))
                .insert_header("x-pages", "3"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/markets/10000002/orders/"))
        .and(query_param("page", "2"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([order_json(3, 36)]))
                .insert_header("x-pages", "3"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/markets/10000002/orders/"))
        .and(query_param("page", "3"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([order_json(4, 37), order_json(5, 38)]))
                .insert_header("x-pages", "3"),
        )
        .mount(&server)
        .await;

    let client = client_for(&server);
    let orders = market::region_orders(&client, 10000002).await.unwrap();
    assert_eq!(orders.len(), 5);
    let mut ids: Vec<i64> = orders.iter().map(|o| o.order_id).collect();
    ids.sort();
    assert_eq!(ids, vec![1, 2, 3, 4, 5]);
}
