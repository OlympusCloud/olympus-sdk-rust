//! Integration tests for the SmsService (Wave 2).

use mockito::Server;
use olympus_sdk::services::sms::{GetConversationsOptions, SendViaCpaasRequest};
use olympus_sdk::{OlympusClient, OlympusConfig, OlympusError};
use serde_json::json;

fn make_client(base_url: &str) -> OlympusClient {
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url(base_url);
    OlympusClient::from_config(cfg)
}

#[tokio::test]
async fn send_posts_config_to_body() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice/sms/send")
        .match_body(mockito::Matcher::Json(json!({
            "config_id": "cfg_1",
            "to": "+15551234567",
            "body": "Hello"
        })))
        .with_status(200)
        .with_body(r#"{"sid": "msg_1"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc
        .sms()
        .send("cfg_1", "+15551234567", "Hello")
        .await
        .expect("ok");
    assert_eq!(resp["sid"], json!("msg_1"));
    m.assert_async().await;
}

#[tokio::test]
async fn get_conversations_url_encodes_phone_and_appends_query() {
    let mut server = Server::new_async().await;
    let m = server
        .mock(
            "GET",
            "/voice/sms/conversations/%2B15551234567?limit=10&offset=20",
        )
        .with_status(200)
        .with_body(r#"{"conversations": []}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .sms()
        .get_conversations(
            "+15551234567",
            GetConversationsOptions {
                limit: Some(10),
                offset: Some(20),
            },
        )
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn get_conversations_no_opts_omits_query() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/voice/sms/conversations/%2B1")
        .with_status(200)
        .with_body(r#"{"conversations": []}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .sms()
        .get_conversations("+1", GetConversationsOptions::default())
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn send_via_cpaas_includes_optional_webhook() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/cpaas/messages/sms")
        .match_body(mockito::Matcher::Json(json!({
            "from": "+1555",
            "to": "+1666",
            "body": "Hi",
            "webhook_url": "https://hook"
        })))
        .with_status(200)
        .with_body(r#"{"id": "cpaas_1"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc
        .sms()
        .send_via_cpaas(SendViaCpaasRequest {
            from: "+1555",
            to: "+1666",
            body: "Hi",
            webhook_url: Some("https://hook"),
        })
        .await
        .expect("ok");
    assert_eq!(resp["id"], json!("cpaas_1"));
    m.assert_async().await;
}

#[tokio::test]
async fn send_via_cpaas_omits_webhook_when_none() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/cpaas/messages/sms")
        .match_body(mockito::Matcher::Json(json!({
            "from": "+1555",
            "to": "+1666",
            "body": "Hi"
        })))
        .with_status(200)
        .with_body(r#"{"id": "cpaas_2"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc
        .sms()
        .send_via_cpaas(SendViaCpaasRequest {
            from: "+1555",
            to: "+1666",
            body: "Hi",
            webhook_url: None,
        })
        .await
        .expect("ok");
    assert_eq!(resp["id"], json!("cpaas_2"));
    m.assert_async().await;
}

#[tokio::test]
async fn get_status_returns_provider_metadata() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/cpaas/messages/msg_abc")
        .with_status(200)
        .with_body(r#"{"id": "msg_abc", "status": "delivered"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc.sms().get_status("msg_abc").await.expect("ok");
    assert_eq!(resp["status"], json!("delivered"));
    m.assert_async().await;
}

#[tokio::test]
async fn send_propagates_server_error() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice/sms/send")
        .with_status(429)
        .with_body(r#"{"error": {"message": "rate limit"}}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let res = oc.sms().send("cfg", "+1", "x").await;
    match res {
        Err(OlympusError::Api { status, .. }) => assert_eq!(status, 429),
        other => panic!("expected Api error, got {:?}", other),
    }
    m.assert_async().await;
}
