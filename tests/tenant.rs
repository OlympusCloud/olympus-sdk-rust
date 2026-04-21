//! Integration tests for the TenantApi (#3403 §4.2 + §4.4).
//!
//! Uses `wiremock` per the task spec. The suite exercises each public method
//! on the borrow-pattern `TenantApi`, plus the internal `switch_tenant`
//! chain that walks `POST /tenant/switch` -> `POST /auth/switch-tenant` and
//! rotates the HTTP client's access / refresh tokens on success.

use olympus_sdk::tenant::{TenantCreateRequest, TenantFirstAdmin, TenantUpdate};
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
// create
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn create_posts_snake_case_body_and_returns_provision_result() {
    let (client, server) = new_client_with_mock_server().await;
    let response = json!({
        "tenant": {
            "id": "t-abc",
            "slug": "pizza-shop",
            "name": "Pizza Shop",
            "industry": "restaurant",
            "subscription_tier": "ember",
            "settings": {},
            "features": {},
            "branding": {},
            "metadata": {},
            "tags": [],
            "is_active": true,
            "is_suspended": false,
            "is_nebusai_company": false,
            "created_at": "2026-04-21T00:00:00Z",
            "updated_at": "2026-04-21T00:00:00Z",
        },
        "admin_user_id": "u-111",
        "session": {},
        "installed_apps": [
            {"app_id": "pizza-os", "status": "installed", "installed_at": "2026-04-21T00:00:00Z"}
        ],
        "idempotent": false,
    });

    Mock::given(method("POST"))
        .and(path("/tenant/create"))
        .and(body_partial_json(json!({
            "brand_name": "Pizza Shop",
            "slug": "pizza-shop",
            "plan": "demo",
            "idempotency_key": "firebase-uid-abc",
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(response.clone()))
        .mount(&server)
        .await;

    let req = TenantCreateRequest {
        brand_name: "Pizza Shop".into(),
        slug: "pizza-shop".into(),
        plan: "demo".into(),
        first_admin: TenantFirstAdmin {
            firebase_link: Some("fb-abc".into()),
            email: "owner@pizza.shop".into(),
            first_name: "Scott".into(),
            last_name: "Owner".into(),
        },
        install_apps: vec!["pizza-os".into()],
        billing_address: None,
        tax_id: None,
        idempotency_key: "firebase-uid-abc".into(),
    };

    let result = client.tenant().create(req).await.expect("create");
    assert_eq!(result.tenant.slug, "pizza-shop");
    assert_eq!(result.admin_user_id, "u-111");
    assert_eq!(result.installed_apps.len(), 1);
    assert!(!result.idempotent);
}

#[tokio::test(flavor = "multi_thread")]
async fn create_surfaces_idempotent_retry_payload() {
    let (client, server) = new_client_with_mock_server().await;
    let response = json!({
        "tenant": {
            "id": "t-orig",
            "slug": "original",
            "name": "Original Co",
            "industry": "other",
            "subscription_tier": "ember",
            "settings": {},
            "features": {},
            "branding": {},
            "metadata": {},
            "tags": [],
            "is_active": true,
            "is_suspended": false,
            "is_nebusai_company": false,
            "created_at": "2026-04-21T00:00:00Z",
            "updated_at": "2026-04-21T00:00:00Z",
        },
        "admin_user_id": "",
        "session": {},
        "installed_apps": [],
        "idempotent": true,
    });

    Mock::given(method("POST"))
        .and(path("/tenant/create"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .mount(&server)
        .await;

    let result = client
        .tenant()
        .create(TenantCreateRequest {
            brand_name: "Original Co".into(),
            slug: "original".into(),
            plan: "starter".into(),
            first_admin: TenantFirstAdmin {
                firebase_link: None,
                email: "a@b.co".into(),
                first_name: "A".into(),
                last_name: "B".into(),
            },
            install_apps: vec![],
            billing_address: None,
            tax_id: None,
            idempotency_key: "idem-retry".into(),
        })
        .await
        .expect("create");
    assert!(result.idempotent);
    assert_eq!(result.tenant.id, "t-orig");
}

// ---------------------------------------------------------------------------
// current + update
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn current_deserializes_tenant_payload() {
    let (client, server) = new_client_with_mock_server().await;
    let tenant_body = json!({
        "id": "t-1",
        "slug": "acme",
        "name": "ACME Co",
        "industry": "retail",
        "subscription_tier": "spark",
        "settings": {"plan": "starter"},
        "features": {},
        "branding": {},
        "metadata": {},
        "tags": ["beta"],
        "is_active": true,
        "is_suspended": false,
        "is_nebusai_company": false,
        "created_at": "2026-04-20T00:00:00Z",
        "updated_at": "2026-04-21T00:00:00Z",
        "retired_at": null,
    });

    Mock::given(method("GET"))
        .and(path("/tenant/current"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tenant_body))
        .mount(&server)
        .await;

    let tenant = client.tenant().current().await.expect("current");
    assert_eq!(tenant.slug, "acme");
    assert_eq!(tenant.tags, vec!["beta".to_string()]);
    assert!(tenant.retired_at.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn update_patches_tenant_with_partial_body() {
    let (client, server) = new_client_with_mock_server().await;
    let response = json!({
        "id": "t-1",
        "slug": "acme",
        "name": "ACME Co",
        "industry": "retail",
        "subscription_tier": "blaze",
        "settings": {"plan": "pro"},
        "features": {},
        "branding": {},
        "metadata": {},
        "tags": [],
        "is_active": true,
        "is_suspended": false,
        "is_nebusai_company": false,
        "locale": "en-US",
        "timezone": "America/New_York",
        "created_at": "2026-04-20T00:00:00Z",
        "updated_at": "2026-04-21T00:00:00Z",
    });

    Mock::given(method("PATCH"))
        .and(path("/tenant/current"))
        .and(body_partial_json(json!({
            "plan": "pro",
            "timezone": "America/New_York",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .mount(&server)
        .await;

    let patch = TenantUpdate {
        brand_name: None,
        plan: Some("pro".into()),
        billing_address: None,
        tax_id: None,
        locale: None,
        timezone: Some("America/New_York".into()),
    };
    let tenant = client.tenant().update(patch).await.expect("update");
    assert_eq!(tenant.timezone.as_deref(), Some("America/New_York"));
}

// ---------------------------------------------------------------------------
// retire + unretire
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn retire_posts_confirmation_slug_and_null_reason() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/tenant/retire"))
        .and(body_partial_json(json!({
            "confirmation_slug": "acme",
            "reason": null,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tenant_id": "t-1",
            "retired_at": "2026-04-21T00:00:00Z",
            "purge_eligible_at": "2026-05-21T00:00:00Z",
        })))
        .mount(&server)
        .await;
    client.tenant().retire("acme").await.expect("retire");
}

#[tokio::test(flavor = "multi_thread")]
async fn retire_with_reason_includes_reason_in_body() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/tenant/retire"))
        .and(body_partial_json(json!({
            "confirmation_slug": "acme",
            "reason": "closing for good",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tenant_id": "t-1",
            "retired_at": "2026-04-21T00:00:00Z",
            "purge_eligible_at": "2026-05-21T00:00:00Z",
        })))
        .mount(&server)
        .await;
    client
        .tenant()
        .retire_with_reason("acme", Some("closing for good"))
        .await
        .expect("retire_with_reason");
}

#[tokio::test(flavor = "multi_thread")]
async fn unretire_posts_empty_body() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/tenant/unretire"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tenant_id": "t-1",
            "unretired_at": "2026-04-21T00:00:00Z",
        })))
        .mount(&server)
        .await;
    client.tenant().unretire().await.expect("unretire");
}

// ---------------------------------------------------------------------------
// my_tenants
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn my_tenants_returns_options_with_optional_role() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("GET"))
        .and(path("/tenant/mine"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"tenant_id": "t-1", "slug": "acme", "name": "ACME Co"},
            {"tenant_id": "t-2", "slug": "beta", "name": "Beta Inc", "role": "manager"},
        ])))
        .mount(&server)
        .await;
    let options = client.tenant().my_tenants().await.expect("mine");
    assert_eq!(options.len(), 2);
    assert!(options[0].role.is_none());
    assert_eq!(options[1].role.as_deref(), Some("manager"));
}

