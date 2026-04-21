//! In-SDK silent access-token refresh (#3403 §1.4 / #3412).
//!
//! A spawned [`tokio`] task sleeps until `exp - refresh_margin`, POSTs
//! `/auth/refresh` with the cached refresh token, and — on success — swaps
//! the access token in [`crate::http::OlympusHttpClient`] and broadcasts a
//! [`SessionEvent::Refreshed`] on the client's broadcast channel. On
//! failure (network, 4xx, missing `exp`, etc.) the task broadcasts
//! [`SessionEvent::Expired`] and exits.
//!
//! The task is owned by the client via an [`Arc<Mutex<Option<AbortHandle>>>`];
//! calling [`crate::OlympusClient::stop_silent_refresh`] or issuing a fresh
//! [`crate::OlympusClient::start_silent_refresh`] cancels any prior task.
//! [`SilentRefreshHandle`] returned from `start_silent_refresh` also aborts
//! the task on `Drop`, giving callers a scoped control surface.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::task::AbortHandle;

use crate::http::OlympusHttpClient;
use crate::session::{AuthSession, SessionEvent};

/// Handle returned from [`crate::OlympusClient::start_silent_refresh`].
///
/// Dropping the handle aborts the background task — `abort` is idempotent,
/// so a subsequent `stop_silent_refresh` call on the client (or a second
/// `start_silent_refresh`) is safe.
pub struct SilentRefreshHandle {
    abort: Option<AbortHandle>,
}

impl SilentRefreshHandle {
    pub(crate) fn new(abort: AbortHandle) -> Self {
        Self { abort: Some(abort) }
    }

    /// Explicitly abort the task without dropping the handle. Idempotent.
    pub fn abort(&mut self) {
        if let Some(h) = self.abort.take() {
            h.abort();
        }
    }
}

impl Drop for SilentRefreshHandle {
    fn drop(&mut self) {
        self.abort();
    }
}

/// Shared silent-refresh state. Kept on [`crate::OlympusClient`] via an
/// `Arc` so the spawned task, the client, and the returned
/// [`SilentRefreshHandle`] all refer to the same channel + abort slot.
pub(crate) struct SilentRefreshState {
    /// Broadcast sender cloned by `session_events()` via `sender.subscribe()`.
    /// Created once in the client constructor — never recreated across
    /// start/stop cycles so pre-stop subscribers still see post-start events.
    pub sender: broadcast::Sender<SessionEvent>,

    /// The current task's abort handle. Replaced (old one aborted) on each
    /// `start_silent_refresh` call.
    pub current: Mutex<Option<AbortHandle>>,
}

impl SilentRefreshState {
    pub fn new() -> Arc<Self> {
        let (sender, _) = broadcast::channel(32);
        Arc::new(Self {
            sender,
            current: Mutex::new(None),
        })
    }

    /// Abort the currently-running task (if any). Used by
    /// `stop_silent_refresh`, a second `start_silent_refresh`, and `logout`.
    pub fn abort_current(&self) {
        let mut guard = self.current.lock().expect("poisoned");
        if let Some(h) = guard.take() {
            h.abort();
        }
    }

    pub fn set_current(&self, handle: AbortHandle) {
        let mut guard = self.current.lock().expect("poisoned");
        if let Some(old) = guard.replace(handle) {
            old.abort();
        }
    }

    /// Best-effort broadcast — missing receivers produce no error.
    pub fn emit(&self, event: SessionEvent) {
        // `send` fails only when there are zero receivers, which is fine.
        let _ = self.sender.send(event);
    }
}

/// Spawn the silent-refresh loop. Returns an `AbortHandle` that the caller
/// is expected to register via [`SilentRefreshState::set_current`] and
/// wrap in a [`SilentRefreshHandle`].
pub(crate) fn spawn_refresh_loop(
    http: Arc<OlympusHttpClient>,
    state: Arc<SilentRefreshState>,
    refresh_margin: Duration,
) -> AbortHandle {
    let handle = tokio::spawn(async move {
        run_refresh_loop(http, state, refresh_margin).await;
    });
    handle.abort_handle()
}

async fn run_refresh_loop(
    http: Arc<OlympusHttpClient>,
    state: Arc<SilentRefreshState>,
    refresh_margin: Duration,
) {
    loop {
        // Snapshot the token; bail if it was cleared.
        let access_token = match http.access_token_for_internal() {
            Some(t) if !t.is_empty() => t,
            _ => {
                state.emit(SessionEvent::Expired {
                    reason: "no access token".into(),
                });
                return;
            }
        };

        // Decode the `exp` claim from the JWT. A missing `exp` is terminal —
        // there is no way to schedule the next fire.
        let exp = match jwt_exp_seconds(&access_token) {
            Some(e) => e,
            None => {
                state.emit(SessionEvent::Expired {
                    reason: "no exp claim".into(),
                });
                return;
            }
        };

        let now = unix_now();
        let fire_in = compute_fire_delay(exp, now, refresh_margin);

        // Sleep. `tokio::time::sleep` works on the task-local clock, which
        // tokio::test can advance with `tokio::time::pause` / `advance`.
        tokio::time::sleep(fire_in).await;

        // Fetch refresh token + perform the exchange.
        let refresh_token = match http.refresh_token_for_internal() {
            Some(t) if !t.is_empty() => t,
            _ => {
                state.emit(SessionEvent::Expired {
                    reason: "no refresh token".into(),
                });
                return;
            }
        };

        let body = serde_json::json!({ "refresh_token": refresh_token });
        let response = match http.post("/auth/refresh", &body).await {
            Ok(v) => v,
            Err(e) => {
                state.emit(SessionEvent::Expired {
                    reason: format!("refresh failed: {e}"),
                });
                return;
            }
        };

        let session = AuthSession::from_json(&response);
        if session.access_token.is_empty() {
            state.emit(SessionEvent::Expired {
                reason: "refresh response missing access_token".into(),
            });
            return;
        }

        // Swap tokens in the shared HTTP client, then re-loop to schedule
        // the next refresh based on the new `exp`.
        http.set_access_token(&session.access_token);
        if !session.refresh_token.is_empty() {
            http.set_refresh_token(&session.refresh_token);
        }

        state.emit(SessionEvent::Refreshed(session));
    }
}

/// Decode the unsigned `exp` claim (seconds since epoch) from a JWT string
/// without verifying the signature. The SDK relies on the server as the
/// source of truth; `exp` is only used to schedule the next refresh.
///
/// Exposed for test coverage; not part of the SDK's public API contract.
pub fn jwt_exp_seconds(token: &str) -> Option<u64> {
    let parts: Vec<&str> = token.splitn(3, '.').collect();
    if parts.len() < 2 {
        return None;
    }
    let payload = base64_url_decode(parts[1])?;
    let value: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    value.get("exp").and_then(|v| v.as_u64())
}

fn base64_url_decode(s: &str) -> Option<Vec<u8>> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    URL_SAFE_NO_PAD.decode(s).ok()
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Compute the delay until the next refresh fire. Split out for test
/// coverage of clamp/edge behaviour.
///
/// - When `exp` is in the past (or within `refresh_margin`), fire immediately.
/// - Otherwise fire at `exp - refresh_margin`.
///
/// Exposed for test coverage; not part of the SDK's public API contract.
pub fn compute_fire_delay(exp: u64, now: u64, margin: Duration) -> Duration {
    if exp <= now {
        return Duration::from_secs(0);
    }
    let remaining = Duration::from_secs(exp - now);
    remaining.saturating_sub(margin)
}
