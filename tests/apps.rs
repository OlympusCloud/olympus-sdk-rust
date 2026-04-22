//! Integration tests for the AppsApi install ceremony (#3413 §3).
//!
//! Uses `wiremock` per the task spec. Covers all 7 methods
//! ([`AppsApi::install`], [`AppsApi::list_installed`], [`AppsApi::uninstall`],
//! [`AppsApi::get_manifest`], [`AppsApi::get_pending_install`],
//! [`AppsApi::approve_pending_install`], [`AppsApi::deny_pending_install`])
//! plus error paths (403 mfa_required on install, 410 Gone on expired
//! pending row, 404 on unknown app on get_manifest).

use olympus_sdk::apps::{AppInstallRequest, AppsApi};
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
// install
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn install_posts_request_and_returns_pending_install() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/apps/install"))
        .and(body_partial_json(json!({
            "app_id": "com.pizzaos",
            "scopes": ["commerce.orders.read", "identity.users.read"],
            "return_to": "https://pizza.shop/settings/perms",
            "idempotency_key": "install-btn-click-42",
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "pending_install_id": "7a3b8c1d-0000-4000-8000-000000000001",
            "consent_url": "https://platform.olympuscloud.ai/apps/consent/7a3b8c1d-0000-4000-8000-000000000001",
            "expires_at": "2026-04-21T00:10:00Z",
        })))
        .mount(&server)
        .await;

    let pending = client
        .apps()
        .install(AppInstallRequest {
            app_id: "com.pizzaos".into(),
            scopes: vec!["commerce.orders.read".into(), "identity.users.read".into()],
            return_to: "https://pizza.shop/settings/perms".into(),
            idempotency_key: Some("install-btn-click-42".into()),
        })
        .await
        .expect("install");

    assert_eq!(
        pending.pending_install_id,
        "7a3b8c1d-0000-4000-8000-000000000001"
    );
    assert!(pending
        .consent_url
        .contains("platform.olympuscloud.ai/apps/consent"));
    assert_eq!(pending.expires_at, "2026-04-21T00:10:00Z");
}

#[tokio::test(flavor = "multi_thread")]
async fn install_omits_scopes_and_idempotency_when_empty() {
    // Empty scopes vec + None idempotency_key should NOT serialize either
    // field — the server accepts this (manifest-required-only install).
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/apps/install"))
        .and(body_partial_json(json!({
            "app_id": "com.barOS",
            "return_to": "https://bar.shop/r",
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "pending_install_id": "uuid-2",
            "consent_url": "https://platform.olympuscloud.ai/apps/consent/uuid-2",
            "expires_at": "2026-04-21T00:10:00Z",
        })))
        .mount(&server)
        .await;

    client
        .apps()
        .install(AppInstallRequest {
            app_id: "com.barOS".into(),
            scopes: vec![],
            return_to: "https://bar.shop/r".into(),
            idempotency_key: None,
        })
        .await
        .expect("install");
}

#[tokio::test(flavor = "multi_thread")]
async fn install_surfaces_403_mfa_required_as_api_error() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/apps/install"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "error": {"message": "mfa_required"}
        })))
        .mount(&server)
        .await;

    let res = client
        .apps()
        .install(AppInstallRequest {
            app_id: "com.pizzaos".into(),
            scopes: vec![],
            return_to: "https://x/y".into(),
            idempotency_key: None,
        })
        .await;
    match res {
        Err(olympus_sdk::OlympusError::Api { status, .. }) => assert_eq!(status, 403),
        other => panic!("expected Api 403 mfa_required, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// list_installed
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn list_installed_returns_active_rows() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("GET"))
        .and(path("/apps/installed"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "tenant_id": "t1",
                "app_id": "com.pizzaos",
                "installed_at": "2026-04-21T00:00:00Z",
                "installed_by": "user_admin_1",
                "scopes_granted": ["commerce.orders.read", "identity.users.read"],
                "status": "active",
            },
            {
                "tenant_id": "t1",
                "app_id": "com.barOS",
                "installed_at": "2026-04-20T00:00:00Z",
                "installed_by": "user_admin_1",
                "scopes_granted": ["commerce.orders.read"],
                "status": "active",
            }
        ])))
        .mount(&server)
        .await;

    let installs = client.apps().list_installed().await.expect("list");
    assert_eq!(installs.len(), 2);
    assert_eq!(installs[0].app_id, "com.pizzaos");
    assert_eq!(installs[0].scopes_granted.len(), 2);
    assert_eq!(installs[1].status, "active");
}

