//! Integration tests for the VoiceService (Wave 1 typed paths + Wave 2
//! dart-parity additions).

use mockito::Server;
use olympus_sdk::services::voice::{
    CloneAgentRequest, CreateAgentRequest, GetAnalyticsOptions, ListCallerProfilesOptions,
    ListConversationsOptions, ListGeminiVoicesOptions, ListPersonasOptions, ListVoicemailsOptions,
    ListVoicesOptions, PageOptions, PreviewAgentVoiceRequest, ProcessAudioRequest,
    ProvisionAgentRequest, SearchNumbersOptions, UpdateAgentAmbianceRequest, UpdateAgentRequest,
    UpdateAgentVoiceOverridesRequest, UploadAmbianceBedRequest,
};
use olympus_sdk::{OlympusClient, OlympusConfig, OlympusError};
use serde_json::{json, Map};

fn make_client(base_url: &str) -> OlympusClient {
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url(base_url);
    OlympusClient::from_config(cfg)
}

// ---------------------------------------------------------------------------
// V2-005 typed cascade-resolver paths (preserved from Wave 1).
// ---------------------------------------------------------------------------

const EFFECTIVE_CONFIG_FIXTURE: &str = r#"{
    "agentId": "agent_abc",
    "tenantId": "tenant_xyz",
    "pipeline": "olympus_native",
    "pipelineConfig": {"defaultLogLevel": "INFO"},
    "tierOverride": "T3",
    "logLevel": "INFO",
    "debugTranscriptsEnabled": false,
    "v2ShadowEnabled": false,
    "v2PrimaryEnabled": false,
    "voiceDefaults": {},
    "resolvedAt": "2026-04-19T00:00:00Z",
    "cascadeVersion": "v2.1-rename"
}"#;

#[tokio::test]
async fn get_effective_config_returns_typed_struct() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/voice-agents/configs/agent_abc/effective-config")
        .with_status(200)
        .with_body(EFFECTIVE_CONFIG_FIXTURE)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let cfg = oc
        .voice()
        .get_effective_config("agent_abc")
        .await
        .expect("ok");
    assert_eq!(cfg.agent_id, "agent_abc");
    assert_eq!(cfg.cascade_version, "v2.1-rename");
    assert_eq!(cfg.tier_override.as_deref(), Some("T3"));
    m.assert_async().await;
}

#[tokio::test]
async fn get_pipeline_returns_typed_struct() {
    let mut server = Server::new_async().await;
    let body = r#"{
        "agentId": "a", "pipeline": "olympus_native",
        "pipelineConfig": {}, "resolvedAt": "2026-04-19T00:00:00Z",
        "cascadeVersion": "v2.1"
    }"#;
    let m = server
        .mock("GET", "/voice-agents/configs/a/pipeline")
        .with_status(200)
        .with_body(body)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let pipe = oc.voice().get_pipeline("a").await.expect("ok");
    assert_eq!(pipe.pipeline, "olympus_native");
    m.assert_async().await;
}

