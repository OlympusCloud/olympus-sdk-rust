//! Unit/integration tests for MapsService (#3227) and AuthSession.company_id (#3151).
//!
//! Uses mockito to spin up a local HTTP server that the SDK client calls.
//! All tests are offline — no live API calls.

use mockito::Server;
use olympus_sdk::services::maps::{
    DirectionsRequest, GeocodeRequest, ValidateDeliveryZoneRequest,
};
use olympus_sdk::session::AuthSession;
use olympus_sdk::{OlympusClient, OlympusConfig};

fn make_client(base_url: &str) -> OlympusClient {
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url(base_url);
    OlympusClient::from_config(cfg)
}

// ─────────────────────────────────────── #3151 AuthSession.company_id ──

#[test]
fn auth_session_from_json_includes_company_id() {
    let json = serde_json::json!({
        "access_token": "tok",
        "refresh_token": "ref",
        "expires_at": 9999999999_u64,
        "token_type": "Bearer",
        "user_id": "u-1",
        "tenant_id": "t-1",
        "company_id": "c-abc",
    });
    let session = AuthSession::from_json(&json);
    assert_eq!(session.company_id.as_deref(), Some("c-abc"));
    assert_eq!(session.tenant_id.as_deref(), Some("t-1"));
}

#[test]
fn auth_session_from_json_company_id_none_when_absent() {
    let json = serde_json::json!({
        "access_token": "tok",
        "token_type": "Bearer",
    });
    let session = AuthSession::from_json(&json);
    assert!(session.company_id.is_none());
}

#[test]
fn auth_session_deserialize_company_id() {
    let json_str = r#"{
        "access_token": "t",
        "refresh_token": "r",
        "expires_at": 1000,
        "token_type": "Bearer",
        "company_id": "c-xyz"
    }"#;
    let session: AuthSession = serde_json::from_str(json_str).unwrap();
    assert_eq!(session.company_id.as_deref(), Some("c-xyz"));
}

// ─────────────────────────────────────── #3227 MapsService::geocode ──

#[tokio::test]
async fn maps_geocode_posts_and_returns_response() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/maps/geocode")
        .match_body(mockito::Matcher::Json(serde_json::json!({
            "address": "123 Main St"
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{
            "formatted": "123 Main St, Austin, TX 78701",
            "lat": 30.2672,
            "lng": -97.7431,
            "place_id": "ChIJabc"
        }"#)
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let resp = oc
        .maps()
        .geocode(GeocodeRequest {
            address: "123 Main St".to_string(),
        })
        .await
        .expect("geocode should succeed");

    assert_eq!(resp.formatted, "123 Main St, Austin, TX 78701");
    assert!((resp.lat - 30.2672).abs() < 0.001);
    assert!((resp.lng - (-97.7431)).abs() < 0.001);
    assert_eq!(resp.place_id.as_deref(), Some("ChIJabc"));
    m.assert_async().await;
}

#[tokio::test]
async fn maps_geocode_place_id_none_when_absent() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/maps/geocode")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"formatted": "Austin, TX", "lat": 30.2, "lng": -97.7}"#)
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let resp = oc
        .maps()
        .geocode(GeocodeRequest { address: "Austin".to_string() })
        .await
        .unwrap();

    assert!(resp.place_id.is_none());
}

// ─────────────────────────────────────── #3227 MapsService::directions ──

#[tokio::test]
async fn maps_directions_posts_and_returns_steps() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/maps/directions")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{
            "distance_text": "5.2 mi",
            "distance_meters": 8369,
            "duration_text": "12 mins",
            "duration_seconds": 720,
            "start_address": "A",
            "end_address": "B",
            "steps": [
                {
                    "html_instructions": "Head north",
                    "distance_text": "0.1 mi",
                    "duration_text": "1 min"
                }
            ]
        }"#)
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let resp = oc
        .maps()
        .directions(DirectionsRequest {
            origin: "A".to_string(),
            destination: "B".to_string(),
            mode: None,
        })
        .await
        .unwrap();

    assert_eq!(resp.distance_meters, 8369);
    assert_eq!(resp.duration_seconds, 720);
    assert_eq!(resp.steps.len(), 1);
    assert_eq!(resp.steps[0].html_instructions, "Head north");
    m.assert_async().await;
}

#[tokio::test]
async fn maps_directions_mode_forwarded_when_set() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/maps/directions")
        .match_body(mockito::Matcher::Json(serde_json::json!({
            "origin": "A",
            "destination": "B",
            "mode": "walking"
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{
            "distance_text": "", "distance_meters": 0,
            "duration_text": "", "duration_seconds": 0,
            "start_address": "", "end_address": "", "steps": []
        }"#)
        .create_async()
        .await;

    let oc = make_client(&server.url());
    oc.maps()
        .directions(DirectionsRequest {
            origin: "A".to_string(),
            destination: "B".to_string(),
            mode: Some("walking".to_string()),
        })
        .await
        .unwrap();

    m.assert_async().await;
}

// ─────────────────────────────────────── #3227 MapsService::validate_delivery_zone ──

#[tokio::test]
async fn maps_validate_delivery_zone_in_zone() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/maps/delivery-zones/validate")
        .match_body(mockito::Matcher::Json(serde_json::json!({
            "lat": 30.27,
            "lng": -97.74
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{
            "in_zone": true,
            "zone_id": "z-001",
            "zone_name": "Downtown",
            "eta_minutes": 25,
            "delivery_fee_cents": 299,
            "min_order_cents": 1500,
            "lat": 30.27,
            "lng": -97.74
        }"#)
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let resp = oc
        .maps()
        .validate_delivery_zone(ValidateDeliveryZoneRequest {
            lat: Some(30.27),
            lng: Some(-97.74),
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(resp.in_zone);
    assert_eq!(resp.zone_id.as_deref(), Some("z-001"));
    assert_eq!(resp.eta_minutes, Some(25));
    assert_eq!(resp.delivery_fee_cents, Some(299));
    m.assert_async().await;
}

#[tokio::test]
async fn maps_validate_delivery_zone_not_in_zone() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/maps/delivery-zones/validate")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"in_zone": false, "lat": 29.0, "lng": -98.0}"#)
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let resp = oc
        .maps()
        .validate_delivery_zone(ValidateDeliveryZoneRequest {
            lat: Some(29.0),
            lng: Some(-98.0),
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(!resp.in_zone);
    assert!(resp.zone_id.is_none());
    assert!(resp.eta_minutes.is_none());
}

#[tokio::test]
async fn maps_validate_delivery_zone_address_forwarded() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/maps/delivery-zones/validate")
        .match_body(mockito::Matcher::Json(serde_json::json!({
            "address": "1 Main St"
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"in_zone": false, "lat": 0.0, "lng": 0.0}"#)
        .create_async()
        .await;

    let oc = make_client(&server.url());
    oc.maps()
        .validate_delivery_zone(ValidateDeliveryZoneRequest {
            address: Some("1 Main St".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();

    m.assert_async().await;
}
