//! Integration tests for the App-Scoped Permissions Wave 14c SDK fanout.
//!
//! Covers:
//!
//! * `AuthService::mint_app_token` — POST `/auth/app-tokens/mint` (#3781)
//! * `AuthService::refresh_app_token` — POST `/auth/app-tokens/refresh` (#3781)
//! * `AuthService::get_app_jwks` — GET `/.well-known/app-keys/{app_id}` (#3788)
//! * `PlatformService::onboard_app` — POST `/platform/apps/onboard` (#3810)
//! * `PlatformService::submit_consent` — POST `/platform/authorize/consent` (#3804)
//! * `PlatformService::submit_grant` — POST `/platform/authorize/grant` (#3808)
//! * `PlatformService::exchange_authorization_code` — POST `/platform/authorize/exchange` (#3808)
//! * `PlatformService::get_grants_graph` — GET `/platform/admin/grants/graph` (#3806)
//!
//! Wire-shape contract: every payload here byte-matches the request /
//! response structs in
//! `backend/rust/auth/src/handlers/app_tokens.rs`,
//! `backend/rust/auth/src/services/signing_keys.rs`,
//! `backend/rust/platform/src/handlers/apps_onboard.rs`,
//! `backend/rust/platform/src/handlers/authorize.rs`,
//! `backend/rust/platform/src/handlers/authorize_oauth.rs`, and
//! `backend/rust/platform/src/handlers/grant_graph_projection.rs`.

use std::sync::Arc;

use olympus_sdk::error::OlympusError;
use olympus_sdk::http::OlympusHttpClient;
use olympus_sdk::services::auth::{
    AppJwksResponse, AuthService, MintAppTokenRequest, RefreshAppTokenRequest,
};
use olympus_sdk::services::platform::{
    ConsentForm, ExchangeRequest, GrantForm, GrantsGraphQuery, OnboardRequest, OnboardStatus,
    PlatformService,
};
use olympus_sdk::OlympusConfig;
use serde_json::{json, Value};
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

// ---------------------------------------------------------------------------
// Test scaffolding
// ---------------------------------------------------------------------------

fn build_clients(server: &MockServer) -> (AuthService, PlatformService) {
    let config = OlympusConfig::new("com.test", "oc_test_key").with_base_url(server.uri());
    let http = Arc::new(OlympusHttpClient::new(Arc::new(config)).expect("http client"));
    (AuthService::new(http.clone()), PlatformService::new(http))
}

fn error_envelope(code: &str, message: &str) -> Value {
    json!({
        "error": {
            "code": code,
            "message": message,
            "request_id": "req-test-asp",
        }
    })
}

// ---------------------------------------------------------------------------
// /auth/app-tokens/mint (#3781)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mint_app_token_happy_path_includes_install_id() {
    let server = MockServer::start().await;
    let expected_body = json!({
        "app_check_token": "appcheck-token-abc",
        "firebase_install_id": "fis-id-123",
    });
    Mock::given(method("POST"))
        .and(path("/auth/app-tokens/mint"))
        .and(body_json(&expected_body))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "acc-1",
            "refresh_token": "ref-1",
            "expires_in": 900,
            "token_type": "App-JWT",
        })))
        .expect(1)
        .mount(&server)
        .await;

    let (auth, _) = build_clients(&server);
    let req = MintAppTokenRequest::new("appcheck-token-abc").with_firebase_install_id("fis-id-123");
    let resp = auth.mint_app_token(req).await.expect("mint succeeds");
    assert_eq!(resp.access_token, "acc-1");
    assert_eq!(resp.refresh_token, "ref-1");
    assert_eq!(resp.expires_in, 900);
    assert_eq!(resp.token_type, "App-JWT");
}

#[tokio::test]
async fn mint_app_token_omits_install_id_when_absent() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/app-tokens/mint"))
        .respond_with(move |req: &Request| {
            let body: Value = serde_json::from_slice(&req.body).expect("json body");
            assert_eq!(body["app_check_token"], "tk");
            assert!(
                body.as_object()
                    .map(|o| !o.contains_key("firebase_install_id"))
                    .unwrap_or(false),
                "firebase_install_id must be absent when None, got: {body}"
            );
            ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "a",
                "refresh_token": "r",
                "expires_in": 900,
                "token_type": "App-JWT",
            }))
        })
        .expect(1)
        .mount(&server)
        .await;

    let (auth, _) = build_clients(&server);
    let req = MintAppTokenRequest::new("tk");
    auth.mint_app_token(req).await.expect("ok");
}

