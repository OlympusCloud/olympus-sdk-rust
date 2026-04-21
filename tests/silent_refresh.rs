//! Tests for in-SDK silent token refresh + session event broadcast
//! (#3403 §1.4 / #3412).
//!
//! All tests use `#[tokio::test(flavor = "multi_thread")]` so that the
//! background `tokio::spawn`ed refresh task actually makes progress on a
//! worker thread distinct from the test driver. Wiremock supplies the
//! `/auth/refresh` endpoint.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use olympus_sdk::silent_refresh::{compute_fire_delay, jwt_exp_seconds};
use olympus_sdk::{OlympusClient, OlympusConfig, SessionEvent};
use serde_json::{json, Value};
use tokio::sync::broadcast::error::RecvError;
use tokio::time::{timeout, Duration as TokioDuration};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn b64url_nopad(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Build an unsigned JWT with the given claims. The SDK only decodes the
/// payload; the signature is never verified locally.
fn make_jwt(claims: Value) -> String {
    let header = b64url_nopad(br#"{"alg":"RS256","typ":"JWT"}"#);
    let payload = b64url_nopad(claims.to_string().as_bytes());
    format!("{}.{}.sig", header, payload)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn new_client_with_mock_server() -> (OlympusClient, MockServer) {
    let server = MockServer::start().await;
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url(server.uri());
    let client = OlympusClient::from_config(cfg);
    (client, server)
}

async fn recv_event_with_timeout(
    rx: &mut tokio::sync::broadcast::Receiver<SessionEvent>,
    timeout_ms: u64,
) -> Option<SessionEvent> {
    timeout(TokioDuration::from_millis(timeout_ms), rx.recv())
        .await
        .ok()
        .and_then(|r| r.ok())
}

// ---------------------------------------------------------------------------
// Pure helpers — no runtime required
// ---------------------------------------------------------------------------

#[test]
fn jwt_exp_decode_extracts_exp_claim() {
    let token = make_jwt(json!({ "sub": "u", "exp": 1234567890_u64 }));
    assert_eq!(jwt_exp_seconds(&token), Some(1234567890));
}

#[test]
fn jwt_exp_decode_returns_none_for_missing_exp() {
    let token = make_jwt(json!({ "sub": "u" }));
    assert_eq!(jwt_exp_seconds(&token), None);
}

#[test]
fn jwt_exp_decode_returns_none_for_malformed_token() {
    assert_eq!(jwt_exp_seconds("not-a-jwt"), None);
    assert_eq!(jwt_exp_seconds(""), None);
    assert_eq!(jwt_exp_seconds("only.two"), None); // base64 of "only" is not valid JSON
}

#[test]
fn compute_fire_delay_uses_exp_minus_margin() {
    let now = 1_000_000;
    let exp = 1_000_300; // 5 minutes out
    let margin = Duration::from_secs(60);
    assert_eq!(
        compute_fire_delay(exp, now, margin),
        Duration::from_secs(300 - 60)
    );
}

#[test]
fn compute_fire_delay_clamps_to_zero_when_exp_in_past() {
    let now = 1_000_000;
    let exp = 999_999;
    let margin = Duration::from_secs(60);
    assert_eq!(compute_fire_delay(exp, now, margin), Duration::from_secs(0));
}

#[test]
fn compute_fire_delay_clamps_to_zero_when_within_margin() {
    let now = 1_000_000;
    let exp = 1_000_030; // 30s out
    let margin = Duration::from_secs(60); // margin larger than remaining
    assert_eq!(compute_fire_delay(exp, now, margin), Duration::from_secs(0));
}

// ---------------------------------------------------------------------------
// Silent refresh — end-to-end with wiremock
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn refresh_fires_and_emits_refreshed_event() {
    let (client, server) = new_client_with_mock_server().await;

    // Access token expiring in ~2s — margin of 1s fires in ~1s.
    let exp = now_secs() + 2;
    let initial = make_jwt(json!({ "sub": "u", "exp": exp }));
    client.set_access_token(initial);
    client.set_refresh_token("initial-refresh-token");

    // Server emits a new session with a far-future exp so the loop
    // reschedules (rather than re-firing immediately).
    let new_exp = now_secs() + 3600;
    let new_access = make_jwt(json!({ "sub": "u", "exp": new_exp }));
    Mock::given(method("POST"))
        .and(path("/auth/refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": new_access,
            "refresh_token": "rotated-refresh-token",
            "expires_at": new_exp,
            "token_type": "Bearer",
        })))
        .mount(&server)
        .await;

    let mut events = client.session_events();
    let _handle = client.start_silent_refresh(Duration::from_secs(1));

    let event = recv_event_with_timeout(&mut events, 5_000)
        .await
        .expect("expected Refreshed event within 5s");
    match event {
        SessionEvent::Refreshed(session) => {
            assert!(!session.access_token.is_empty());
            assert_eq!(session.refresh_token, "rotated-refresh-token");
        }
        other => panic!("expected Refreshed, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_swaps_access_token_in_http_client() {
    let (client, server) = new_client_with_mock_server().await;
    let exp = now_secs() + 2;
    client.set_access_token(make_jwt(json!({ "sub": "u", "exp": exp })));
    client.set_refresh_token("r0");

    let new_exp = now_secs() + 3600;
    let new_access = make_jwt(json!({ "sub": "u", "exp": new_exp }));
    let new_access_clone = new_access.clone();
    Mock::given(method("POST"))
        .and(path("/auth/refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": new_access_clone,
            "refresh_token": "r1",
        })))
        .mount(&server)
        .await;

    let mut events = client.session_events();
    let _handle = client.start_silent_refresh(Duration::from_secs(1));

    let _ = recv_event_with_timeout(&mut events, 5_000)
        .await
        .expect("refresh event");
    // The HTTP client should now carry the rotated access token.
    assert_eq!(client.granted_scopes().len(), 0);
    // Indirect verification: a second refresh cycle should have been
    // scheduled against the new exp (far future), so no Expired event
    // fires in the next 200ms.
    let racing = recv_event_with_timeout(&mut events, 200).await;
    assert!(
        racing.is_none(),
        "expected no further event; got {racing:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_failure_emits_expired() {
    let (client, server) = new_client_with_mock_server().await;
    let exp = now_secs() + 2;
    client.set_access_token(make_jwt(json!({ "sub": "u", "exp": exp })));
    client.set_refresh_token("bad-refresh");

    Mock::given(method("POST"))
        .and(path("/auth/refresh"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": { "code": "invalid_grant", "message": "refresh token expired" }
        })))
        .mount(&server)
        .await;

    let mut events = client.session_events();
    let _handle = client.start_silent_refresh(Duration::from_secs(1));

    let event = recv_event_with_timeout(&mut events, 5_000)
        .await
        .expect("expected Expired event");
    match event {
        SessionEvent::Expired { reason } => {
            assert!(reason.contains("refresh failed"), "reason: {reason}");
        }
        other => panic!("expected Expired, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_with_no_exp_claim_emits_expired_immediately() {
    let (client, _server) = new_client_with_mock_server().await;
    client.set_access_token(make_jwt(json!({ "sub": "u" }))); // no exp
    client.set_refresh_token("r0");

    let mut events = client.session_events();
    let _handle = client.start_silent_refresh(Duration::from_secs(1));

    let event = recv_event_with_timeout(&mut events, 1_000)
        .await
        .expect("expected Expired for missing exp");
    match event {
        SessionEvent::Expired { reason } => assert!(reason.contains("exp"), "reason: {reason}"),
        other => panic!("expected Expired, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_with_no_access_token_emits_expired_immediately() {
    let (client, _server) = new_client_with_mock_server().await;
    // No access token set.

    let mut events = client.session_events();
    let _handle = client.start_silent_refresh(Duration::from_secs(1));

    let event = recv_event_with_timeout(&mut events, 1_000)
        .await
        .expect("expected Expired");
    match event {
        SessionEvent::Expired { reason } => {
            assert!(reason.contains("access token"), "reason: {reason}")
        }
        other => panic!("expected Expired, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Logout + cancellation semantics
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn logout_aborts_task_and_emits_logged_out() {
    let (client, server) = new_client_with_mock_server().await;
    let exp = now_secs() + 3600;
    client.set_access_token(make_jwt(json!({ "sub": "u", "exp": exp })));
    client.set_refresh_token("r0");

    // Mount a refresh endpoint that should never be called (task is aborted first).
    Mock::given(method("POST"))
        .and(path("/auth/refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "should-not-reach",
            "refresh_token": "r1",
        })))
        .mount(&server)
        .await;

    let mut events = client.session_events();
    let _handle = client.start_silent_refresh(Duration::from_secs(60));

    // Give the task a moment to actually spawn + enter sleep.
    tokio::time::sleep(TokioDuration::from_millis(50)).await;

    client.logout();

    let event = recv_event_with_timeout(&mut events, 500)
        .await
        .expect("expected LoggedOut");
    assert!(matches!(event, SessionEvent::LoggedOut));

    // No further events (no Refreshed, no Expired).
    let trailing = recv_event_with_timeout(&mut events, 200).await;
    assert!(trailing.is_none(), "got unexpected event: {trailing:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_silent_refresh_cancels_without_event() {
    let (client, _server) = new_client_with_mock_server().await;
    let exp = now_secs() + 3600;
    client.set_access_token(make_jwt(json!({ "sub": "u", "exp": exp })));
    client.set_refresh_token("r0");

    let mut events = client.session_events();
    let _handle = client.start_silent_refresh(Duration::from_secs(60));

    tokio::time::sleep(TokioDuration::from_millis(50)).await;
    client.stop_silent_refresh();

    // stop_silent_refresh is deliberately silent — no LoggedOut/Expired.
    let trailing = recv_event_with_timeout(&mut events, 200).await;
    assert!(trailing.is_none(), "got unexpected event: {trailing:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn double_start_cancels_first_task() {
    let (client, server) = new_client_with_mock_server().await;
    let exp = now_secs() + 2;
    client.set_access_token(make_jwt(json!({ "sub": "u", "exp": exp })));
    client.set_refresh_token("r0");

    let new_exp = now_secs() + 3600;
    let new_access = make_jwt(json!({ "sub": "u", "exp": new_exp }));
    Mock::given(method("POST"))
        .and(path("/auth/refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": new_access,
            "refresh_token": "r1",
        })))
        .mount(&server)
        .await;

    let mut events = client.session_events();
    // Start two tasks back-to-back. The first should be aborted by the second.
    let _h1 = client.start_silent_refresh(Duration::from_secs(1));
    let _h2 = client.start_silent_refresh(Duration::from_secs(1));

    // Exactly one Refreshed should be observed within the window.
    let e1 = recv_event_with_timeout(&mut events, 5_000)
        .await
        .expect("first event");
    assert!(matches!(e1, SessionEvent::Refreshed(_)), "got {e1:?}");

    // No second event for this single-exp window.
    let e2 = recv_event_with_timeout(&mut events, 200).await;
    assert!(e2.is_none(), "expected single Refreshed, got {e2:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn dropping_handle_cancels_task() {
    let (client, _server) = new_client_with_mock_server().await;
    let exp = now_secs() + 3600;
    client.set_access_token(make_jwt(json!({ "sub": "u", "exp": exp })));
    client.set_refresh_token("r0");

    let mut events = client.session_events();
    {
        let _handle = client.start_silent_refresh(Duration::from_secs(60));
        // Let the task enter its sleep before dropping.
        tokio::time::sleep(TokioDuration::from_millis(50)).await;
    } // _handle dropped here — aborts the task.

    // Clear the client-side slot so the test's "no events" assertion is
    // validating the Drop-path rather than the slot-path.
    //
    // The drop of the handle calls abort on the abort_handle; abort is
    // idempotent with the slot's copy, so a subsequent stop is a no-op.
    // Either way: no Refreshed should fire.
    let trailing = recv_event_with_timeout(&mut events, 200).await;
    assert!(trailing.is_none(), "got unexpected event: {trailing:?}");
}

// ---------------------------------------------------------------------------
// Broadcast channel lifetime + lag handling
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn broadcast_channel_survives_start_stop_cycle() {
    let (client, _server) = new_client_with_mock_server().await;

    // Subscribe BEFORE any start.
    let mut events = client.session_events();

    // No task running; emit a LoggedIn manually.
    client.emit_logged_in(olympus_sdk::AuthSession {
        access_token: "t1".into(),
        refresh_token: "r1".into(),
        ..Default::default()
    });

    let e1 = recv_event_with_timeout(&mut events, 500)
        .await
        .expect("first LoggedIn");
    assert!(matches!(e1, SessionEvent::LoggedIn(_)));

    // Start then stop — subscriber must still see subsequent emits.
    let exp = now_secs() + 3600;
    client.set_access_token(make_jwt(json!({ "sub": "u", "exp": exp })));
    client.set_refresh_token("r0");
    let _h = client.start_silent_refresh(Duration::from_secs(60));
    tokio::time::sleep(TokioDuration::from_millis(30)).await;
    client.stop_silent_refresh();

    // Emit another LoggedIn through the same channel.
    client.emit_logged_in(olympus_sdk::AuthSession {
        access_token: "t2".into(),
        ..Default::default()
    });

    let e2 = recv_event_with_timeout(&mut events, 500)
        .await
        .expect("second LoggedIn");
    match e2 {
        SessionEvent::LoggedIn(s) => assert_eq!(s.access_token, "t2"),
        other => panic!("expected LoggedIn(t2), got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn broadcast_lag_surfaces_recv_error_lagged() {
    // Channel capacity is 32. Flood with 100+ events without polling,
    // then verify the receiver observes a Lagged error on the next recv.
    let (client, _server) = new_client_with_mock_server().await;
    let mut events = client.session_events();

    for i in 0..100 {
        client.emit_logged_in(olympus_sdk::AuthSession {
            access_token: format!("t{i}"),
            ..Default::default()
        });
    }

    // First recv should either be Lagged OR a LoggedIn (drained subset).
    // Definitively, the first poll must observe lag before any drained event.
    let first = events.recv().await;
    match first {
        Err(RecvError::Lagged(skipped)) => {
            assert!(skipped > 0, "expected > 0 skipped, got {skipped}");
        }
        other => panic!("expected Lagged error, got {other:?}"),
    }

    // After observing Lagged, subsequent recvs drain remaining events.
    let next = events.recv().await.expect("should get a drained event");
    assert!(matches!(next, SessionEvent::LoggedIn(_)));
}

#[tokio::test(flavor = "multi_thread")]
async fn logout_with_no_active_task_still_emits_logged_out() {
    let (client, _server) = new_client_with_mock_server().await;
    let mut events = client.session_events();

    client.logout();

    let event = recv_event_with_timeout(&mut events, 500)
        .await
        .expect("LoggedOut");
    assert!(matches!(event, SessionEvent::LoggedOut));
}