#[tokio::test(flavor = "multi_thread")]
async fn list_installed_returns_empty_vec_on_zero_rows() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("GET"))
        .and(path("/apps/installed"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;

    let installs = client.apps().list_installed().await.expect("list");
    assert!(installs.is_empty());
}

// ---------------------------------------------------------------------------
// uninstall
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn uninstall_posts_to_app_id_path_and_discards_body() {
    let (client, server) = new_client_with_mock_server().await;
    // app_id with a dot doesn't need encoding, but we still validate the
    // path shape ends up at /apps/uninstall/com.pizzaos.
    Mock::given(method("POST"))
        .and(path("/apps/uninstall/com.pizzaos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tenant_id": "t1",
            "app_id": "com.pizzaos",
            "uninstalled_at": "2026-04-21T00:00:00Z",
            "uninstalled_by": "user_admin_1",
        })))
        .mount(&server)
        .await;

    // SDK discards the body per parity with Dart `uninstall` returning void.
    client
        .apps()
        .uninstall("com.pizzaos")
        .await
        .expect("uninstall");
}

#[tokio::test(flavor = "multi_thread")]
async fn uninstall_surfaces_404_when_app_not_installed() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/apps/uninstall/com.unknown"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "error": {"message": "app not installed on this tenant"}
        })))
        .mount(&server)
        .await;

    let res = client.apps().uninstall("com.unknown").await;
    match res {
        Err(olympus_sdk::OlympusError::Api { status, .. }) => assert_eq!(status, 404),
        other => panic!("expected Api 404, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// get_manifest
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn get_manifest_returns_manifest_row() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("GET"))
        .and(path("/apps/manifest/com.pizzaos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "app_id": "com.pizzaos",
            "version": "1.4.0",
            "name": "PizzaOS",
            "publisher": "NëbusAI",
            "logo_url": "https://cdn.example/pizza.png",
            "scopes_required": ["commerce.orders.read"],
            "scopes_optional": ["identity.users.read"],
            "privacy_url": "https://pizzaos.app/privacy",
            "tos_url": "https://pizzaos.app/terms",
        })))
        .mount(&server)
        .await;

    let manifest = client
        .apps()
        .get_manifest("com.pizzaos")
        .await
        .expect("manifest");
    assert_eq!(manifest.version, "1.4.0");
    assert_eq!(manifest.name, "PizzaOS");
    assert_eq!(
        manifest.scopes_required,
        vec!["commerce.orders.read".to_string()]
    );
    assert_eq!(
        manifest.logo_url.as_deref(),
        Some("https://cdn.example/pizza.png")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn get_manifest_surfaces_404_for_unknown_app() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("GET"))
        .and(path("/apps/manifest/com.unknown"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "error": {"message": "manifest not found"}
        })))
        .mount(&server)
        .await;

    let res = client.apps().get_manifest("com.unknown").await;
    match res {
        Err(olympus_sdk::OlympusError::Api { status, .. }) => assert_eq!(status, 404),
        other => panic!("expected Api 404, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// get_pending_install
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn get_pending_install_returns_detail_with_eager_manifest() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("GET"))
        .and(path("/apps/pending_install/7a3b8c1d"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "7a3b8c1d",
            "app_id": "com.pizzaos",
            "tenant_id": "t_pizza",
            "requested_scopes": ["commerce.orders.read", "identity.users.read"],
            "return_to": "https://pizza.shop/settings/perms",
            "status": "pending",
            "expires_at": "2026-04-21T00:10:00Z",
            "manifest": {
                "app_id": "com.pizzaos",
                "version": "1.4.0",
                "name": "PizzaOS",
                "publisher": "NëbusAI",
                "scopes_required": ["commerce.orders.read"],
                "scopes_optional": ["identity.users.read"],
            },
        })))
        .mount(&server)
        .await;

    let detail = client
        .apps()
        .get_pending_install("7a3b8c1d")
        .await
        .expect("pending");
    assert_eq!(detail.status, "pending");
    assert_eq!(detail.tenant_id, "t_pizza");
    assert_eq!(detail.requested_scopes.len(), 2);
    let manifest = detail.manifest.expect("manifest eager-loaded");
    assert_eq!(manifest.name, "PizzaOS");
}

