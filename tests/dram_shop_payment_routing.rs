//! Tests for the dram-shop compliance ledger (#3316) and the payment-
//! routing config CRUD (#3312) — mirrors the Dart 0.8.3 / TS 0.5.2 /
//! Python 0.5.2 / Go 0.5.2 SDK shipments.
//!
//! Endpoints covered:
//!
//!   - POST /platform/compliance/dram-shop-events
//!   - GET  /platform/compliance/dram-shop-events
//!   - GET  /platform/compliance/dram-shop-rules
//!   - POST /platform/pay/routing
//!   - GET  /platform/pay/routing/{location_id}
//!
//! Pattern matches `tests/plan_details_consent_prompt.rs` and
//! `tests/app_scoped_permissions.rs`: spin up a mockito server, assert
//! exact path/body/query, return canonical handler envelope, verify the
//! parsed DTOs.

use mockito::{Matcher, Server};
use olympus_sdk::services::compliance::{
    ListDramShopEventsParams, ListDramShopRulesParams, RecordDramShopEventParams,
};
use olympus_sdk::services::pay::ConfigureRoutingParams;
use olympus_sdk::{OlympusClient, OlympusConfig};
use serde_json::json;

fn make_client(base_url: &str) -> OlympusClient {
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url(base_url);
    OlympusClient::from_config(cfg)
}

// ---------------------------------------------------------------------------
// Dram-shop events — POST canonical body shape
// ---------------------------------------------------------------------------

#[tokio::test]
async fn record_dram_shop_event_canonical_body() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/platform/compliance/dram-shop-events")
        .match_body(Matcher::Json(json!({
            "location_id": "loc-1",
            "event_type": "id_check_passed",
            "customer_ref": "hashed-cust-key",
            "staff_user_id": "usr-staff",
            "estimated_bac": 0.04,
            "bac_inputs": {"gender": "F", "weight_kg": 65},
            "vertical_extensions": {"food_weight_g": 240},
            "notes": "first scan of the night",
            "occurred_at": "2026-04-25T13:00:00Z"
        })))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "event_id": "evt-1",
                "tenant_id": "ten-1",
                "location_id": "loc-1",
                "event_type": "id_check_passed",
                "customer_ref": "hashed-cust-key",
                "staff_user_id": "usr-staff",
                "estimated_bac": 0.04,
                "bac_inputs": {"gender": "F", "weight_kg": 65},
                "vertical_extensions": {"food_weight_g": 240},
                "notes": "first scan of the night",
                "occurred_at": "2026-04-25T13:00:00Z",
                "created_at": "2026-04-25T13:00:01Z"
            })
            .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let params = RecordDramShopEventParams {
        location_id: "loc-1".into(),
        event_type: "id_check_passed".into(),
        customer_ref: Some("hashed-cust-key".into()),
        staff_user_id: Some("usr-staff".into()),
        estimated_bac: Some(0.04),
        bac_inputs: Some(json!({"gender": "F", "weight_kg": 65})),
        vertical_extensions: Some(json!({"food_weight_g": 240})),
        notes: Some("first scan of the night".into()),
        occurred_at: Some("2026-04-25T13:00:00Z".into()),
    };
    let evt = oc
        .compliance()
        .record_dram_shop_event(params)
        .await
        .unwrap();

    assert_eq!(evt.event_id, "evt-1");
    assert_eq!(evt.tenant_id, "ten-1");
    assert_eq!(evt.location_id, "loc-1");
    assert_eq!(evt.event_type, "id_check_passed");
    assert_eq!(evt.customer_ref.as_deref(), Some("hashed-cust-key"));
    assert_eq!(evt.estimated_bac, Some(0.04));
    assert_eq!(evt.notes.as_deref(), Some("first scan of the night"));
    assert_eq!(evt.occurred_at, "2026-04-25T13:00:00Z");
    assert_eq!(evt.created_at, "2026-04-25T13:00:01Z");
    m.assert_async().await;
}

