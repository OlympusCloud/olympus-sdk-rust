//! Integration tests for `AuthService::assign_roles` + `AuthService::revoke_roles`.
//!
//! Mirror of the Python SDK consumer (`OlympusCloud/olympus-sdk-python#17`) for
//! issue OlympusCloud/olympus-cloud-gcp#3458 ac-5 (Rust fanout). The behavioural
//! contract under test:
//!
//! * Wraps `POST /platform/users/{user_id}/roles/assign`.
//! * Body shape: `{ tenant_id, grant_scopes, revoke_scopes, note? }`.
//! * Arrays relay AS-IS — NO client-side dedupe / sort. The server is the
//!   single source of truth for normalization.
//! * Empty grant / revoke serialize as `[]`, NOT `null`.
//! * `note` is omitted when not supplied.
//! * Error codes surface via `OlympusError::Api { code, .. }` —
//!   `ROLES_VALIDATION_ERROR` (400), `INSUFFICIENT_PERMISSIONS` (403),
//!   `USER_NOT_FOUND` (404).
//! * `revoke_roles` is a thin wrapper that calls `assign_roles` with empty
//!   `grant_scopes`.

use std::sync::Arc;

use olympus_sdk::error::OlympusError;
use olympus_sdk::http::OlympusHttpClient;
use olympus_sdk::services::auth::{AssignRolesRequest, AuthService};
use olympus_sdk::OlympusConfig;
use serde_json::Value;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// Build an `AuthService` pointing at `mock_server.uri()`.
fn build_service(mock_server: &MockServer) -> AuthService {
    let config = OlympusConfig::new("com.test", "oc_test_key").with_base_url(mock_server.uri());
    let http = Arc::new(OlympusHttpClient::new(Arc::new(config)).expect("http client"));
    AuthService::new(http)
}

/// Build the canonical error envelope used by the gateway.
fn error_envelope(code: &str, message: &str) -> Value {
    serde_json::json!({
        "error": {
            "code": code,
            "message": message,
            "request_id": "req-test-1234",
        }
    })
}

// ---------------------------------------------------------------------------
// Happy path — grant + revoke both populated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn assign_roles_happy_path_grant_and_revoke() {
    let server = MockServer::start().await;
    let expected_body = serde_json::json!({
        "tenant_id": "tenant-1",
        "grant_scopes": ["platform.user.read@tenant", "platform.user.write@tenant"],
        "revoke_scopes": ["platform.user.delete@tenant"],
        "note": "audit sweep",
    });
    Mock::given(method("POST"))
        .and(path("/platform/users/user-42/roles/assign"))
        .and(body_json(&expected_body))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_service(&server);
    let req = AssignRolesRequest::new("tenant-1", "user-42")
        .with_grants(["platform.user.read@tenant", "platform.user.write@tenant"])
        .with_revokes(["platform.user.delete@tenant"])
        .with_note("audit sweep");

    svc.assign_roles(req).await.expect("expected 204 success");
}

// ---------------------------------------------------------------------------
// Grant-only: revoke_scopes must serialize as `[]`, not be omitted / null
// ---------------------------------------------------------------------------

#[tokio::test]
async fn assign_roles_grant_only_emits_empty_revoke_array() {
    let server = MockServer::start().await;
    let expected_body = serde_json::json!({
        "tenant_id": "tenant-1",
        "grant_scopes": ["platform.user.read@tenant"],
        "revoke_scopes": [],
    });
    Mock::given(method("POST"))
        .and(path("/platform/users/user-42/roles/assign"))
        .and(body_json(&expected_body))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_service(&server);
    let req = AssignRolesRequest::new("tenant-1", "user-42")
        .with_grants(["platform.user.read@tenant"]);

    svc.assign_roles(req).await.expect("expected 204 success");
}

// ---------------------------------------------------------------------------
// Revoke-only: grant_scopes must serialize as `[]`
// ---------------------------------------------------------------------------

