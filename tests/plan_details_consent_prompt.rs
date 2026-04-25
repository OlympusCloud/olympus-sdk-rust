//! Tests for the two platform endpoints landed via olympus-cloud-gcp PRs
//! #3519 + #3520:
//!
//!   - GatingService::get_plan_details → GET /platform/gating/plan-details
//!   - ConsentService::describe        → GET /platform/consent-prompt
//!
//! mockito server, assert path + query string, return canonical Rust
//! handler envelope, verify parsed DTOs.

use mockito::Server;
use olympus_sdk::{OlympusClient, OlympusConfig};
use serde_json::json;

fn make_client(base_url: &str) -> OlympusClient {
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url(base_url);
    OlympusClient::from_config(cfg)
}

#[tokio::test]
async fn get_plan_details_no_tenant_id() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/platform/gating/plan-details")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "current_plan": "growth",
                "plans": [
                    {
                        "tier_id": "free",
                        "display_name": "Free",
                        "monthly_price_usd": 0.0,
                        "features": ["basic"],
                        "usage_limits": {},
                        "ranks_higher_than_current": false,
                        "is_current": false,
                        "diff_vs_current": [],
                        "contact_sales": false,
                    },
                    {
                        "tier_id": "growth",
                        "display_name": "Growth",
                        "monthly_price_usd": 99.0,
                        "features": ["basic", "analytics"],
                        "usage_limits": {"voice_minutes": 60},
                        "ranks_higher_than_current": false,
                        "is_current": true,
                        "diff_vs_current": [],
                        "contact_sales": false,
                    },
                    {
                        "tier_id": "enterprise",
                        "display_name": "Enterprise",
                        "monthly_price_usd": null,
                        "features": ["basic", "analytics", "sla"],
                        "usage_limits": {"voice_minutes": 300},
                        "ranks_higher_than_current": true,
                        "is_current": false,
                        "diff_vs_current": ["unlocks: sla", "+240 voice_minutes"],
                        "contact_sales": true,
                    }
                ],
                "as_of": "2026-04-25T13:00:00Z"
            })
            .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let details = oc.gating().get_plan_details(None).await.unwrap();

    assert_eq!(details.current_plan.as_deref(), Some("growth"));
    assert_eq!(details.plans.len(), 3);

    let free = &details.plans[0];
    assert_eq!(free.monthly_price_usd, Some(0.0));
    assert!(!free.is_current);

    let growth = &details.plans[1];
    assert!(growth.is_current);
    assert_eq!(growth.monthly_price_usd, Some(99.0));

    let ent = &details.plans[2];
    assert!(ent.contact_sales);
    assert_eq!(ent.monthly_price_usd, None);
    assert!(ent.ranks_higher_than_current);
    assert!(ent
        .diff_vs_current
        .iter()
        .any(|s| s == "unlocks: sla"));

    m.assert_async().await;
}

#[tokio::test]
async fn get_plan_details_with_tenant_id() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/platform/gating/plan-details?tenant_id=ten-abc")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({"current_plan": null, "plans": [], "as_of": "2026-04-25T13:00:00Z"})
                .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let _ = oc.gating().get_plan_details(Some("ten-abc")).await.unwrap();

    m.assert_async().await;
}

#[tokio::test]
async fn get_plan_details_null_current_plan() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("GET", "/platform/gating/plan-details")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({"current_plan": null, "plans": [], "as_of": "2026-04-25T13:00:00Z"})
                .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let details = oc.gating().get_plan_details(None).await.unwrap();
    assert!(details.current_plan.is_none());
    assert!(details.plans.is_empty());
}

#[tokio::test]
async fn describe_consent_prompt_full_envelope() {
    let mut server = Server::new_async().await;
    let m = server
        .mock(
            "GET",
            "/platform/consent-prompt?app_id=com.olympuscloud.maximus&scope=auth.session.read%40user",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "app_id": "com.olympuscloud.maximus",
                "scope": "auth.session.read@user",
                "prompt_text": "Maximus will be able to see your active sessions.",
                "prompt_hash": "0".repeat(64),
                "is_destructive": false,
                "requires_mfa": false,
                "app_may_request": true
            })
            .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let prompt = oc
        .consent()
        .describe("com.olympuscloud.maximus", "auth.session.read@user")
        .await
        .unwrap();

    assert_eq!(prompt.app_id, "com.olympuscloud.maximus");
    assert_eq!(prompt.scope, "auth.session.read@user");
    assert!(prompt.prompt_text.starts_with("Maximus"));
    assert_eq!(prompt.prompt_hash.len(), 64);
    assert!(!prompt.is_destructive);
    assert!(prompt.app_may_request);

    m.assert_async().await;
}

#[tokio::test]
async fn describe_consent_prompt_destructive_and_mfa() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock(
            "GET",
            "/platform/consent-prompt?app_id=com.x&scope=auth.session.delete%40user",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "app_id": "com.x",
                "scope": "auth.session.delete@user",
                "prompt_text": "X will sign you out of other devices.",
                "prompt_hash": "a".repeat(64),
                "is_destructive": true,
                "requires_mfa": true,
                "app_may_request": true
            })
            .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let prompt = oc
        .consent()
        .describe("com.x", "auth.session.delete@user")
        .await
        .unwrap();
    assert!(prompt.is_destructive);
    assert!(prompt.requires_mfa);
}

#[tokio::test]
async fn describe_consent_prompt_app_may_request_false() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock(
            "GET",
            "/platform/consent-prompt?app_id=com.untrusted&scope=pizza.menu.read%40tenant",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "app_id": "com.untrusted",
                "scope": "pizza.menu.read@tenant",
                "prompt_text": "untrusted will read pizza menu data.",
                "prompt_hash": "b".repeat(64),
                "is_destructive": false,
                "requires_mfa": false,
                "app_may_request": false
            })
            .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let prompt = oc
        .consent()
        .describe("com.untrusted", "pizza.menu.read@tenant")
        .await
        .unwrap();
    assert!(!prompt.app_may_request);
}