// ---------------------------------------------------------------------------
// switch_tenant — chains /tenant/switch + /auth/switch-tenant and rotates
// the HTTP client's access/refresh tokens.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn switch_tenant_chains_platform_then_auth_and_rotates_tokens() {
    let (client, server) = new_client_with_mock_server().await;

    // Seed a current access + refresh pair so rotation is observable.
    client.set_access_token("old-access");
    client.set_refresh_token("old-refresh");

    Mock::given(method("POST"))
        .and(path("/tenant/switch"))
        .and(body_partial_json(json!({
            "tenant_id": "target-tenant-uuid",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "target_tenant_id": "target-tenant-uuid",
            "auth_endpoint": "/auth/switch-tenant",
            "instructions": "POST...",
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/auth/switch-tenant"))
        .and(body_partial_json(json!({
            "tenant_id": "target-tenant-uuid",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "new-access",
            "refresh_token": "new-refresh",
            "token_type": "Bearer",
            "expires_in": 3600,
            "user": {"id": "u-1"},
        })))
        .mount(&server)
        .await;

    let session = client
        .tenant()
        .switch_tenant("target-tenant-uuid")
        .await
        .expect("switch");
    assert_eq!(session.access_token.as_deref(), Some("new-access"));
    assert_eq!(session.refresh_token.as_deref(), Some("new-refresh"));

    // Verify tokens were rotated by issuing another call — the mock below
    // matches only when Authorization is `Bearer new-access`.
    Mock::given(method("GET"))
        .and(path("/tenant/current"))
        .and(wiremock::matchers::header(
            "authorization",
            "Bearer new-access",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "t-new",
            "slug": "new-tenant",
            "name": "New Tenant",
            "industry": "other",
            "subscription_tier": "ember",
            "settings": {},
            "features": {},
            "branding": {},
            "metadata": {},
            "tags": [],
            "is_active": true,
            "is_suspended": false,
            "is_nebusai_company": false,
            "created_at": "2026-04-21T00:00:00Z",
            "updated_at": "2026-04-21T00:00:00Z",
        })))
        .mount(&server)
        .await;
    let tenant = client
        .tenant()
        .current()
        .await
        .expect("current after switch");
    assert_eq!(tenant.id, "t-new");
}

#[tokio::test(flavor = "multi_thread")]
async fn switch_tenant_surfaces_platform_error_without_touching_auth() {
    let (client, server) = new_client_with_mock_server().await;
    client.set_access_token("keep-me");

    Mock::given(method("POST"))
        .and(path("/tenant/switch"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "error": {"message": "caller has no account in the target tenant"}
        })))
        .mount(&server)
        .await;

    // No /auth/switch-tenant mock — if the SDK wrongly chained, the test
    // would fail with a 404 from wiremock instead of the expected 403.
    let err = client
        .tenant()
        .switch_tenant("wrong-tenant")
        .await
        .expect_err("should error");
    match err {
        olympus_sdk::OlympusError::Api { status, .. } => assert_eq!(status, 403),
        other => panic!("expected Api 403, got {:?}", other),
    }
}