#[tokio::test]
async fn assign_roles_revoke_only_emits_empty_grant_array() {
    let server = MockServer::start().await;
    let expected_body = serde_json::json!({
        "tenant_id": "tenant-1",
        "grant_scopes": [],
        "revoke_scopes": ["platform.user.delete@tenant"],
    });
    Mock::given(method("POST"))
        .and(path("/platform/users/user-42/roles/assign"))
        .and(body_json(&expected_body))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_service(&server);
    let req = AssignRolesRequest::new("tenant-1", "user-42")
        .with_revokes(["platform.user.delete@tenant"]);

    svc.assign_roles(req).await.expect("expected 204 success");
}

// ---------------------------------------------------------------------------
// PINNED INVARIANT: arrays pass through unmodified.
//
// The server owns dedupe + lex-sort. Client-side normalization would mask a
// regression if the server contract ever loosened. This test asserts the wire
// shape preserves duplicates AND original ordering. If anyone ever adds
// `.dedup()` or `.sort()` in `assign_roles`, this test breaks deliberately.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn assign_roles_passes_arrays_through_unmodified() {
    let server = MockServer::start().await;
    // Note: NOT lex-sorted, AND has duplicates. The body matcher is exact —
    // any client-side reordering or dedup will fail this test.
    let expected_body = serde_json::json!({
        "tenant_id": "tenant-1",
        "grant_scopes": [
            "z.last.read@tenant",
            "a.first.read@tenant",
            "z.last.read@tenant",
            "m.middle.read@tenant",
        ],
        "revoke_scopes": [
            "y.delete@tenant",
            "b.delete@tenant",
            "y.delete@tenant",
        ],
    });
    Mock::given(method("POST"))
        .and(path("/platform/users/user-42/roles/assign"))
        .and(body_json(&expected_body))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_service(&server);
    let req = AssignRolesRequest::new("tenant-1", "user-42")
        .with_grants([
            "z.last.read@tenant",
            "a.first.read@tenant",
            "z.last.read@tenant",
            "m.middle.read@tenant",
        ])
        .with_revokes([
            "y.delete@tenant",
            "b.delete@tenant",
            "y.delete@tenant",
        ]);

    svc.assign_roles(req).await.expect("expected 204 success");
}

// ---------------------------------------------------------------------------
// Note is omitted from the body when None
// ---------------------------------------------------------------------------

#[tokio::test]
async fn assign_roles_omits_note_when_absent() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/platform/users/user-42/roles/assign"))
        .respond_with(move |req: &Request| {
            // Verify body has NO `note` key when not supplied.
            let body: Value = serde_json::from_slice(&req.body).expect("json body");
            assert!(
                body.as_object()
                    .map(|o| !o.contains_key("note"))
                    .unwrap_or(false),
                "expected note to be absent from body, got: {body}"
            );
            ResponseTemplate::new(204)
        })
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_service(&server);
    let req = AssignRolesRequest::new("tenant-1", "user-42")
        .with_grants(["platform.user.read@tenant"]);

    svc.assign_roles(req).await.expect("expected 204 success");
}

// ---------------------------------------------------------------------------
// Error mappings
// ---------------------------------------------------------------------------

#[tokio::test]
async fn assign_roles_maps_validation_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/platform/users/user-42/roles/assign"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(error_envelope(
                    "ROLES_VALIDATION_ERROR",
                    "scope string malformed",
                )),
        )
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_service(&server);
    let req = AssignRolesRequest::new("tenant-1", "user-42").with_grants(["bad-scope"]);

    let err = svc
        .assign_roles(req)
        .await
        .expect_err("expected ROLES_VALIDATION_ERROR");

    match err {
        OlympusError::Api { status, code, .. } => {
            assert_eq!(status, 400);
            assert_eq!(code, "ROLES_VALIDATION_ERROR");
        }
        other => panic!("expected OlympusError::Api, got {other:?}"),
    }
}