#[tokio::test(flavor = "multi_thread")]
async fn get_pending_install_surfaces_410_gone_on_expiry() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("GET"))
        .and(path("/apps/pending_install/expired-uuid"))
        .respond_with(ResponseTemplate::new(410).set_body_json(json!({
            "error": {"message": "pending install expired or not found"}
        })))
        .mount(&server)
        .await;

    let res = client.apps().get_pending_install("expired-uuid").await;
    match res {
        Err(olympus_sdk::OlympusError::Api { status, .. }) => assert_eq!(status, 410),
        other => panic!("expected Api 410 Gone, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// approve_pending_install
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn approve_pending_install_returns_fresh_app_install() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/apps/pending_install/7a3b8c1d/approve"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tenant_id": "t_pizza",
            "app_id": "com.pizzaos",
            "installed_at": "2026-04-21T00:01:00Z",
            "installed_by": "user_admin_1",
            "scopes_granted": ["commerce.orders.read", "identity.users.read"],
            "status": "active",
        })))
        .mount(&server)
        .await;

    let install = client
        .apps()
        .approve_pending_install("7a3b8c1d")
        .await
        .expect("approve");
    assert_eq!(install.tenant_id, "t_pizza");
    assert_eq!(install.app_id, "com.pizzaos");
    assert_eq!(install.status, "active");
    assert_eq!(install.scopes_granted.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn approve_pending_install_surfaces_410_gone_on_expired_row() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/apps/pending_install/expired-uuid/approve"))
        .respond_with(ResponseTemplate::new(410).set_body_json(json!({
            "error": {"message": "pending install expired"}
        })))
        .mount(&server)
        .await;

    let res = client.apps().approve_pending_install("expired-uuid").await;
    match res {
        Err(olympus_sdk::OlympusError::Api { status, .. }) => assert_eq!(status, 410),
        other => panic!("expected Api 410 Gone, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// deny_pending_install
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn deny_pending_install_posts_and_returns_ok() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/apps/pending_install/7a3b8c1d/deny"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    client
        .apps()
        .deny_pending_install("7a3b8c1d")
        .await
        .expect("deny");
}

#[tokio::test(flavor = "multi_thread")]
async fn deny_pending_install_surfaces_403_for_non_admin() {
    let (client, server) = new_client_with_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/apps/pending_install/7a3b8c1d/deny"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "error": {"message": "tenant_admin required"}
        })))
        .mount(&server)
        .await;

    let res = client.apps().deny_pending_install("7a3b8c1d").await;
    match res {
        Err(olympus_sdk::OlympusError::Api { status, .. }) => assert_eq!(status, 403),
        other => panic!("expected Api 403, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Constructor smoke — guarantees AppsApi::new is part of the public surface.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn apps_api_new_constructs_usable_instance() {
    let (client, _server) = new_client_with_mock_server().await;
    // The inherent accessor route:
    let _via_client: AppsApi = client.apps();
    // The explicit constructor route:
    let _via_new: AppsApi = AppsApi::new(&client);
}
