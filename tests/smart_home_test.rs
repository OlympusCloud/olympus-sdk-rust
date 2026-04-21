//! Integration tests for the SmartHomeService (Wave 2).

use mockito::Server;
use olympus_sdk::services::smart_home::ListDevicesOptions;
use olympus_sdk::{OlympusClient, OlympusConfig, OlympusError};
use serde_json::json;

fn make_client(base_url: &str) -> OlympusClient {
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url(base_url);
    OlympusClient::from_config(cfg)
}

#[tokio::test]
async fn list_platforms_returns_envelope() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/smart-home/platforms")
        .with_status(200)
        .with_body(r#"{"platforms": [{"id": "hue"}]}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc.smart_home().list_platforms().await.expect("ok");
    assert_eq!(resp["platforms"][0]["id"], json!("hue"));
    m.assert_async().await;
}

#[tokio::test]
async fn list_devices_with_filters_appends_query() {
    let mut server = Server::new_async().await;
    let m = server
        .mock(
            "GET",
            "/smart-home/devices?platform_id=hue&room_id=living-room",
        )
        .with_status(200)
        .with_body(r#"{"devices": []}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .smart_home()
        .list_devices(ListDevicesOptions {
            platform_id: Some("hue"),
            room_id: Some("living-room"),
        })
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn list_devices_no_filters_omits_query() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/smart-home/devices")
        .with_status(200)
        .with_body(r#"{"devices": []}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .smart_home()
        .list_devices(ListDevicesOptions::default())
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn get_device_url_encodes_id() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/smart-home/devices/dev%2F1")
        .with_status(200)
        .with_body(r#"{"id": "dev/1"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc.smart_home().get_device("dev/1").await.expect("ok");
    assert_eq!(resp["id"], json!("dev/1"));
    m.assert_async().await;
}

#[tokio::test]
async fn control_device_posts_command() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/smart-home/devices/d1/control")
        .match_body(mockito::Matcher::Json(json!({"on": true})))
        .with_status(200)
        .with_body(r#"{"ok": true}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc
        .smart_home()
        .control_device("d1", json!({"on": true}))
        .await
        .expect("ok");
    assert_eq!(resp["ok"], json!(true));
    m.assert_async().await;
}

#[tokio::test]
async fn list_rooms_returns_envelope() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/smart-home/rooms")
        .with_status(200)
        .with_body(r#"{"rooms": []}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc.smart_home().list_rooms().await.expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn scene_full_lifecycle() {
    let mut server = Server::new_async().await;
    let list = server
        .mock("GET", "/smart-home/scenes")
        .with_status(200)
        .with_body(r#"{"scenes": [{"id": "morning"}]}"#)
        .create_async()
        .await;
    let activate = server
        .mock("POST", "/smart-home/scenes/morning/activate")
        .with_status(200)
        .with_body(r#"{"activated": true}"#)
        .create_async()
        .await;
    let create = server
        .mock("POST", "/smart-home/scenes")
        .match_body(mockito::Matcher::Json(json!({"name": "Movie night"})))
        .with_status(201)
        .with_body(r#"{"id": "movie", "name": "Movie night"}"#)
        .create_async()
        .await;
    let del = server
        .mock("DELETE", "/smart-home/scenes/movie")
        .with_status(204)
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let scenes = oc.smart_home().list_scenes().await.expect("list");
    assert_eq!(scenes["scenes"][0]["id"], json!("morning"));

    let activated = oc
        .smart_home()
        .activate_scene("morning")
        .await
        .expect("activate");
    assert_eq!(activated["activated"], json!(true));

    let created = oc
        .smart_home()
        .create_scene(json!({"name": "Movie night"}))
        .await
        .expect("create");
    assert_eq!(created["id"], json!("movie"));

    oc.smart_home().delete_scene("movie").await.expect("delete");

    list.assert_async().await;
    activate.assert_async().await;
    create.assert_async().await;
    del.assert_async().await;
}

#[tokio::test]
async fn automation_lifecycle() {
    let mut server = Server::new_async().await;
    let list = server
        .mock("GET", "/smart-home/automations")
        .with_status(200)
        .with_body(r#"{"automations": []}"#)
        .create_async()
        .await;
    let create = server
        .mock("POST", "/smart-home/automations")
        .with_status(201)
        .with_body(r#"{"id": "a1"}"#)
        .create_async()
        .await;
    let del = server
        .mock("DELETE", "/smart-home/automations/a1")
        .with_status(204)
        .create_async()
        .await;

    let oc = make_client(&server.url());
    let _ = oc.smart_home().list_automations().await.expect("list");
    let created = oc
        .smart_home()
        .create_automation(json!({"name": "auto"}))
        .await
        .expect("create");
    assert_eq!(created["id"], json!("a1"));
    oc.smart_home()
        .delete_automation("a1")
        .await
        .expect("delete");
    list.assert_async().await;
    create.assert_async().await;
    del.assert_async().await;
}

#[tokio::test]
async fn list_platforms_propagates_server_error() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/smart-home/platforms")
        .with_status(503)
        .with_body(r#"{"error": {"message": "unavailable"}}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let res = oc.smart_home().list_platforms().await;
    match res {
        Err(OlympusError::Api { status, .. }) => assert_eq!(status, 503),
        other => panic!("expected Api error, got {:?}", other),
    }
    m.assert_async().await;
}
