//! Integration tests for app-scoped permissions (olympus-cloud-gcp#3254).

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use mockito::Server;
use olympus_sdk::services::consent::Holder;
use olympus_sdk::{OlympusClient, OlympusConfig, OlympusError};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn make_client(base_url: &str) -> OlympusClient {
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url(base_url);
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

fn make_bitset(bits: &[usize], size_bytes: usize) -> String {
    let mut buf = vec![0u8; size_bytes];
    for b in bits {
        buf[b / 8] |= 1 << (b % 8);
    }
    b64url_nopad(&buf)
}

// ---------------------------------------------------------------------------
// Client fast-path helpers
// ---------------------------------------------------------------------------

#[test]
fn has_scope_bit_returns_false_without_token() {
    let oc = make_client("http://ignored");
    assert!(!oc.has_scope_bit(0));
    assert!(!oc.has_scope_bit(1023));
    assert!(!oc.is_app_scoped());
}

#[test]
fn platform_shell_token_has_no_app_claims() {
    let oc = make_client("http://ignored");
    let token = make_jwt(json!({
        "sub": "u", "tenant_id": "t", "session_id": "s",
        "roles": ["tenant_admin"],
        "iat": 0, "exp": 9999999999_i64, "iss": "i", "aud": "a",
    }));
    oc.set_access_token(token);
    assert!(!oc.has_scope_bit(0));
    assert!(!oc.is_app_scoped());
}

#[test]
fn app_scoped_token_has_bit_set_and_unset() {
    let oc = make_client("http://ignored");
    let bitset = make_bitset(&[0, 7, 8, 127, 1023], 128);
    let token = make_jwt(json!({
        "sub": "u", "tenant_id": "t", "session_id": "s",
        "roles": ["staff"],
        "app_id": "pizza-os",
        "app_scopes_bitset": bitset,
        "platform_catalog_digest": "d1",
        "app_catalog_digest": "d2",
        "iat": 0, "exp": 9999999999_i64, "iss": "i", "aud": "a",
    }));
    oc.set_access_token(token);

    assert!(oc.is_app_scoped());
    for &b in &[0usize, 7, 8, 127, 1023] {
        assert!(oc.has_scope_bit(b), "bit {} should be set", b);
    }
    for &b in &[1usize, 6, 9, 500] {
        assert!(!oc.has_scope_bit(b), "bit {} should be unset", b);
    }
    assert!(!oc.has_scope_bit(2048));
}

#[test]
fn token_switch_reflects_new_bitset() {
    let oc = make_client("http://ignored");
    let a = make_jwt(json!({
        "sub": "u", "tenant_id": "t", "session_id": "s", "roles": [],
        "app_id": "a", "app_scopes_bitset": make_bitset(&[0], 128),
        "iat": 0, "exp": 9999999999_i64, "iss": "i", "aud": "a",
    }));
    let b = make_jwt(json!({
        "sub": "u", "tenant_id": "t", "session_id": "s", "roles": [],
        "app_id": "b", "app_scopes_bitset": make_bitset(&[5], 128),
        "iat": 0, "exp": 9999999999_i64, "iss": "i", "aud": "a",
    }));
    oc.set_access_token(a);
    assert!(oc.has_scope_bit(0));
    assert!(!oc.has_scope_bit(5));
    oc.set_access_token(b);
    assert!(!oc.has_scope_bit(0));
    assert!(oc.has_scope_bit(5));
}

// ---------------------------------------------------------------------------
// Error-routing tests — each asserts a typed OlympusError variant for the
// matching server error code.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scope_not_granted_routes_to_consent_required() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/api/v1/platform/apps/pizza-os/tenant-grants")
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(json!({"error":{"code":"scope_not_granted","message":"commerce.order.write required","scope":"commerce.order.write@tenant","consent_url":"https://platform/authorize"}}).to_string())
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let res = oc
        .consent()
        .list_granted("pizza-os", None, Holder::Tenant)
        .await;
    match res {
        Err(OlympusError::ConsentRequired {
            scope, consent_url, ..
        }) => {
            assert_eq!(scope, "commerce.order.write@tenant");
            assert_eq!(consent_url.as_deref(), Some("https://platform/authorize"));
        }
        other => panic!("expected ConsentRequired, got {:?}", other),
    }
    m.assert_async().await;
}

