//! Integration tests for the IdentityService (Wave 2).

use mockito::Server;
use olympus_sdk::services::identity::GetOrCreateIdentityRequest;
use olympus_sdk::{OlympusClient, OlympusConfig, OlympusError};
use serde_json::json;

fn make_client(base_url: &str) -> OlympusClient {
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url(base_url);
    OlympusClient::from_config(cfg)
}

const IDENTITY_FIXTURE: &str = r#"{
    "id": "olympus_id_abc",
    "firebase_uid": "firebase_xyz",
    "email": "ada@example.com",
    "phone": "+15551234567",
    "first_name": "Ada",
    "last_name": "Lovelace",
    "global_preferences": {"theme": "dark"},
    "stripe_customer_id": "cus_123",
    "created_at": "2026-04-19T00:00:00Z",
    "updated_at": "2026-04-19T00:00:00Z"
}"#;

#[tokio::test]
async fn get_or_create_from_firebase_returns_typed_identity() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/platform/identities")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(IDENTITY_FIXTURE)
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let identity = oc
        .identity()
        .get_or_create_from_firebase(GetOrCreateIdentityRequest {
            firebase_uid: "firebase_xyz",
            email: Some("ada@example.com"),
            ..Default::default()
        })
        .await
        .expect("happy path");
    assert_eq!(identity.id, "olympus_id_abc");
    assert_eq!(identity.firebase_uid, "firebase_xyz");
    assert_eq!(identity.email.as_deref(), Some("ada@example.com"));
    assert_eq!(identity.stripe_customer_id.as_deref(), Some("cus_123"));
    m.assert_async().await;
}

#[tokio::test]
async fn link_to_tenant_succeeds_on_204() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/platform/identities/links")
        .with_status(204)
        .create_async()
        .await;

    let oc = make_client(&server.url());
    oc.identity()
        .link_to_tenant("olympus_id_abc", "tenant_xyz", "cust_001")
        .await
        .expect("link");
    m.assert_async().await;
}

#[tokio::test]
async fn scan_id_serializes_image_bytes_as_json_array() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/identity/scan-id")
        .match_body(mockito::Matcher::PartialJson(json!({
            "phone": "+15551234567",
            "image": [1, 2, 3, 4]
        })))
        .with_status(200)
        .with_body(r#"{"verified": true, "age": 28}"#)
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let resp = oc
        .identity()
        .scan_id("+15551234567", &[1u8, 2, 3, 4])
        .await
        .expect("scan");
    assert_eq!(resp["verified"], json!(true));
    assert_eq!(resp["age"], json!(28));
    m.assert_async().await;
}

#[tokio::test]
async fn check_verification_status_url_encodes_phone() {
    let mut server = Server::new_async().await;
    // urlencoding turns '+' into '%2B'.
    let m = server
        .mock("GET", "/identity/status/%2B15551234567")
        .with_status(200)
        .with_body(r#"{"verified": true}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc
        .identity()
        .check_verification_status("+15551234567")
        .await
        .expect("status");
    assert_eq!(resp["verified"], json!(true));
    m.assert_async().await;
}

#[tokio::test]
async fn verify_passphrase_posts_phone_and_passphrase() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/identity/verify-passphrase")
        .match_body(mockito::Matcher::Json(
            json!({"phone": "+1555", "passphrase": "secret"}),
        ))
        .with_status(200)
        .with_body(r#"{"valid": true}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc
        .identity()
        .verify_passphrase("+1555", "secret")
        .await
        .expect("verify");
    assert_eq!(resp["valid"], json!(true));
    m.assert_async().await;
}

#[tokio::test]
async fn set_passphrase_posts_phone_and_passphrase() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/identity/set-passphrase")
        .match_body(mockito::Matcher::Json(
            json!({"phone": "+1555", "passphrase": "new-secret"}),
        ))
        .with_status(200)
        .with_body(r#"{"updated": true}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc
        .identity()
        .set_passphrase("+1555", "new-secret")
        .await
        .expect("set");
    assert_eq!(resp["updated"], json!(true));
    m.assert_async().await;
}

#[tokio::test]
async fn create_upload_session_posts_empty_body() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/identity/create-upload-session")
        .with_status(200)
        .with_body(r#"{"upload_url": "https://signed/url"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc
        .identity()
        .create_upload_session()
        .await
        .expect("session");
    assert_eq!(resp["upload_url"], json!("https://signed/url"));
    m.assert_async().await;
}

// Error path: 5xx surfaces as OlympusError::Api
#[tokio::test]
async fn check_verification_status_propagates_server_error() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/identity/status/%2B1555")
        .with_status(500)
        .with_body(r#"{"error": {"message": "internal"}}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let res = oc.identity().check_verification_status("+1555").await;
    match res {
        Err(OlympusError::Api { status, .. }) => assert_eq!(status, 500),
        other => panic!("expected Api error, got {:?}", other),
    }
    m.assert_async().await;
}