// ---------------------------------------------------------------------------
// Agent CRUD
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_configs_with_page_opts() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/voice-agents/configs?page=1&limit=20&tenant_id=t1")
        .with_status(200)
        .with_body(r#"{"configs": []}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .list_configs(PageOptions {
            page: Some(1),
            limit: Some(20),
            tenant_id: Some("t1"),
        })
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn list_configs_no_opts_omits_query() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/voice-agents/configs")
        .with_status(200)
        .with_body(r#"{"configs": []}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .list_configs(PageOptions::default())
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn get_config_url_encodes_id() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/voice-agents/configs/cfg%2F1")
        .with_status(200)
        .with_body(r#"{"id": "cfg/1"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc.voice().get_config("cfg/1").await.expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn create_config_passes_through_body() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice-agents/configs")
        .match_body(mockito::Matcher::Json(json!({"name": "Greeter"})))
        .with_status(201)
        .with_body(r#"{"id": "agent_new"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc
        .voice()
        .create_config(json!({"name": "Greeter"}))
        .await
        .expect("ok");
    assert_eq!(resp["id"], json!("agent_new"));
    m.assert_async().await;
}

#[tokio::test]
async fn update_config_puts_body() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("PUT", "/voice-agents/configs/a1")
        .match_body(mockito::Matcher::Json(json!({"name": "x"})))
        .with_status(200)
        .with_body(r#"{"id": "a1"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .update_config("a1", json!({"name": "x"}))
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn delete_config_succeeds() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("DELETE", "/voice-agents/configs/a1")
        .with_status(204)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    oc.voice().delete_config("a1").await.expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn create_agent_emits_only_set_fields() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice-agents/configs")
        .match_body(mockito::Matcher::Json(json!({
            "name": "Greeter",
            "voice_id": "v_1",
            "phone_number": "+1555",
            "ambiance_config": {"enabled": true}
        })))
        .with_status(201)
        .with_body(r#"{"id": "agent_2"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .create_agent(CreateAgentRequest {
            name: Some("Greeter"),
            voice_id: Some("v_1"),
            phone_number: Some("+1555"),
            ambiance_config: Some(json!({"enabled": true})),
            ..Default::default()
        })
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn update_agent_emits_only_set_fields_and_is_active() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("PUT", "/voice-agents/configs/a1")
        .match_body(mockito::Matcher::Json(json!({
            "greeting": "Hello!",
            "is_active": false
        })))
        .with_status(200)
        .with_body(r#"{"id": "a1"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .update_agent(
            "a1",
            UpdateAgentRequest {
                greeting: Some("Hello!"),
                is_active: Some(false),
                ..Default::default()
            },
        )
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn clone_agent_posts_optional_fields() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice-agents/configs/a1/clone")
        .match_body(mockito::Matcher::Json(json!({"new_name": "copy"})))
        .with_status(201)
        .with_body(r#"{"id": "a2"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .clone_agent(
            "a1",
            CloneAgentRequest {
                new_name: Some("copy"),
                ..Default::default()
            },
        )
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn preview_agent_voice_includes_sample_text() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice-agents/configs/a1/preview")
        .match_body(mockito::Matcher::Json(json!({
            "sample_text": "Welcome",
            "voice_id": "v_1"
        })))
        .with_status(200)
        .with_body(r#"{"audio_url": "https://x"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .preview_agent_voice(
            "a1",
            PreviewAgentVoiceRequest {
                sample_text: "Welcome",
                voice_id: Some("v_1"),
                voice_overrides: None,
            },
        )
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn list_gemini_voices_with_language_query() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/voice/voices?language=en-US")
        .with_status(200)
        .with_body(r#"{"voices": []}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .list_gemini_voices(ListGeminiVoicesOptions {
            language: Some("en-US"),
        })
        .await
        .expect("ok");
    m.assert_async().await;
}

