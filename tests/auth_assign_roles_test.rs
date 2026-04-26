//! Integration tests for AuthService.assign_roles + list_teammates wrappers
//! (W12-1 / olympus-cloud-gcp#3599 / olympus-sdk-dart#45 fanout).
//!
//! Mirrors the canonical Dart contract:
//!   POST /platform/users/{id}/roles/assign with snake_case body returning
//!   void; GET /platform/teammates returning OlympusTeammate[].
//!
//! Covers ac-1/2/3/4/6: method shape, request body wire format, success,
//! 400 ROLES_VALIDATION_ERROR, 403 INSUFFICIENT_PERMISSIONS,
//! 404 USER_NOT_FOUND mappings via OlympusError::Api.

use mockito::{Matcher, Server};
use olympus_sdk::services::auth::{AssignRolesRequest, OlympusTeammate};
use olympus_sdk::{OlympusClient, OlympusConfig, OlympusError};

fn make_client(base_url: &str) -> OlympusClient {
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url(base_url);
    OlympusClient::from_config(cfg)
}

#[tokio::test]
async fn assign_roles_posts_canonical_body() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/platform/users/u-1/roles/assign")
        .match_body(Matcher::Json(serde_json::json!({
            "tenant_id": "t-1",
            "grant_scopes": ["commerce.order.write@tenant"],
            "revoke_scopes": ["platform.policy.write@tenant"],
            "note": "rotating ops on-call",
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"ok":true,"audit_id":"aud-3599-0001"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    oc.auth()
        .assign_roles(AssignRolesRequest {
            user_id: "u-1",
            tenant_id: "t-1",
            // Duplicate intentionally — must be deduped on the wire.
            grant_scopes: &[
                "commerce.order.write@tenant",
                "commerce.order.write@tenant",
            ],
            revoke_scopes: &["platform.policy.write@tenant"],
            note: Some("rotating ops on-call"),
        })
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn assign_roles_omits_note_when_none_and_handles_empty_revoke() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/platform/users/u-2/roles/assign")
        .match_body(Matcher::Json(serde_json::json!({
            "tenant_id": "t-1",
            "grant_scopes": ["a.b.c@tenant"],
            "revoke_scopes": [],
        })))
        .with_status(200)
        .with_body("{}")
        .create_async()
        .await;
    let oc = make_client(&server.url());
    oc.auth()
        .assign_roles(AssignRolesRequest {
            user_id: "u-2",
            tenant_id: "t-1",
            grant_scopes: &["a.b.c@tenant"],
            revoke_scopes: &[],
            note: None,
        })
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn assign_roles_400_validation_error() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/platform/users/u-4/roles/assign")
        .with_status(400)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"error":{"code":"ROLES_VALIDATION_ERROR","message":"grant_scopes and revoke_scopes cannot both be empty"}}"#,
        )
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let err = oc
        .auth()
        .assign_roles(AssignRolesRequest {
            user_id: "u-4",
            tenant_id: "t-1",
            grant_scopes: &[],
            revoke_scopes: &[],
            note: None,
        })
        .await
        .expect_err("must error");
    match err {
        OlympusError::Api { status, message } => {
            assert_eq!(status, 400);
            assert!(message.contains("cannot both be empty"), "msg={message}");
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[tokio::test]
async fn assign_roles_403_forbidden() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/platform/users/u-5/roles/assign")
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"error":{"code":"INSUFFICIENT_PERMISSIONS","message":"caller lacks platform.founder.roles.assign@tenant"}}"#,
        )
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let err = oc
        .auth()
        .assign_roles(AssignRolesRequest {
            user_id: "u-5",
            tenant_id: "t-1",
            grant_scopes: &["x@tenant"],
            revoke_scopes: &[],
            note: None,
        })
        .await
        .expect_err("must error");
    match err {
        OlympusError::Api { status, .. } => assert_eq!(status, 403),
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[tokio::test]
async fn assign_roles_404_user_not_found() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/platform/users/missing/roles/assign")
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"error":{"code":"USER_NOT_FOUND","message":"user is not a member of this tenant"}}"#,
        )
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let err = oc
        .auth()
        .assign_roles(AssignRolesRequest {
            user_id: "missing",
            tenant_id: "t-1",
            grant_scopes: &["x@tenant"],
            revoke_scopes: &[],
            note: None,
        })
        .await
        .expect_err("must error");
    match err {
        OlympusError::Api { status, .. } => assert_eq!(status, 404),
        other => panic!("unexpected error variant: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// list_teammates
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_teammates_with_tenant_id_filter() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/platform/teammates")
        .match_query(Matcher::UrlEncoded("tenant_id".into(), "t-1".into()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"data":[{"user_id":"u-1","display_name":"Alice","role":"tenant_admin","assigned_scopes":["commerce.order.write@tenant"]}]}"#,
        )
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let teammates = oc
        .auth()
        .list_teammates(Some("t-1"))
        .await
        .expect("ok");
    m.assert_async().await;
    assert_eq!(teammates.len(), 1);
    assert_eq!(teammates[0].user_id, "u-1");
    assert_eq!(teammates[0].display_name, "Alice");
    assert_eq!(teammates[0].role, "tenant_admin");
    assert!(teammates[0]
        .assigned_scopes
        .contains("commerce.order.write@tenant"));
}

#[tokio::test]
async fn list_teammates_without_filter_omits_query() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/platform/teammates")
        .match_query(Matcher::Missing)
        .with_status(200)
        .with_body(r#"{"data":[]}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let teammates = oc.auth().list_teammates(None).await.expect("ok");
    m.assert_async().await;
    assert!(teammates.is_empty());
}

#[tokio::test]
async fn list_teammates_accepts_bare_array_response() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/platform/teammates")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"[{"user_id":"u-2","display_name":"Bob","role":"staff","assigned_scopes":[]}]"#,
        )
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let teammates = oc.auth().list_teammates(None).await.expect("ok");
    assert_eq!(teammates.len(), 1);
    assert_eq!(teammates[0].user_id, "u-2");
    assert!(teammates[0].assigned_scopes.is_empty());
}

#[tokio::test]
async fn list_teammates_tolerates_missing_optional_fields() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/platform/teammates")
        .with_status(200)
        .with_body(r#"{"data":[{"user_id":"u-3"}]}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let teammates = oc.auth().list_teammates(None).await.expect("ok");
    assert_eq!(teammates.len(), 1);
    let t: &OlympusTeammate = &teammates[0];
    assert_eq!(t.user_id, "u-3");
    assert_eq!(t.display_name, "");
    assert_eq!(t.role, "");
    assert!(t.assigned_scopes.is_empty());
}