#[tokio::test]
async fn mint_app_token_maps_app_check_invalid_401() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/app-tokens/mint"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(error_envelope("app_check_invalid", "bad signature")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let (auth, _) = build_clients(&server);
    let err = auth
        .mint_app_token(MintAppTokenRequest::new("bad"))
        .await
        .expect_err("expected app_check_invalid");
    match err {
        OlympusError::Api { status, code, .. } => {
            assert_eq!(status, 401);
            assert_eq!(code, "app_check_invalid");
        }
        other => panic!("expected Api error, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// /auth/app-tokens/refresh (#3781)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn refresh_app_token_happy_path() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/app-tokens/refresh"))
        .and(body_json(json!({"refresh_token": "old-refresh"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "new-access",
            "refresh_token": "new-refresh",
            "expires_in": 900,
            "token_type": "App-JWT",
        })))
        .expect(1)
        .mount(&server)
        .await;

    let (auth, _) = build_clients(&server);
    let resp = auth
        .refresh_app_token(RefreshAppTokenRequest::new("old-refresh"))
        .await
        .expect("refresh ok");
    assert_eq!(resp.access_token, "new-access");
    assert_eq!(resp.refresh_token, "new-refresh");
}

#[tokio::test]
async fn refresh_app_token_maps_reuse_detected_401() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/app-tokens/refresh"))
        .respond_with(ResponseTemplate::new(401).set_body_json(error_envelope(
            "refresh_reuse_detected",
            "rotation reuse detected; family burned",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let (auth, _) = build_clients(&server);
    let err = auth
        .refresh_app_token(RefreshAppTokenRequest::new("burned"))
        .await
        .expect_err("expected reuse error");
    match err {
        OlympusError::Api { status, code, .. } => {
            assert_eq!(status, 401);
            assert_eq!(code, "refresh_reuse_detected");
        }
        other => panic!("expected Api error, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// /.well-known/app-keys/{app_id} (#3788)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_app_jwks_returns_public_keys() {
    let server = MockServer::start().await;
    let body = json!({
        "keys": [
            {
                "kty": "OKP",
                "crv": "Ed25519",
                "x": "MCowBQYDK2VwAyEAabc",
                "alg": "EdDSA",
                "use": "sig",
                "kid": "kid-active-1",
                "status": "active",
            },
            {
                "kty": "OKP",
                "crv": "Ed25519",
                "x": "MCowBQYDK2VwAyEAxyz",
                "alg": "EdDSA",
                "use": "sig",
                "kid": "kid-retired-7d",
                "status": "retired_overlap",
            },
        ]
    });
    Mock::given(method("GET"))
        .and(path("/.well-known/app-keys/com.test.app"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;

    let (auth, _) = build_clients(&server);
    let jwks: AppJwksResponse = auth.get_app_jwks("com.test.app").await.expect("jwks ok");
    assert_eq!(jwks.keys.len(), 2);
    assert_eq!(jwks.keys[0].kid, "kid-active-1");
    assert_eq!(jwks.keys[0].crv, "Ed25519");
    assert_eq!(jwks.keys[0].alg, "EdDSA");
    assert_eq!(jwks.keys[0].use_, "sig");
    assert_eq!(jwks.keys[0].status.as_deref(), Some("active"));
    assert_eq!(jwks.keys[1].status.as_deref(), Some("retired_overlap"));
}

#[tokio::test]
async fn get_app_jwks_maps_404() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/app-keys/missing-app"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(error_envelope("APP_NOT_FOUND", "no app")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let (auth, _) = build_clients(&server);
    let err = auth
        .get_app_jwks("missing-app")
        .await
        .expect_err("expected 404");
    match err {
        OlympusError::Api { status, code, .. } => {
            assert_eq!(status, 404);
            assert_eq!(code, "APP_NOT_FOUND");
        }
        other => panic!("expected Api error, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// /platform/apps/onboard (#3810)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn onboard_app_happy_path_returns_plaintext_key() {
    let server = MockServer::start().await;
    let manifest = json!({
        "app_id": "com.example.demo",
        "version": "1.0.0",
        "name": "Demo",
        "publisher": "Example",
        "scopes_required": ["pizza.menu.read"],
        "scopes_optional": [],
    });
    let expected_body = json!({
        "manifest": manifest,
        "cors_origins": ["https://app.example.com"],
        "api_key_label": "production",
    });
    Mock::given(method("POST"))
        .and(path("/platform/apps/onboard"))
        .and(body_json(&expected_body))
        .and(header("Authorization", "Bearer oc_test_key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "app_id": "com.example.demo",
            "status": "onboarded",
            "api_key": "osk_abcdefABCDEF012345_plaintext_once",
            "api_key_prefix": "osk_abcdefAB",
            "api_key_id": "key-uuid-1",
            "cors_origins_pending": ["https://app.example.com"],
            "cors_followup_issue_url": "https://github.com/OlympusCloud/olympus-cloud-gcp/issues/3281",
            "signing_key_seed_event_published": true,
            "validator_warnings": []
        })))
        .expect(1)
        .mount(&server)
        .await;

    let (_, platform) = build_clients(&server);
    let req = OnboardRequest::new(manifest.clone())
        .with_cors_origins(["https://app.example.com"])
        .with_api_key_label("production");
    let resp = platform.onboard_app(req).await.expect("onboard ok");
    assert_eq!(resp.app_id, "com.example.demo");
    assert_eq!(resp.status, OnboardStatus::Onboarded);
    assert!(resp.api_key.starts_with("osk_"));
    assert_eq!(resp.api_key_prefix, "osk_abcdefAB");
    assert!(resp.signing_key_seed_event_published);
    assert_eq!(resp.cors_origins_pending, vec!["https://app.example.com"]);
}

#[tokio::test]
async fn onboard_app_idempotent_re_onboard_returns_already_onboarded() {
    let server = MockServer::start().await;
    let manifest = json!({"app_id": "com.example.demo"});
    Mock::given(method("POST"))
        .and(path("/platform/apps/onboard"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "app_id": "com.example.demo",
            "status": "already_onboarded",
            "api_key": "osk_freshkey",
            "api_key_prefix": "osk_freshk",
            "api_key_id": "key-uuid-2",
            "cors_origins_pending": [],
            "signing_key_seed_event_published": true,
            "validator_warnings": [],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let (_, platform) = build_clients(&server);
    let resp = platform
        .onboard_app(OnboardRequest::new(manifest))
        .await
        .expect("ok");
    assert_eq!(resp.status, OnboardStatus::AlreadyOnboarded);
}

#[tokio::test]
async fn onboard_app_omits_optional_fields_when_unset() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/platform/apps/onboard"))
        .respond_with(move |req: &Request| {
            let body: Value = serde_json::from_slice(&req.body).expect("json");
            // cors_origins is `Vec<String>` with `skip_serializing_if=Vec::is_empty`,
            // and api_key_label is `Option` skipped when None — both must be absent.
            let obj = body.as_object().expect("object");
            assert!(
                !obj.contains_key("cors_origins"),
                "cors_origins should be omitted when empty"
            );
            assert!(
                !obj.contains_key("api_key_label"),
                "api_key_label should be omitted when None"
            );
            ResponseTemplate::new(200).set_body_json(json!({
                "app_id": "com.example.demo",
                "status": "onboarded",
                "api_key": "osk_x",
                "api_key_prefix": "osk_x",
                "api_key_id": "k",
                "cors_origins_pending": [],
                "signing_key_seed_event_published": false,
                "validator_warnings": [],
            }))
        })
        .expect(1)
        .mount(&server)
        .await;

    let (_, platform) = build_clients(&server);
    let resp = platform
        .onboard_app(OnboardRequest::new(json!({"app_id": "com.example.demo"})))
        .await
        .expect("ok");
    assert!(!resp.signing_key_seed_event_published);
}

#[tokio::test]
async fn onboard_app_maps_validation_failure_422() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/platform/apps/onboard"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({
            "error": {
                "code": "MANIFEST_INVALID",
                "message": "manifest_validator: scope com.bogus not in catalog",
                "request_id": "req-onboard-1"
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let (_, platform) = build_clients(&server);
    let err = platform
        .onboard_app(OnboardRequest::new(json!({})))
        .await
        .expect_err("expected 422");
    match err {
        OlympusError::Api { status, code, .. } => {
            assert_eq!(status, 422);
            assert_eq!(code, "MANIFEST_INVALID");
        }
        other => panic!("expected Api, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// /platform/authorize/consent (#3804) — form encoded, 303 redirect
// ---------------------------------------------------------------------------

#[tokio::test]
async fn submit_consent_authorize_branch_extracts_grant_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/platform/authorize/consent"))
        .and(header("Content-Type", "application/x-www-form-urlencoded"))
        .respond_with(move |req: &Request| {
            // Body shape is form-urlencoded — just confirm the action key
            // is wired through and the request reached us.
            let body = std::str::from_utf8(&req.body).unwrap_or("");
            assert!(
                body.contains("action=authorize"),
                "expected action=authorize in form body, got: {body}"
            );
            assert!(body.contains("app_id=com.example.demo"));
            ResponseTemplate::new(303).insert_header(
                "Location",
                "pizzaos://settings/voice-agents?grant_id=grant-uuid-77&state=csrf-state-77",
            )
        })
        .expect(1)
        .mount(&server)
        .await;

    let (_, platform) = build_clients(&server);
    let form = ConsentForm {
        app_id: "com.example.demo".into(),
        scopes: "pizza.menu.read,pizza.hours.read".into(),
        return_to: "pizzaos://settings/voice-agents".into(),
        state: "csrf-state-77".into(),
        request_id: "11111111-1111-4111-8111-111111111111".into(),
        code_challenge: None,
        code_challenge_method: None,
        action: "authorize".into(),
        destructive_ack: None,
    };
    let result = platform.submit_consent(form).await.expect("consent ok");
    assert_eq!(result.status, 303);
    assert_eq!(result.grant_id.as_deref(), Some("grant-uuid-77"));
    assert_eq!(result.state.as_deref(), Some("csrf-state-77"));
    assert_eq!(result.error, None);
}

#[tokio::test]
async fn submit_consent_cancel_branch_extracts_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/platform/authorize/consent"))
        .respond_with(ResponseTemplate::new(303).insert_header(
            "Location",
            "pizzaos://cancel?error=user_cancelled&state=csrf-78",
        ))
        .expect(1)
        .mount(&server)
        .await;

    let (_, platform) = build_clients(&server);
    let form = ConsentForm {
        app_id: "com.example.demo".into(),
        scopes: "pizza.menu.read".into(),
        return_to: "pizzaos://cancel".into(),
        state: "csrf-78".into(),
        request_id: "22222222-2222-4222-8222-222222222222".into(),
        code_challenge: None,
        code_challenge_method: None,
        action: "cancel".into(),
        destructive_ack: None,
    };
    let result = platform.submit_consent(form).await.expect("ok");
    assert_eq!(result.error.as_deref(), Some("user_cancelled"));
    assert_eq!(result.grant_id, None);
}

// ---------------------------------------------------------------------------
// /platform/authorize/grant (#3808) — form encoded, 303 redirect with code
// ---------------------------------------------------------------------------

#[tokio::test]
async fn submit_grant_extracts_code_and_state() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/platform/authorize/grant"))
        .respond_with(move |req: &Request| {
            let body = std::str::from_utf8(&req.body).unwrap_or("");
            // PKCE is required on /grant — form must include both
            // code_challenge and code_challenge_method.
            assert!(body.contains("code_challenge=challenge-43-bytes"));
            assert!(body.contains("code_challenge_method=S256"));
            ResponseTemplate::new(303).insert_header(
                "Location",
                "pizzaos://oauth-cb?code=auth-code-abc&state=csrf-state-9",
            )
        })
        .expect(1)
        .mount(&server)
        .await;

    let (_, platform) = build_clients(&server);
    let form = GrantForm {
        app_id: "com.example.demo".into(),
        scopes: "pizza.menu.read".into(),
        return_to: "pizzaos://oauth-cb".into(),
        state: "csrf-state-9".into(),
        request_id: "33333333-3333-4333-8333-333333333333".into(),
        code_challenge: "challenge-43-bytes-abcdefghijklmnopqrstuvwxy".into(),
        code_challenge_method: "S256".into(),
        action: "authorize".into(),
        destructive_ack: None,
    };
    let result = platform.submit_grant(form).await.expect("ok");
    assert_eq!(result.status, 303);
    assert_eq!(result.code.as_deref(), Some("auth-code-abc"));
    assert_eq!(result.state.as_deref(), Some("csrf-state-9"));
    assert!(result.location.is_some());
}

// ---------------------------------------------------------------------------
// /platform/authorize/exchange (#3808) — JSON in / JSON out
// ---------------------------------------------------------------------------

#[tokio::test]
async fn exchange_authorization_code_returns_mint_ticket() {
    let server = MockServer::start().await;
    let expected_body = json!({
        "code": "auth-code-abc",
        "code_verifier": "verifier-rand-43",
        "app_id": "com.example.demo",
    });
    Mock::given(method("POST"))
        .and(path("/platform/authorize/exchange"))
        .and(body_json(&expected_body))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "mint_ticket": {
                "app_id": "com.example.demo",
                "tenant_id": "11111111-2222-3333-4444-555555555555",
                "user_id": "66666666-7777-8888-9999-aaaaaaaaaaaa",
                "granted_scopes": ["pizza.menu.read", "pizza.hours.read"],
                "issued_at": 1_730_000_000_i64,
                "exp": 1_730_000_300_i64,
                "nonce": "noncebase64url",
            },
            "expires_in": 300,
        })))
        .expect(1)
        .mount(&server)
        .await;

    let (_, platform) = build_clients(&server);
    let req = ExchangeRequest::new("auth-code-abc", "verifier-rand-43", "com.example.demo");
    let resp = platform.exchange_authorization_code(req).await.expect("ok");
    assert_eq!(resp.expires_in, 300);
    assert_eq!(resp.mint_ticket.app_id, "com.example.demo");
    assert_eq!(resp.mint_ticket.granted_scopes.len(), 2);
    assert_eq!(resp.mint_ticket.exp - resp.mint_ticket.issued_at, 300);
    assert_eq!(resp.mint_ticket.nonce, "noncebase64url");
}

#[tokio::test]
async fn exchange_authorization_code_maps_pkce_mismatch_400() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/platform/authorize/exchange"))
        .respond_with(ResponseTemplate::new(400).set_body_json(error_envelope(
            "pkce_mismatch",
            "code_verifier does not match code_challenge",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let (_, platform) = build_clients(&server);
    let err = platform
        .exchange_authorization_code(ExchangeRequest::new("c", "v", "com.example.demo"))
        .await
        .expect_err("expected pkce_mismatch");
    match err {
        OlympusError::Api { status, code, .. } => {
            assert_eq!(status, 400);
            assert_eq!(code, "pkce_mismatch");
        }
        other => panic!("expected Api, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// /platform/admin/grants/graph (#3806)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_grants_graph_unfiltered_returns_signed_snapshot() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/platform/admin/grants/graph"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "snapshot": {
                "captured_at": "2026-04-28T12:00:00Z",
                "tenants": [{"tenant_id": "tenant-1"}, {"tenant_id": "tenant-2"}],
                "apps": [{"app_id": "com.example.demo"}],
                "granted_edges": [
                    {
                        "tenant_id": "tenant-1",
                        "app_id": "com.example.demo",
                        "source_app_id": "",
                        "active_scope_count": 3,
                        "last_granted_at": "2026-04-28T11:30:00Z",
                        "first_granted_at": "2026-04-20T10:00:00Z",
                        "weight": 3.85
                    }
                ],
                "requires_edges": [
                    {
                        "app_id": "com.example.demo",
                        "scope": "pizza.menu.read",
                        "is_destructive": false,
                        "granted_by_tenants": 2
                    }
                ],
                "total_active_grants": 3
            },
            "signature_hex": "a1b2c3d4",
            "canonical_json": "{...canonical...}"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let (_, platform) = build_clients(&server);
    let resp = platform
        .get_grants_graph(GrantsGraphQuery::all())
        .await
        .expect("ok");
    assert_eq!(resp.snapshot.total_active_grants, 3);
    assert_eq!(resp.snapshot.tenants.len(), 2);
    assert_eq!(resp.snapshot.granted_edges.len(), 1);
    assert_eq!(resp.snapshot.granted_edges[0].active_scope_count, 3);
    assert_eq!(resp.signature_hex, "a1b2c3d4");
    assert!(resp.canonical_json.starts_with('{'));
}

#[tokio::test]
async fn get_grants_graph_forwards_filters_as_query_params() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/platform/admin/grants/graph"))
        .and(query_param("tenant_id", "tenant-7"))
        .and(query_param("app_id", "com.example.demo"))
        .and(query_param("limit", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "snapshot": {
                "captured_at": "2026-04-28T12:00:00Z",
                "tenants": [],
                "apps": [],
                "granted_edges": [],
                "requires_edges": [],
                "total_active_grants": 0
            },
            "signature_hex": "ff",
            "canonical_json": "{}"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let (_, platform) = build_clients(&server);
    let q = GrantsGraphQuery::all()
        .with_tenant_id("tenant-7")
        .with_app_id("com.example.demo")
        .with_limit(100);
    let resp = platform.get_grants_graph(q).await.expect("ok");
    assert_eq!(resp.snapshot.total_active_grants, 0);
}

#[tokio::test]
async fn get_grants_graph_maps_403_insufficient_permissions() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/platform/admin/grants/graph"))
        .respond_with(ResponseTemplate::new(403).set_body_json(error_envelope(
            "INSUFFICIENT_PERMISSIONS",
            "platform_admin role required",
        )))
        .expect(1)
        .mount(&server)
        .await;

    let (_, platform) = build_clients(&server);
    let err = platform
        .get_grants_graph(GrantsGraphQuery::all())
        .await
        .expect_err("expected 403");
    match err {
        OlympusError::Api { status, code, .. } => {
            assert_eq!(status, 403);
            assert_eq!(code, "INSUFFICIENT_PERMISSIONS");
        }
        other => panic!("expected Api, got {other:?}"),
    }
}
