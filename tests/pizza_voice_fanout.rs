//! Integration tests for the pizza-voice campaign fanout — RC1 #3719.
//!
//! Covers:
//!   - 86 (out-of-stock) kill-switch on items + ingredients (#3690 / #3695)
//!   - Combos lifecycle CRUD (#3707)
//!   - Voice campaigns list (#3651)
//!
//! Mirror of the test suites in olympus-sdk-typescript#19 and
//! olympus-sdk-python#19. Each test asserts the wire path + body shape so
//! callers like CallStackAI, OrderEchoAI, PizzaOS, BarOS get a stable
//! contract regardless of which SDK language they consume.

use std::sync::Arc;

use olympus_sdk::http::OlympusHttpClient;
use olympus_sdk::services::commerce::CommerceService;
use olympus_sdk::services::voice_orders::{ListVoiceCampaignsOptions, VoiceOrdersService};
use olympus_sdk::OlympusConfig;
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn build_commerce(server: &MockServer) -> CommerceService {
    let config = OlympusConfig::new("com.test", "oc_test_key").with_base_url(server.uri());
    let http = Arc::new(OlympusHttpClient::new(Arc::new(config)).expect("http client"));
    CommerceService::new(http)
}

fn build_voice_orders(server: &MockServer) -> VoiceOrdersService {
    let config = OlympusConfig::new("com.test", "oc_test_key").with_base_url(server.uri());
    let http = Arc::new(OlympusHttpClient::new(Arc::new(config)).expect("http client"));
    VoiceOrdersService::new(http)
}

// ===========================================================================
// 86 kill-switch (#3690 / #3695)
// ===========================================================================

#[tokio::test]
async fn eighty_six_item_full_body_round_trip() {
    let server = MockServer::start().await;
    let body = json!({
        "reason": "sold out",
        "until": "2026-04-28T18:00:00Z",
        "remaining_quantity": 0,
    });
    Mock::given(method("POST"))
        .and(path("/commerce/menus/items/item-uuid/86"))
        .and(body_json(&body))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"is_86ed": true})))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_commerce(&server);
    svc.eighty_six_item("item-uuid", &body)
        .await
        .expect("eighty_six_item");
}

#[tokio::test]
async fn eighty_six_item_empty_body_works() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/commerce/menus/items/item-uuid/86"))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_commerce(&server);
    svc.eighty_six_item("item-uuid", &json!({}))
        .await
        .expect("eighty_six_item with empty body");
}

#[tokio::test]
async fn un_eighty_six_item_deletes_canonical_path() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/commerce/menus/items/item-uuid/86"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_commerce(&server);
    svc.un_eighty_six_item("item-uuid")
        .await
        .expect("un_eighty_six_item");
}

#[tokio::test]
async fn eighty_six_ingredient_cascade_route() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/commerce/menus/ingredients/ing-uuid/86"))
        .and(body_json(json!({"reason": "mushroom delivery delayed"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"is_86ed": true})))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_commerce(&server);
    svc.eighty_six_ingredient(
        "ing-uuid",
        &json!({"reason": "mushroom delivery delayed"}),
    )
    .await
    .expect("eighty_six_ingredient");
}

#[tokio::test]
async fn un_eighty_six_ingredient_deletes_canonical_path() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/commerce/menus/ingredients/ing-uuid/86"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_commerce(&server);
    svc.un_eighty_six_ingredient("ing-uuid")
        .await
        .expect("un_eighty_six_ingredient");
}

#[tokio::test]
async fn list_eighty_sixed_items_hits_canonical_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/commerce/menus/items/86"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": []})))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_commerce(&server);
    svc.list_eighty_sixed_items()
        .await
        .expect("list_eighty_sixed_items");
}

#[tokio::test]
async fn list_eighty_sixed_ingredients_hits_canonical_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/commerce/menus/ingredients/86"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ingredients": []})))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_commerce(&server);
    svc.list_eighty_sixed_ingredients()
        .await
        .expect("list_eighty_sixed_ingredients");
}

