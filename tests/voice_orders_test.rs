//! Integration tests for the VoiceOrdersService (Wave 1 + Wave 2 additions).

use mockito::Server;
use olympus_sdk::services::voice_orders::ListVoiceOrdersOptions;
use olympus_sdk::{OlympusClient, OlympusConfig, OlympusError};
use serde_json::json;

fn make_client(base_url: &str) -> OlympusClient {
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url(base_url);
    OlympusClient::from_config(cfg)
}

#[tokio::test]
async fn create_typed_builds_full_body_with_extra() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice-orders")
        .match_body(mockito::Matcher::Json(json!({
            "location_id": "loc_1",
            "items": [{"name": "pizza", "quantity": 2}],
            "fulfillment": "delivery",
            "caller_phone": "+1555"
        })))
        .with_status(201)
        .with_body(r#"{"id": "order_1"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc
        .voice_orders()
        .create(
            "loc_1",
            json!([{"name": "pizza", "quantity": 2}]),
            Some("delivery"),
            Some(json!({"caller_phone": "+1555"})),
        )
        .await
        .expect("create");
    assert_eq!(resp["id"], json!("order_1"));
    m.assert_async().await;
}

#[tokio::test]
async fn create_raw_dart_parity_passes_body_unchanged() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice-orders")
        .match_body(mockito::Matcher::Json(json!({
            "location_id": "loc_2",
            "items": [],
            "metadata": {"source": "voice"}
        })))
        .with_status(201)
        .with_body(r#"{"id": "order_2"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc
        .voice_orders()
        .create_raw(json!({
            "location_id": "loc_2",
            "items": [],
            "metadata": {"source": "voice"}
        }))
        .await
        .expect("ok");
    assert_eq!(resp["id"], json!("order_2"));
    m.assert_async().await;
}

#[tokio::test]
async fn get_returns_order() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/voice-orders/order_1")
        .with_status(200)
        .with_body(r#"{"id": "order_1", "status": "pending"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc.voice_orders().get("order_1").await.expect("get");
    assert_eq!(resp["status"], json!("pending"));
    m.assert_async().await;
}

#[tokio::test]
async fn list_with_filters_appends_query() {
    let mut server = Server::new_async().await;
    let m = server
        .mock(
            "GET",
            "/voice-orders?caller_phone=%2B1555&status=pending&location_id=loc_1&limit=25",
        )
        .with_status(200)
        .with_body(r#"{"orders": []}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice_orders()
        .list(ListVoiceOrdersOptions {
            caller_phone: Some("+1555"),
            status: Some("pending"),
            location_id: Some("loc_1"),
            limit: Some(25),
        })
        .await
        .expect("list");
    m.assert_async().await;
}

#[tokio::test]
async fn list_no_opts_omits_query() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/voice-orders")
        .with_status(200)
        .with_body(r#"{"orders": []}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice_orders()
        .list(ListVoiceOrdersOptions::default())
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn push_to_pos_posts_empty_body() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice-orders/order_1/push-pos")
        .match_body(mockito::Matcher::Json(json!({})))
        .with_status(200)
        .with_body(r#"{"pushed": true}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc
        .voice_orders()
        .push_to_pos("order_1")
        .await
        .expect("push");
    assert_eq!(resp["pushed"], json!(true));
    m.assert_async().await;
}

#[tokio::test]
async fn create_propagates_validation_error() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice-orders")
        .with_status(400)
        .with_body(r#"{"error": {"message": "menu mismatch"}}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let res = oc
        .voice_orders()
        .create_raw(json!({"location_id": "x"}))
        .await;
    match res {
        Err(OlympusError::Api { status, message }) => {
            assert_eq!(status, 400);
            assert!(message.contains("menu mismatch"));
        }
        other => panic!("expected Api error, got {:?}", other),
    }
    m.assert_async().await;
}