#[tokio::test]
async fn billing_grace_pulls_header_fallbacks() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/api/v1/platform/exceptions")
        .with_status(402)
        .with_header("content-type", "application/json")
        .with_header("X-Olympus-Grace-Until", "2026-04-25T00:00:00Z")
        .with_header("X-Olympus-Upgrade-URL", "https://billing/upgrade")
        .with_body(
            json!({"error":{"code":"billing_grace_exceeded","message":"lapsed"}}).to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let res = oc.governance().list_exceptions(None, None).await;
    match res {
        Err(OlympusError::BillingGraceExceeded {
            grace_until,
            upgrade_url,
            ..
        }) => {
            assert_eq!(grace_until.as_deref(), Some("2026-04-25T00:00:00Z"));
            assert_eq!(upgrade_url.as_deref(), Some("https://billing/upgrade"));
        }
        other => panic!("expected BillingGraceExceeded, got {:?}", other),
    }
    m.assert_async().await;
}

#[tokio::test]
async fn webauthn_required_routes_to_device_changed() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/platform/consent-prompt?app_id=aura-ai&scope=aura.calendar.read%40user")
        .with_status(401)
        .with_header("content-type", "application/json")
        .with_body(json!({"error":{"code":"webauthn_required","message":"new device","challenge":"abc"},"requires_reconsent":true}).to_string())
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let res = oc
        .consent()
        .describe("aura-ai", "aura.calendar.read@user")
        .await;
    match res {
        Err(OlympusError::DeviceChanged {
            challenge,
            requires_reconsent,
            ..
        }) => {
            assert_eq!(challenge, "abc");
            assert!(requires_reconsent);
        }
        other => panic!("expected DeviceChanged, got {:?}", other),
    }
    m.assert_async().await;
}

// ---------------------------------------------------------------------------
// Stale-catalog header debounce
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stale_catalog_handler_fires_once_per_token() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc as StdArc;
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/api/v1/platform/apps/pizza-os/tenant-grants")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("X-Olympus-Catalog-Stale", "true")
        .with_body(r#"{"grants":[]}"#)
        .expect(4)
        .create_async()
        .await;

    let oc = make_client(&server.url());
    // Set an access token so debounce has a key.
    oc.set_access_token("stale-token-v1");

    let counter = StdArc::new(AtomicUsize::new(0));
    let counter_clone = StdArc::clone(&counter);
    oc.on_catalog_stale(Some(StdArc::new(move || {
        counter_clone.fetch_add(1, Ordering::SeqCst);
    })));

    // Three requests against the same token — handler fires once.
    for _ in 0..3 {
        let _ = oc
            .consent()
            .list_granted("pizza-os", None, Holder::Tenant)
            .await
            .unwrap();
    }
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    // Rotate the token — handler fires again on next stale response.
    oc.set_access_token("stale-token-v2");
    let _ = oc
        .consent()
        .list_granted("pizza-os", None, Holder::Tenant)
        .await
        .unwrap();
    assert_eq!(counter.load(Ordering::SeqCst), 2);
    m.assert_async().await;
}

// ---------------------------------------------------------------------------
// X-App-Token attachment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn app_token_attached_when_set() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/api/v1/platform/apps/pizza-os/tenant-grants")
        .match_header("X-App-Token", "app-jwt-xyz")
        .with_status(200)
        .with_body(r#"{"grants":[]}"#)
        .create_async()
        .await;

    let oc = make_client(&server.url());
    oc.set_app_token("app-jwt-xyz");
    let _ = oc
        .consent()
        .list_granted("pizza-os", None, Holder::Tenant)
        .await
        .unwrap();
    m.assert_async().await;
}

// ---------------------------------------------------------------------------
// Governance client-side validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn request_exception_rejects_short_justification() {
    let oc = make_client("http://ignored");
    let res = oc
        .governance()
        .request_exception(
            "session_ttl_role_ceiling",
            json!({"role": "staff", "max_seconds": 54000}),
            "too short",
            None,
        )
        .await;
    match res {
        Err(OlympusError::Config(msg)) => assert!(
            msg.contains(">= 100 chars"),
            "expected length error, got: {}",
            msg
        ),
        other => panic!("expected Config error, got {:?}", other),
    }
}

#[test]
fn error_scope_accessor_returns_correct_value() {
    let e = OlympusError::ConsentRequired {
        scope: "commerce.order.write@tenant".into(),
        consent_url: None,
        message: "".into(),
        status: 403,
        request_id: None,
    };
    assert_eq!(e.scope(), Some("commerce.order.write@tenant"));
    let e2 = OlympusError::ScopeDenied {
        scope: "pizza.orders.refund@tenant".into(),
        message: "".into(),
        status: 403,
        request_id: None,
    };
    assert_eq!(e2.scope(), Some("pizza.orders.refund@tenant"));
    let e3 = OlympusError::AuthExpired;
    assert_eq!(e3.scope(), None);
}