// ---------------------------------------------------------------------------
// Pool / schedule / provisioning
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pool_lifecycle() {
    let mut server = Server::new_async().await;
    let get = server
        .mock("GET", "/voice-agents/a1/pool")
        .with_status(200)
        .with_body(r#"{"pool": []}"#)
        .create_async()
        .await;
    let add = server
        .mock("POST", "/voice-agents/a1/pool")
        .with_status(201)
        .with_body(r#"{"id": "e1"}"#)
        .create_async()
        .await;
    let del = server
        .mock("DELETE", "/voice-agents/a1/pool/e1")
        .with_status(204)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc.voice().get_pool("a1").await.expect("ok");
    let _ = oc
        .voice()
        .add_to_pool("a1", json!({"voice_id": "v"}))
        .await
        .expect("ok");
    oc.voice().remove_from_pool("a1", "e1").await.expect("ok");
    get.assert_async().await;
    add.assert_async().await;
    del.assert_async().await;
}

#[tokio::test]
async fn schedule_get_and_update() {
    let mut server = Server::new_async().await;
    let g = server
        .mock("GET", "/voice-agents/a1/schedule")
        .with_status(200)
        .with_body(r#"{"timezone": "UTC"}"#)
        .create_async()
        .await;
    let u = server
        .mock("PUT", "/voice-agents/a1/schedule")
        .match_body(mockito::Matcher::Json(json!({"timezone": "PT"})))
        .with_status(200)
        .with_body(r#"{"timezone": "PT"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc.voice().get_schedule("a1").await.expect("ok");
    let _ = oc
        .voice()
        .update_schedule("a1", json!({"timezone": "PT"}))
        .await
        .expect("ok");
    g.assert_async().await;
    u.assert_async().await;
}

#[tokio::test]
async fn provision_agent_posts_full_body() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/ether/voice/agents/a1/provision-wizard")
        .match_body(mockito::Matcher::Json(json!({
            "tenant_id": "t",
            "voice_name": "Aria",
            "profile": {"role": "host"},
            "greeting_text": "Hi"
        })))
        .with_status(202)
        .with_body(r#"{"job_id": "j_1"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc
        .voice()
        .provision_agent(ProvisionAgentRequest {
            agent_id: "a1",
            tenant_id: "t",
            voice_name: "Aria",
            profile: json!({"role": "host"}),
            greeting_text: "Hi",
        })
        .await
        .expect("ok");
    assert_eq!(resp["job_id"], json!("j_1"));
    m.assert_async().await;
}

#[tokio::test]
async fn provisioning_status_includes_job_id_query() {
    let mut server = Server::new_async().await;
    let m = server
        .mock(
            "GET",
            "/ether/voice/agents/a1/provisioning-status?job_id=j_1",
        )
        .with_status(200)
        .with_body(r#"{"status": "running"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .get_provisioning_status("a1", "j_1")
        .await
        .expect("ok");
    m.assert_async().await;
}

// ---------------------------------------------------------------------------
// Personas / templates / ambiance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_personas_with_premium_only() {
    let mut server = Server::new_async().await;
    let m = server
        .mock(
            "GET",
            "/voice/personas?category=hospitality&premium_only=true",
        )
        .with_status(200)
        .with_body(r#"{"personas": []}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .list_personas(ListPersonasOptions {
            category: Some("hospitality"),
            industry: None,
            premium_only: Some(true),
        })
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn apply_persona_posts_persona_field() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice-agents/configs/a1/apply-persona")
        .match_body(mockito::Matcher::Json(json!({"persona": "warm-host"})))
        .with_status(200)
        .with_body(r#"{"applied": true}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .apply_persona_to_agent("a1", "warm-host")
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn upload_ambiance_bed_base64_encodes_audio() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice/ambiance/upload")
        .match_body(mockito::Matcher::Json(json!({
            "name": "rain",
            "audio_base64": "AQIDBA==",
            "time_of_day": "evening"
        })))
        .with_status(201)
        .with_body(r#"{"id": "bed_1"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .upload_ambiance_bed(UploadAmbianceBedRequest {
            name: "rain",
            audio_bytes: &[1u8, 2, 3, 4],
            time_of_day: Some("evening"),
            description: None,
        })
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn update_agent_ambiance_patches_only_set_fields() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("PATCH", "/voice-agents/configs/a1/ambiance")
        .match_body(mockito::Matcher::Json(json!({
            "enabled": true,
            "intensity": 0.5
        })))
        .with_status(200)
        .with_body(r#"{"updated": true}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .update_agent_ambiance(
            "a1",
            UpdateAgentAmbianceRequest {
                enabled: Some(true),
                intensity: Some(0.5),
                ..Default::default()
            },
        )
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn update_agent_voice_overrides_patches_floats() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("PATCH", "/voice-agents/configs/a1/voice-overrides")
        .match_body(mockito::Matcher::Json(json!({
            "pitch": 1.2,
            "regional_dialect": "us-southern"
        })))
        .with_status(200)
        .with_body(r#"{}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .update_agent_voice_overrides(
            "a1",
            UpdateAgentVoiceOverridesRequest {
                pitch: Some(1.2),
                speed: None,
                warmth: None,
                regional_dialect: Some("us-southern"),
            },
        )
        .await
        .expect("ok");
    m.assert_async().await;
}

// ---------------------------------------------------------------------------
// Voicemail / conversations / analytics / messages
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_voicemails_with_filters() {
    let mut server = Server::new_async().await;
    let m = server
        .mock(
            "GET",
            "/voice/voicemails?caller_phone=%2B1555&page=2&limit=10",
        )
        .with_status(200)
        .with_body(r#"{"voicemails": []}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .list_voicemails(ListVoicemailsOptions {
            caller_phone: Some("+1555"),
            page: Some(2),
            limit: Some(10),
        })
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn list_conversations_with_filters() {
    let mut server = Server::new_async().await;
    let m = server
        .mock(
            "GET",
            "/voice-agents/conversations?agent_id=a1&status=ended&page=1&limit=50",
        )
        .with_status(200)
        .with_body(r#"{"conversations": []}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .list_conversations(ListConversationsOptions {
            agent_id: Some("a1"),
            status: Some("ended"),
            page: Some(1),
            limit: Some(50),
            tenant_id: None,
        })
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn get_analytics_with_date_range() {
    let mut server = Server::new_async().await;
    let m = server
        .mock(
            "GET",
            "/voice-agents/analytics?from=2026-04-01&to=2026-04-30",
        )
        .with_status(200)
        .with_body(r#"{"calls": 100}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .get_analytics(GetAnalyticsOptions {
            agent_id: None,
            from: Some("2026-04-01"),
            to: Some("2026-04-30"),
        })
        .await
        .expect("ok");
    m.assert_async().await;
}

// ---------------------------------------------------------------------------
// Phone numbers + marketplace
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_numbers_with_area_code() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/voice/phone-numbers/search?area_code=415&limit=5")
        .with_status(200)
        .with_body(r#"{"numbers": []}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .search_numbers(SearchNumbersOptions {
            area_code: Some("415"),
            contains: None,
            country: None,
            limit: Some(5),
        })
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn assign_number_posts_agent_id() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice/phone-numbers/n1/assign")
        .match_body(mockito::Matcher::Json(json!({"agent_id": "a1"})))
        .with_status(200)
        .with_body(r#"{"assigned": true}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc.voice().assign_number("n1", "a1").await.expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn list_voices_marketplace_with_filters() {
    let mut server = Server::new_async().await;
    let m = server
        .mock(
            "GET",
            "/voice/marketplace/voices?language=en&gender=female&limit=20",
        )
        .with_status(200)
        .with_body(r#"{"voices": []}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .list_voices(ListVoicesOptions {
            language: Some("en"),
            gender: Some("female"),
            limit: Some(20),
        })
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn install_pack_posts_empty_body() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice/marketplace/packs/p1/install")
        .with_status(200)
        .with_body(r#"{"installed": true}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc.voice().install_pack("p1").await.expect("ok");
    m.assert_async().await;
}

// ---------------------------------------------------------------------------
// Calls + speaker + caller profiles
// ---------------------------------------------------------------------------

#[tokio::test]
async fn end_call_posts_to_calls_end() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice/calls/c1/end")
        .with_status(204)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    oc.voice().end_call("c1").await.expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn add_words_posts_words_array() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice/speaker/s1/words")
        .match_body(mockito::Matcher::Json(json!({"words": ["alpha", "beta"]})))
        .with_status(200)
        .with_body(r#"{}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    oc.voice()
        .add_words("s1", vec!["alpha".into(), "beta".into()])
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn list_caller_profiles_uses_default_limit_offset() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/caller-profiles?limit=50&offset=0")
        .with_status(200)
        .with_body(r#"{"profiles": []}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .list_caller_profiles(ListCallerProfilesOptions::default())
        .await
        .expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn record_caller_order_posts_order_data() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/caller-profiles/%2B1555/orders")
        .match_body(mockito::Matcher::Json(json!({"items": []})))
        .with_status(201)
        .with_body(r#"{"recorded": true}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .record_caller_order("+1555", json!({"items": []}))
        .await
        .expect("ok");
    m.assert_async().await;
}

// ---------------------------------------------------------------------------
// Edge pipeline
// ---------------------------------------------------------------------------

#[tokio::test]
async fn process_audio_base64_encodes_bytes() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice/process")
        .match_body(mockito::Matcher::Json(json!({
            "audio": "AQIDBA==",
            "language": "en"
        })))
        .with_status(200)
        .with_body(r#"{"transcript": "hi"}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc
        .voice()
        .process_audio(ProcessAudioRequest {
            audio_bytes: &[1u8, 2, 3, 4],
            language: Some("en"),
            agent_id: None,
            voice_id: None,
            session_id: None,
        })
        .await
        .expect("ok");
    assert_eq!(resp["transcript"], json!("hi"));
    m.assert_async().await;
}

#[tokio::test]
async fn pipeline_health_returns_status() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/voice/pipeline/health")
        .with_status(200)
        .with_body(r#"{"healthy": true}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let resp = oc.voice().pipeline_health().await.expect("ok");
    assert_eq!(resp["healthy"], json!(true));
    m.assert_async().await;
}

#[test]
fn voice_websocket_url_swaps_https_for_wss() {
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url("https://api.example.com");
    let oc = OlympusClient::from_config(cfg);
    let url = oc.voice().get_voice_web_socket_url(None);
    assert_eq!(url, "wss://api.example.com/ws/voice");
}

#[test]
fn voice_websocket_url_includes_session_id() {
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url("https://api.example.com");
    let oc = OlympusClient::from_config(cfg);
    let url = oc.voice().get_voice_web_socket_url(Some("sess/1"));
    assert_eq!(url, "wss://api.example.com/ws/voice?session_id=sess%2F1");
}

#[test]
fn voice_websocket_url_trims_trailing_slash() {
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url("https://api.example.com/");
    let oc = OlympusClient::from_config(cfg);
    let url = oc.voice().get_voice_web_socket_url(None);
    assert_eq!(url, "wss://api.example.com/ws/voice");
}

// ---------------------------------------------------------------------------
// Test agent + escalation/business-hours config
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_agent_posts_tenant_and_count() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/voice-agents/test")
        .match_body(mockito::Matcher::Json(json!({
            "tenant_id": "t1",
            "scenario_count": 7
        })))
        .with_status(200)
        .with_body(r#"{"score": 0.92}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc.voice().test_agent("t1", 7).await.expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn escalation_config_get_and_update() {
    let mut server = Server::new_async().await;
    let g = server
        .mock("GET", "/voice-agents/a1/escalation-config")
        .with_status(200)
        .with_body(r#"{}"#)
        .create_async()
        .await;
    let u = server
        .mock("PUT", "/voice-agents/a1/escalation-config")
        .match_body(mockito::Matcher::Json(json!({"sentiment_threshold": 0.4})))
        .with_status(200)
        .with_body(r#"{}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc.voice().get_escalation_config("a1").await.expect("ok");
    let _ = oc
        .voice()
        .update_escalation_config("a1", json!({"sentiment_threshold": 0.4}))
        .await
        .expect("ok");
    g.assert_async().await;
    u.assert_async().await;
}

// ---------------------------------------------------------------------------
// Error path + alias coverage
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_configs_propagates_server_error() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/voice-agents/configs")
        .with_status(500)
        .with_body(r#"{"error": {"message": "internal"}}"#)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let res = oc.voice().list_configs(PageOptions::default()).await;
    match res {
        Err(OlympusError::Api { status, .. }) => assert_eq!(status, 500),
        other => panic!("expected Api error, got {:?}", other),
    }
    m.assert_async().await;
}

#[tokio::test]
async fn list_agents_aliases_list_configs() {
    let mut server = Server::new_async().await;
    // Two separate calls — both hit /voice-agents/configs
    let m = server
        .mock("GET", "/voice-agents/configs")
        .with_status(200)
        .with_body(r#"{"configs": []}"#)
        .expect(2)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let _ = oc
        .voice()
        .list_configs(PageOptions::default())
        .await
        .expect("ok");
    let _ = oc
        .voice()
        .list_agents(PageOptions::default())
        .await
        .expect("ok");
    m.assert_async().await;
}

// Smoke check — a Map<String, Value> compiles into the ambiance variants field.
#[test]
fn update_agent_ambiance_request_accepts_time_of_day_variants() {
    let mut variants = Map::new();
    variants.insert("morning".into(), json!("rain.mp3"));
    let _ = UpdateAgentAmbianceRequest {
        enabled: Some(true),
        intensity: None,
        default_r2_key: None,
        time_of_day_variants: Some(variants),
    };
}