#[tokio::test]
async fn record_dram_shop_event_minimal_body_omits_optional_fields() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/platform/compliance/dram-shop-events")
        .match_body(Matcher::Json(json!({
            "location_id": "loc-2",
            "event_type": "service_refused"
        })))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "event_id": "evt-2",
                "tenant_id": "ten-1",
                "location_id": "loc-2",
                "event_type": "service_refused",
                "occurred_at": "2026-04-25T14:00:00Z",
                "created_at": "2026-04-25T14:00:01Z"
            })
            .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let params = RecordDramShopEventParams {
        location_id: "loc-2".into(),
        event_type: "service_refused".into(),
        ..Default::default()
    };
    let evt = oc
        .compliance()
        .record_dram_shop_event(params)
        .await
        .unwrap();
    assert_eq!(evt.event_id, "evt-2");
    assert!(evt.customer_ref.is_none());
    assert!(evt.estimated_bac.is_none());
    assert!(evt.bac_inputs.is_none());
    m.assert_async().await;
}

// ---------------------------------------------------------------------------
// Dram-shop events — GET with all filters → query string built correctly
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_dram_shop_events_all_filters_in_query_string() {
    let mut server = Server::new_async().await;
    // Order is deterministic: location_id, from, to, event_type, limit.
    let m = server
        .mock(
            "GET",
            "/platform/compliance/dram-shop-events?\
             location_id=loc-1&\
             from=2026-04-25T00%3A00%3A00Z&\
             to=2026-04-25T23%3A59%3A59Z&\
             event_type=id_check_passed&\
             limit=50",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "events": [
                    {
                        "event_id": "evt-1",
                        "tenant_id": "ten-1",
                        "location_id": "loc-1",
                        "event_type": "id_check_passed",
                        "occurred_at": "2026-04-25T13:00:00Z",
                        "created_at": "2026-04-25T13:00:01Z"
                    }
                ],
                "total_returned": 1
            })
            .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let list = oc
        .compliance()
        .list_dram_shop_events(ListDramShopEventsParams {
            location_id: Some("loc-1".into()),
            from: Some("2026-04-25T00:00:00Z".into()),
            to: Some("2026-04-25T23:59:59Z".into()),
            event_type: Some("id_check_passed".into()),
            limit: Some(50),
        })
        .await
        .unwrap();

    assert_eq!(list.total_returned, 1);
    assert_eq!(list.events.len(), 1);
    assert_eq!(list.events[0].event_id, "evt-1");
    m.assert_async().await;
}

#[tokio::test]
async fn list_dram_shop_events_no_filters_hits_bare_path() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/platform/compliance/dram-shop-events")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({"events": [], "total_returned": 0}).to_string())
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let list = oc
        .compliance()
        .list_dram_shop_events(ListDramShopEventsParams::default())
        .await
        .unwrap();
    assert_eq!(list.total_returned, 0);
    assert!(list.events.is_empty());
    m.assert_async().await;
}

// ---------------------------------------------------------------------------
// Dram-shop rules — envelope parsing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_dram_shop_rules_envelope_parsing() {
    let mut server = Server::new_async().await;
    let m = server
        .mock(
            "GET",
            "/platform/compliance/dram-shop-rules?jurisdiction_code=US-CA&app_id=bar-os",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "rules": [
                    {
                        "tenant_id": "ten-1",
                        "rule_id": "rule-1",
                        "jurisdiction_code": "US-CA",
                        "rule_type": "bac_threshold",
                        "rule_payload": {"max_bac": 0.08},
                        "effective_from": "2026-01-01T00:00:00Z",
                        "effective_until": null,
                        "override_app_id": "bar-os",
                        "notes": null,
                        "created_at": "2026-01-01T00:00:00Z"
                    },
                    {
                        "tenant_id": "*",
                        "rule_id": "rule-2",
                        "jurisdiction_code": "US-CA",
                        "rule_type": "service_hours",
                        "rule_payload": null,
                        "effective_from": "2026-01-01T00:00:00Z",
                        "effective_until": "2027-01-01T00:00:00Z",
                        "override_app_id": null,
                        "notes": "platform default",
                        "created_at": "2026-01-01T00:00:00Z"
                    }
                ]
            })
            .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let rules = oc
        .compliance()
        .list_dram_shop_rules(ListDramShopRulesParams {
            jurisdiction_code: Some("US-CA".into()),
            app_id: Some("bar-os".into()),
            rule_type: None,
        })
        .await
        .unwrap();

    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].rule_id, "rule-1");
    assert_eq!(rules[0].override_app_id.as_deref(), Some("bar-os"));
    assert!(rules[0].effective_until.is_none());
    assert!(rules[0].rule_payload.is_some());

    assert_eq!(rules[1].rule_id, "rule-2");
    assert!(rules[1].override_app_id.is_none());
    assert_eq!(rules[1].effective_until.as_deref(), Some("2027-01-01T00:00:00Z"));
    assert!(rules[1].rule_payload.is_none());
    assert_eq!(rules[1].notes.as_deref(), Some("platform default"));
    m.assert_async().await;
}