#[tokio::test]
async fn get_eighty_six_log_passes_filters() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/commerce/menus/86/log"))
        .and(query_param("entity_id", "ent-uuid"))
        .and(query_param("limit", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"entries": []})))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_commerce(&server);
    svc.get_eighty_six_log(Some("ent-uuid"), Some(50))
        .await
        .expect("get_eighty_six_log");
}

// ===========================================================================
// Combos lifecycle (#3707)
// ===========================================================================

#[tokio::test]
async fn create_combo_full_payload_round_trip() {
    let server = MockServer::start().await;
    let body = json!({
        "location_id": "loc-uuid",
        "name": "Family Combo",
        "combo_price": "24.99",
        "component_items": [
            {"menu_item_id": "item-1", "quantity": 1},
            {"menu_item_id": "item-2", "quantity": 2},
        ],
        "description": "2 large + 2 sodas",
        "valid_from": "2026-04-28T00:00:00Z",
        "active": true,
    });
    Mock::given(method("POST"))
        .and(path("/commerce/combos"))
        .and(body_json(&body))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"combo_id": "c1"})))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_commerce(&server);
    svc.create_combo(&body).await.expect("create_combo");
}

#[tokio::test]
async fn list_combos_voice_bootstrap_shape() {
    // The voice combo matcher (#3701) calls this exact shape at session
    // bootstrap. If the wire contract drifts, voice combo recognition
    // silently regresses.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/commerce/combos"))
        .and(query_param("location_id", "loc-uuid"))
        .and(query_param("active", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"combos": []})))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_commerce(&server);
    svc.list_combos(Some("loc-uuid"), Some(true))
        .await
        .expect("list_combos voice bootstrap");
}

#[tokio::test]
async fn update_combo_partial_patch() {
    let server = MockServer::start().await;
    let body = json!({
        "combo_price": "19.99",
        "component_items": [{"menu_item_id": "item-1", "quantity": 1}],
    });
    Mock::given(method("PATCH"))
        .and(path("/commerce/combos/combo-uuid"))
        .and(body_json(&body))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_commerce(&server);
    svc.update_combo("combo-uuid", &body)
        .await
        .expect("update_combo");
}

#[tokio::test]
async fn delete_combo_soft_delete() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/commerce/combos/combo-uuid"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_commerce(&server);
    svc.delete_combo("combo-uuid")
        .await
        .expect("delete_combo");
}

// ===========================================================================
// Voice campaigns list (#3651)
// ===========================================================================

#[tokio::test]
async fn list_voice_campaigns_with_full_filters() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/voice-agents/campaigns"))
        .and(query_param("status", "draft"))
        .and(query_param("channel", "sms"))
        .and(query_param("created_after", "2026-04-27T00:00:00Z"))
        .and(query_param("limit", "25"))
        .and(query_param("offset", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "campaigns": [{"id": "c1"}],
            "total": 1,
            "next_offset": null,
        })))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_voice_orders(&server);
    let opts = ListVoiceCampaignsOptions {
        status: Some("draft"),
        channel: Some("sms"),
        created_after: Some("2026-04-27T00:00:00Z"),
        limit: Some(25),
        offset: Some(50),
    };
    let resp = svc
        .list_voice_campaigns(opts)
        .await
        .expect("list_voice_campaigns");
    assert_eq!(resp["total"], json!(1));
}

#[tokio::test]
async fn list_voice_campaigns_no_filters_default_path() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/voice-agents/campaigns"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "campaigns": [],
            "total": 0,
            "next_offset": null,
        })))
        .expect(1)
        .mount(&server)
        .await;

    let svc = build_voice_orders(&server);
    svc.list_voice_campaigns(ListVoiceCampaignsOptions::default())
        .await
        .expect("list_voice_campaigns default");
}
