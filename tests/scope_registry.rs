//! Tests for `PlatformService::list_scope_registry` +
//! `get_scope_registry_digest` (gcp#3517).
//!
//! Wire contract:
//!
//!   - GET /platform/scope-registry?namespace=&owner_app_id=&include_drafts=
//!     -> { scopes: ScopeRow[], total: usize }
//!   - GET /platform/scope-registry/digest
//!     -> { platform_catalog_digest: hex, row_count: usize }

use mockito::{Matcher, Server};
use olympus_sdk::services::platform::ListScopeRegistryParams;
use olympus_sdk::{OlympusClient, OlympusConfig};
use serde_json::json;

fn make_client(base_url: &str) -> OlympusClient {
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url(base_url);
    OlympusClient::from_config(cfg)
}

// ---------------------------------------------------------------------------
// PlatformService::list_scope_registry
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_scope_registry_no_filters_returns_full_catalog() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/platform/scope-registry")
        .match_query("")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "scopes": [
                    {
                        "scope": "auth.session.read@user",
                        "resource": "session",
                        "action": "read",
                        "holder": "user",
                        "namespace": "auth",
                        "owner_app_id": null,
                        "description": "Read your own session metadata",
                        "is_destructive": false,
                        "requires_mfa": false,
                        "grace_behavior": "extend",
                        "consent_prompt_copy": "View your session",
                        "workshop_status": "approved",
                        "bit_id": 0
                    },
                    {
                        "scope": "voice.call.write@tenant",
                        "resource": "call",
                        "action": "write",
                        "holder": "tenant",
                        "namespace": "voice",
                        "owner_app_id": "orderecho-ai",
                        "description": "Place outbound voice calls on the tenant",
                        "is_destructive": true,
                        "requires_mfa": true,
                        "grace_behavior": "deny",
                        "consent_prompt_copy": "Place outbound calls",
                        "workshop_status": "service_ok",
                        "bit_id": 12
                    }
                ],
                "total": 2
            })
            .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let listing = oc
        .platform()
        .list_scope_registry(ListScopeRegistryParams::default())
        .await
        .unwrap();
    assert_eq!(listing.total, 2);
    assert_eq!(listing.scopes.len(), 2);
    assert_eq!(listing.scopes[0].scope, "auth.session.read@user");
    assert_eq!(listing.scopes[0].bit_id, Some(0));
    assert_eq!(listing.scopes[0].owner_app_id, None);
    assert_eq!(listing.scopes[1].owner_app_id.as_deref(), Some("orderecho-ai"));
    assert!(listing.scopes[1].is_destructive);
    assert!(listing.scopes[1].requires_mfa);
    assert_eq!(listing.scopes[1].bit_id, Some(12));
    m.assert_async().await;
}

#[tokio::test]
async fn list_scope_registry_filters_forwarded_as_query_params() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/platform/scope-registry")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("namespace".into(), "voice".into()),
            Matcher::UrlEncoded("owner_app_id".into(), "orderecho-ai".into()),
            Matcher::UrlEncoded("include_drafts".into(), "true".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({"scopes": [], "total": 0}).to_string())
        .create_async()
        .await;

    let oc = make_client(&server.url());
    oc.platform()
        .list_scope_registry(ListScopeRegistryParams {
            namespace: Some("voice".into()),
            owner_app_id: Some("orderecho-ai".into()),
            include_drafts: true,
        })
        .await
        .unwrap();
    m.assert_async().await;
}

#[tokio::test]
async fn list_scope_registry_owner_app_id_empty_forwarded_distinctly() {
    // owner_app_id = Some("") MUST round-trip as `owner_app_id=` (empty
    // value) — the server interprets it as the explicit "platform-owned
    // only" filter, semantically distinct from None (no filter).
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/platform/scope-registry")
        .match_query(Matcher::UrlEncoded("owner_app_id".into(), "".into()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({"scopes": [], "total": 0}).to_string())
        .create_async()
        .await;

    let oc = make_client(&server.url());
    oc.platform()
        .list_scope_registry(ListScopeRegistryParams {
            owner_app_id: Some("".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    m.assert_async().await;
}

#[tokio::test]
async fn list_scope_registry_owner_app_id_omitted_when_none() {
    // owner_app_id = None must NOT add the key to the query string.
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/platform/scope-registry")
        .match_query(Matcher::AllOf(vec![Matcher::UrlEncoded(
            "namespace".into(),
            "platform".into(),
        )]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({"scopes": [], "total": 0}).to_string())
        .create_async()
        .await;

    let oc = make_client(&server.url());
    oc.platform()
        .list_scope_registry(ListScopeRegistryParams {
            namespace: Some("platform".into()),
            owner_app_id: None,
            include_drafts: false,
        })
        .await
        .unwrap();
    m.assert_async().await;
}

#[tokio::test]
async fn list_scope_registry_bit_id_null_tolerated_for_pre_allocation() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/platform/scope-registry")
        .match_query(Matcher::UrlEncoded("include_drafts".into(), "true".into()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "scopes": [{
                    "scope": "creator.draft.write@tenant",
                    "resource": "draft",
                    "action": "write",
                    "holder": "tenant",
                    "namespace": "creator",
                    "owner_app_id": null,
                    "description": "",
                    "is_destructive": false,
                    "requires_mfa": false,
                    "grace_behavior": "extend",
                    "consent_prompt_copy": "",
                    "workshop_status": "pending",
                    "bit_id": null
                }],
                "total": 1
            })
            .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let listing = oc
        .platform()
        .list_scope_registry(ListScopeRegistryParams {
            include_drafts: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(listing.scopes.len(), 1);
    assert_eq!(listing.scopes[0].bit_id, None);
    assert_eq!(listing.scopes[0].workshop_status, "pending");
    m.assert_async().await;
}

// ---------------------------------------------------------------------------
// PlatformService::get_scope_registry_digest
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_scope_registry_digest_parses_hex_and_count() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/platform/scope-registry/digest")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "platform_catalog_digest":
                    "12398a9b0517a3576d0e4d88807a34573d940aaada6bb61def2d540009c7bc19",
                "row_count": 3
            })
            .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let digest = oc.platform().get_scope_registry_digest().await.unwrap();
    assert_eq!(
        digest.platform_catalog_digest,
        "12398a9b0517a3576d0e4d88807a34573d940aaada6bb61def2d540009c7bc19"
    );
    assert_eq!(digest.row_count, 3);
    m.assert_async().await;
}

#[tokio::test]
async fn get_scope_registry_digest_empty_catalog() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/platform/scope-registry/digest")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "platform_catalog_digest":
                    "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945",
                "row_count": 0
            })
            .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let digest = oc.platform().get_scope_registry_digest().await.unwrap();
    assert_eq!(digest.row_count, 0);
    assert_eq!(digest.platform_catalog_digest.len(), 64); // sha256 hex
    m.assert_async().await;
}