// ---------------------------------------------------------------------------
// Payment routing — POST canonical body shape
// ---------------------------------------------------------------------------

#[tokio::test]
async fn configure_routing_canonical_body() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/platform/pay/routing")
        .match_body(Matcher::Json(json!({
            "location_id": "loc-1",
            "preferred_processor": "square",
            "fallback_processors": ["olympus_pay"],
            "credentials_secret_ref": "olympus-merchant-credentials-loc-1-square-dev",
            "merchant_id": "MERCH-123",
            "is_active": true
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "tenant_id": "ten-1",
                "location_id": "loc-1",
                "preferred_processor": "square",
                "fallback_processors": ["olympus_pay"],
                "credentials_secret_ref": "olympus-merchant-credentials-loc-1-square-dev",
                "merchant_id": "MERCH-123",
                "is_active": true,
                "notes": null,
                "created_at": "2026-04-25T13:00:00Z",
                "updated_at": "2026-04-25T13:00:00Z"
            })
            .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let cfg = oc
        .pay()
        .configure_routing(ConfigureRoutingParams {
            location_id: "loc-1".into(),
            preferred_processor: "square".into(),
            fallback_processors: vec!["olympus_pay".into()],
            credentials_secret_ref: Some(
                "olympus-merchant-credentials-loc-1-square-dev".into(),
            ),
            merchant_id: Some("MERCH-123".into()),
            is_active: true,
            notes: None,
        })
        .await
        .unwrap();

    assert_eq!(cfg.tenant_id, "ten-1");
    assert_eq!(cfg.location_id, "loc-1");
    assert_eq!(cfg.preferred_processor, "square");
    assert_eq!(cfg.fallback_processors, vec!["olympus_pay".to_string()]);
    assert!(cfg.is_active);
    assert!(cfg.notes.is_none());
    assert_eq!(cfg.created_at.as_deref(), Some("2026-04-25T13:00:00Z"));
    assert_eq!(cfg.updated_at.as_deref(), Some("2026-04-25T13:00:00Z"));
    m.assert_async().await;
}

#[tokio::test]
async fn configure_routing_omits_credentials_secret_ref_when_none() {
    let mut server = Server::new_async().await;
    // Body must NOT contain credentials_secret_ref / merchant_id / notes.
    let m = server
        .mock("POST", "/platform/pay/routing")
        .match_body(Matcher::Json(json!({
            "location_id": "loc-2",
            "preferred_processor": "olympus_pay",
            "fallback_processors": [],
            "is_active": false
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "tenant_id": "ten-1",
                "location_id": "loc-2",
                "preferred_processor": "olympus_pay",
                "fallback_processors": [],
                "is_active": false
            })
            .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let cfg = oc
        .pay()
        .configure_routing(ConfigureRoutingParams {
            location_id: "loc-2".into(),
            preferred_processor: "olympus_pay".into(),
            fallback_processors: vec![],
            credentials_secret_ref: None,
            merchant_id: None,
            is_active: false,
            notes: None,
        })
        .await
        .unwrap();
    assert!(cfg.credentials_secret_ref.is_none());
    assert!(cfg.merchant_id.is_none());
    assert!(cfg.notes.is_none());
    assert!(!cfg.is_active);
    m.assert_async().await;
}

// ---------------------------------------------------------------------------
// Payment routing — GET URL-encodes the location_id path segment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_routing_url_encodes_location_id() {
    let mut server = Server::new_async().await;
    // location_id with spaces + slashes must be percent-encoded.
    let m = server
        .mock(
            "GET",
            "/platform/pay/routing/loc%2Fwith%20space",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "tenant_id": "ten-1",
                "location_id": "loc/with space",
                "preferred_processor": "adyen",
                "fallback_processors": ["worldpay"],
                "is_active": true
            })
            .to_string(),
        )
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let cfg = oc.pay().get_routing("loc/with space").await.unwrap();
    assert_eq!(cfg.location_id, "loc/with space");
    assert_eq!(cfg.preferred_processor, "adyen");
    assert_eq!(cfg.fallback_processors, vec!["worldpay".to_string()]);
    m.assert_async().await;
}
