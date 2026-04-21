//! Tests for the string-keyed scope helpers on `OlympusClient`:
//! `has_scope`, `require_scope`, `granted_scopes`.
//!
//! Complements the bitset fast-path already covered in
//! `app_scoped_permissions.rs`. See olympus-cloud-gcp#3403 §1.2.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use olympus_sdk::{OlympusClient, OlympusConfig, OlympusError, OlympusScopes};
use serde_json::{json, Value};

fn make_client() -> OlympusClient {
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url("http://ignored");
    OlympusClient::from_config(cfg)
}

fn b64url_nopad(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn make_jwt(claims: Value) -> String {
    let header = b64url_nopad(br#"{"alg":"RS256","typ":"JWT"}"#);
    let payload = b64url_nopad(claims.to_string().as_bytes());
    format!("{}.{}.sig-placeholder", header, payload)
}

// ---------------------------------------------------------------------------
// granted_scopes
// ---------------------------------------------------------------------------

#[test]
fn granted_scopes_empty_without_token() {
    let oc = make_client();
    assert!(oc.granted_scopes().is_empty());
}

#[test]
fn granted_scopes_empty_when_claim_missing() {
    let oc = make_client();
    let token = make_jwt(json!({
        "sub": "u", "tenant_id": "t", "session_id": "s",
        "roles": ["tenant_admin"],
        "iat": 0, "exp": 9999999999_i64, "iss": "i", "aud": "a",
    }));
    oc.set_access_token(token);
    assert!(oc.granted_scopes().is_empty());
}

#[test]
fn granted_scopes_empty_when_claim_not_array() {
    let oc = make_client();
    let token = make_jwt(json!({
        "sub": "u", "tenant_id": "t", "session_id": "s",
        "app_scopes": "not-an-array",
        "iat": 0, "exp": 9999999999_i64, "iss": "i", "aud": "a",
    }));
    oc.set_access_token(token);
    assert!(oc.granted_scopes().is_empty());
}

#[test]
fn granted_scopes_decodes_array_of_strings() {
    let oc = make_client();
    let token = make_jwt(json!({
        "sub": "u", "tenant_id": "t", "session_id": "s",
        "app_id": "pizza-os",
        "app_scopes": [
            "commerce.order.write@tenant",
            "commerce.order.read@tenant",
            "platform.user.profile.read@user",
        ],
        "iat": 0, "exp": 9999999999_i64, "iss": "i", "aud": "a",
    }));
    oc.set_access_token(token);
    let scopes = oc.granted_scopes();
    assert_eq!(scopes.len(), 3);
    assert!(scopes.contains("commerce.order.write@tenant"));
    assert!(scopes.contains("commerce.order.read@tenant"));
    assert!(scopes.contains("platform.user.profile.read@user"));
}

#[test]
fn granted_scopes_skips_non_string_entries() {
    let oc = make_client();
    let token = make_jwt(json!({
        "sub": "u", "tenant_id": "t", "session_id": "s",
        "app_scopes": ["valid.scope.read@user", 42, null, {"nope": true}],
        "iat": 0, "exp": 9999999999_i64, "iss": "i", "aud": "a",
    }));
    oc.set_access_token(token);
    let scopes = oc.granted_scopes();
    assert_eq!(scopes.len(), 1);
    assert!(scopes.contains("valid.scope.read@user"));
}

// ---------------------------------------------------------------------------
// has_scope
// ---------------------------------------------------------------------------

#[test]
fn has_scope_false_without_token() {
    let oc = make_client();
    assert!(!oc.has_scope("anything@user"));
}

#[test]
fn has_scope_checks_granted_set() {
    let oc = make_client();
    let token = make_jwt(json!({
        "sub": "u", "tenant_id": "t", "session_id": "s",
        "app_id": "pizza-os",
        "app_scopes": ["commerce.order.write@tenant"],
        "iat": 0, "exp": 9999999999_i64, "iss": "i", "aud": "a",
    }));
    oc.set_access_token(token);
    assert!(oc.has_scope("commerce.order.write@tenant"));
    assert!(!oc.has_scope("commerce.order.delete@tenant"));
}

#[test]
fn has_scope_works_with_generated_constants() {
    let oc = make_client();
    let token = make_jwt(json!({
        "sub": "u", "tenant_id": "t", "session_id": "s",
        "app_id": "pizza-os",
        "app_scopes": [OlympusScopes::AUTH_SESSION_READ_AT_USER],
        "iat": 0, "exp": 9999999999_i64, "iss": "i", "aud": "a",
    }));
    oc.set_access_token(token);
    assert!(oc.has_scope(OlympusScopes::AUTH_SESSION_READ_AT_USER));
    assert!(!oc.has_scope(OlympusScopes::AUTH_SESSION_DELETE_AT_USER));
}

#[test]
fn has_scope_reflects_token_rotation() {
    let oc = make_client();
    let token_a = make_jwt(json!({
        "sub": "u", "tenant_id": "t", "session_id": "s",
        "app_scopes": ["a.read@user"],
        "iat": 0, "exp": 9999999999_i64, "iss": "i", "aud": "a",
    }));
    let token_b = make_jwt(json!({
        "sub": "u", "tenant_id": "t", "session_id": "s",
        "app_scopes": ["b.read@user"],
        "iat": 0, "exp": 9999999999_i64, "iss": "i", "aud": "a",
    }));
    oc.set_access_token(token_a);
    assert!(oc.has_scope("a.read@user"));
    assert!(!oc.has_scope("b.read@user"));
    oc.set_access_token(token_b);
    assert!(!oc.has_scope("a.read@user"));
    assert!(oc.has_scope("b.read@user"));
}

// ---------------------------------------------------------------------------
// require_scope
// ---------------------------------------------------------------------------

#[test]
fn require_scope_ok_when_granted() {
    let oc = make_client();
    let token = make_jwt(json!({
        "sub": "u", "tenant_id": "t", "session_id": "s",
        "app_scopes": ["commerce.order.write@tenant"],
        "iat": 0, "exp": 9999999999_i64, "iss": "i", "aud": "a",
    }));
    oc.set_access_token(token);
    assert!(oc.require_scope("commerce.order.write@tenant").is_ok());
}

#[test]
fn require_scope_err_when_not_granted() {
    let oc = make_client();
    let token = make_jwt(json!({
        "sub": "u", "tenant_id": "t", "session_id": "s",
        "app_scopes": ["commerce.order.read@tenant"],
        "iat": 0, "exp": 9999999999_i64, "iss": "i", "aud": "a",
    }));
    oc.set_access_token(token);
    match oc.require_scope("commerce.order.delete@tenant") {
        Err(OlympusError::ScopeRequired { scope }) => {
            assert_eq!(scope, "commerce.order.delete@tenant");
        }
        other => panic!("expected ScopeRequired, got {:?}", other),
    }
}

#[test]
fn require_scope_err_without_token() {
    let oc = make_client();
    match oc.require_scope("anything.read@user") {
        Err(OlympusError::ScopeRequired { scope }) => {
            assert_eq!(scope, "anything.read@user");
        }
        other => panic!("expected ScopeRequired, got {:?}", other),
    }
}

#[test]
fn scope_required_error_has_scope_accessor() {
    let e = OlympusError::ScopeRequired {
        scope: "commerce.order.write@tenant".into(),
    };
    assert_eq!(e.scope(), Some("commerce.order.write@tenant"));
    // Display impl from thiserror.
    assert!(format!("{}", e).contains("commerce.order.write@tenant"));
}
