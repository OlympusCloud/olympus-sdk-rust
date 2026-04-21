//! Integration tests for the IdentityApi invite surface (#3403 §4.2 + §4.4).
//!
//! Uses `wiremock` per the task spec. Distinct from `identity_test.rs`,
//! which covers the pre-existing global Olympus-ID / age-verification
//! [`olympus_sdk::services::identity::IdentityService`].

use olympus_sdk::identity::{InviteCreateRequest, InviteStatus};
use olympus_sdk::{OlympusClient, OlympusConfig};
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn new_client_with_mock_server() -> (OlympusClient, MockServer) {
    let server = MockServer::start().await;
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url(server.uri());
    let client = OlympusClient::from_config(cfg);
    (client, server)
}

// ---------------------------------------------------------------------------
// invite
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn invite_posts_request_and_returns_handle_with_token() {
    let (client, server) = new_client_with_mock_server().await;
    let response = json!({
        "id": "inv_abc",
        "token": "eyJhbGciOi...signed.invite.jwt",
        "email": "staff@pizza.shop",
        "role": "manager",
        "location_id": "loc_123",
        "tenant_id": "t_pizza",
        "expires_at": "2026-04-28T00:00:00Z",
        "status": "pending",
        "created_at": "2026-04-21T00:00:00Z",
    });

    Mock::given(method("POST"))
        .and(path("/identity/invite"))
        .and(body_partial_json(json!({
            "email": "staff@pizza.shop",
            "role": "manager",
            "location_id": "loc_123",
            "ttl_seconds": 86400,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .mount(&server)
        .await;

    let handle = client
        .identity_invites()
        .invite(InviteCreateRequest {
            email: "staff@pizza.shop".into(),
            role: "manager".into(),
            location_id: Some("loc_123".into()),
            message: None,
            ttl_seconds: Some(86400),
        })
        .await
        .expect("invite");
    assert_eq!(handle.id, "inv_abc");
    assert_eq!(
        handle.token.as_deref(),
        Some("eyJhbGciOi...signed.invite.jwt")
    );
    assert_eq!(handle.status, InviteStatus::Pending);
}

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn list_returns_invites_without_tokens() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("GET"))
        .and(path("/identity/invites"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": "inv_1",
                "email": "a@b.co",
                "role": "manager",
                "tenant_id": "t_1",
                "expires_at": "2026-04-28T00:00:00Z",
                "status": "pending",
                "created_at": "2026-04-21T00:00:00Z",
            },
            {
                "id": "inv_2",
                "email": "c@d.co",
                "role": "staff",
                "tenant_id": "t_1",
                "expires_at": "2026-04-25T00:00:00Z",
                "status": "accepted",
                "created_at": "2026-04-20T00:00:00Z",
                "accepted_at": "2026-04-21T00:00:00Z",
            },
            {
                "id": "inv_3",
                "email": "e@f.co",
                "role": "viewer",
                "tenant_id": "t_1",
                "expires_at": "2026-04-21T00:00:00Z",
                "status": "revoked",
                "created_at": "2026-04-19T00:00:00Z",
            }
        ])))
        .mount(&server)
        .await;

    let invites = client.identity_invites().list().await.expect("list");
    assert_eq!(invites.len(), 3);
    assert!(invites.iter().all(|i| i.token.is_none()));
    assert_eq!(invites[0].status, InviteStatus::Pending);
    assert_eq!(invites[1].status, InviteStatus::Accepted);
    assert_eq!(invites[2].status, InviteStatus::Revoked);
}

// ---------------------------------------------------------------------------
// accept
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn accept_url_encodes_token_and_posts_firebase_id_token() {
    let (client, server) = new_client_with_mock_server().await;
    // `+` and `/` in the token must be percent-encoded.
    let token = "abc+def/ghi";
    Mock::given(method("POST"))
        .and(path("/identity/invites/abc%2Bdef%2Fghi/accept"))
        .and(body_partial_json(json!({
            "firebase_id_token": "fb-id-token-xyz",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "new-access",
            "refresh_token": "new-refresh",
            "token_type": "Bearer",
            "expires_in": 3600,
            "user": {"id": "u_accepted"}
        })))
        .mount(&server)
        .await;

    let resp = client
        .identity_invites()
        .accept(token, "fb-id-token-xyz")
        .await
        .expect("accept");
    assert_eq!(resp["access_token"], json!("new-access"));
    assert_eq!(resp["user"]["id"], json!("u_accepted"));
}

// ---------------------------------------------------------------------------
// revoke
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn revoke_posts_to_invite_id_and_returns_updated_handle() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/identity/invites/inv_abc/revoke"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "inv_abc",
            "email": "a@b.co",
            "role": "manager",
            "tenant_id": "t_1",
            "expires_at": "2026-04-28T00:00:00Z",
            "status": "revoked",
            "created_at": "2026-04-21T00:00:00Z",
        })))
        .mount(&server)
        .await;

    let handle = client
        .identity_invites()
        .revoke("inv_abc")
        .await
        .expect("revoke");
    assert_eq!(handle.status, InviteStatus::Revoked);
    assert!(handle.token.is_none());
}

// ---------------------------------------------------------------------------
// remove_from_tenant
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn remove_from_tenant_posts_user_id_and_reason() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/identity/remove_from_tenant"))
        .and(body_partial_json(json!({
            "user_id": "u-evil",
            "reason": "terminated for cause",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tenant_id": "t_1",
            "user_id": "u-evil",
            "removed_at": "2026-04-21T00:00:00Z",
        })))
        .mount(&server)
        .await;

    let resp = client
        .identity_invites()
        .remove_from_tenant("u-evil", Some("terminated for cause"))
        .await
        .expect("remove");
    assert_eq!(resp.user_id, "u-evil");
    assert_eq!(resp.tenant_id, "t_1");
}

#[tokio::test(flavor = "multi_thread")]
async fn remove_from_tenant_allows_null_reason() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/identity/remove_from_tenant"))
        .and(body_partial_json(json!({
            "user_id": "u-1",
            "reason": null,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tenant_id": "t_1",
            "user_id": "u-1",
            "removed_at": "2026-04-21T00:00:00Z",
        })))
        .mount(&server)
        .await;

    client
        .identity_invites()
        .remove_from_tenant("u-1", None)
        .await
        .expect("remove");
}

// ---------------------------------------------------------------------------
// Error propagation
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn invite_surfaces_403_as_api_error() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/identity/invite"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "error": {"message": "manager or tenant_admin role required"}
        })))
        .mount(&server)
        .await;
    let res = client
        .identity_invites()
        .invite(InviteCreateRequest {
            email: "a@b.co".into(),
            role: "manager".into(),
            location_id: None,
            message: None,
            ttl_seconds: None,
        })
        .await;
    match res {
        Err(olympus_sdk::OlympusError::Api { status, .. }) => assert_eq!(status, 403),
        other => panic!("expected Api 403, got {:?}", other),
    }
}