#[tokio::test]
async fn assign_roles_maps_insufficient_permissions() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/platform/users/user-42/roles/assign"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(error_envelope(
                    "INSUFFICIENT_PERMISSIONS",
                    "caller lacks tenant_admin",
                )),
        )
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_service(&server);
    let req = AssignRolesRequest::new("tenant-1", "user-42")
        .with_grants(["platform.user.read@tenant"]);

    let err = svc
        .assign_roles(req)
        .await
        .expect_err("expected INSUFFICIENT_PERMISSIONS");

    match err {
        OlympusError::Api { status, code, .. } => {
            assert_eq!(status, 403);
            assert_eq!(code, "INSUFFICIENT_PERMISSIONS");
        }
        other => panic!("expected OlympusError::Api, got {other:?}"),
    }
}

#[tokio::test]
async fn assign_roles_maps_user_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/platform/users/user-42/roles/assign"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(error_envelope("USER_NOT_FOUND", "user not in tenant")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_service(&server);
    let req = AssignRolesRequest::new("tenant-1", "user-42")
        .with_grants(["platform.user.read@tenant"]);

    let err = svc
        .assign_roles(req)
        .await
        .expect_err("expected USER_NOT_FOUND");

    match err {
        OlympusError::Api { status, code, .. } => {
            assert_eq!(status, 404);
            assert_eq!(code, "USER_NOT_FOUND");
        }
        other => panic!("expected OlympusError::Api, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// revoke_roles round-trip — must hit the same endpoint with empty grant_scopes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn revoke_roles_round_trip_uses_assign_endpoint() {
    let server = MockServer::start().await;
    let expected_body = serde_json::json!({
        "tenant_id": "tenant-1",
        "grant_scopes": [],
        "revoke_scopes": ["platform.user.delete@tenant", "platform.user.write@tenant"],
        "note": "offboarding",
    });
    Mock::given(method("POST"))
        .and(path("/platform/users/user-42/roles/assign"))
        .and(body_json(&expected_body))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_service(&server);
    svc.revoke_roles(
        "tenant-1",
        "user-42",
        ["platform.user.delete@tenant", "platform.user.write@tenant"],
        Some("offboarding".into()),
    )
    .await
    .expect("revoke_roles should succeed");
}

#[tokio::test]
async fn revoke_roles_omits_note_when_none() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/platform/users/user-42/roles/assign"))
        .respond_with(move |req: &Request| {
            let body: Value = serde_json::from_slice(&req.body).expect("json body");
            assert_eq!(body["grant_scopes"], serde_json::json!([]));
            assert_eq!(
                body["revoke_scopes"],
                serde_json::json!(["platform.user.delete@tenant"])
            );
            assert!(
                body.as_object()
                    .map(|o| !o.contains_key("note"))
                    .unwrap_or(false),
                "expected note to be absent, got: {body}"
            );
            ResponseTemplate::new(204)
        })
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_service(&server);
    svc.revoke_roles(
        "tenant-1",
        "user-42",
        ["platform.user.delete@tenant"],
        None,
    )
    .await
    .expect("revoke_roles should succeed");
}

// ---------------------------------------------------------------------------
// revoke_roles propagates errors from the assign endpoint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn revoke_roles_propagates_validation_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/platform/users/user-42/roles/assign"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(error_envelope(
                    "ROLES_VALIDATION_ERROR",
                    "revoke_scopes empty",
                )),
        )
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_service(&server);
    let err = svc
        .revoke_roles("tenant-1", "user-42", Vec::<String>::new(), None)
        .await
        .expect_err("expected ROLES_VALIDATION_ERROR");

    match err {
        OlympusError::Api { status, code, .. } => {
            assert_eq!(status, 400);
            assert_eq!(code, "ROLES_VALIDATION_ERROR");
        }
        other => panic!("expected OlympusError::Api, got {other:?}"),
    }
}
