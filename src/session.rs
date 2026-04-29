//! Session types shared across auth flows + silent refresh (#3403 §1.4 / #3412).
//!
//! [`AuthSession`] is a minimal view of the `/auth/login` + `/auth/refresh`
//! response bodies — carrying just the tokens + expiry the SDK needs for
//! silent refresh. Service call responses remain `serde_json::Value`; the
//! session struct is materialized when a handler is attached or when the
//! silent-refresh task needs to broadcast a transition.
//!
//! [`SessionEvent`] is the broadcast transition emitted by
//! [`crate::OlympusClient::session_events`] — consumers subscribe with a
//! [`tokio::sync::broadcast::Receiver`] and react to lifecycle changes
//! (login, silent refresh, forced expiry, explicit logout).

use serde::{Deserialize, Serialize};

/// Minimal representation of a server-issued auth session.
///
/// Fields are optional because the SDK may receive partial responses from
/// the gateway (e.g. a refresh that doesn't echo the user_id). Consumers
/// that need the full shape should inspect the raw `serde_json::Value`
/// returned by [`crate::services::auth::AuthService::login`].
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthSession {
    /// Short-lived access token (Authorization bearer).
    pub access_token: String,

    /// Refresh token used by the silent-refresh task. May be empty if the
    /// server issues refresh tokens out-of-band (e.g. via a cookie).
    #[serde(default)]
    pub refresh_token: String,

    /// Unix seconds at which the access token expires. `0` means unknown —
    /// the SDK will fall back to decoding the JWT `exp` claim.
    #[serde(default)]
    pub expires_at: u64,

    /// Token type (typically `"Bearer"`).
    #[serde(default)]
    pub token_type: String,

    /// User id that owns this session, when echoed by the server.
    #[serde(default)]
    pub user_id: Option<String>,

    /// Tenant id for this session, when echoed by the server.
    #[serde(default)]
    pub tenant_id: Option<String>,

    /// Franchise / company identifier. Present only for tenants that belong to
    /// a company hierarchy (see olympus-cloud-gcp#3151). `None` for standalone
    /// single-tenant logins. Decoded from the JWT `company_id` claim.
    #[serde(default)]
    pub company_id: Option<String>,
}

impl AuthSession {
    /// Construct an `AuthSession` from a JSON response body, tolerating the
    /// common shapes emitted by `/auth/login` and `/auth/refresh`.
    ///
    /// Accepts both flat shapes (`{access_token, refresh_token, ...}`) and
    /// envelope shapes (`{session: {...}}` or `{data: {...}}`).
    pub fn from_json(value: &serde_json::Value) -> Self {
        let inner = value
            .get("session")
            .or_else(|| value.get("data"))
            .unwrap_or(value);

        let access_token = inner
            .get("access_token")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let refresh_token = inner
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let expires_at = inner
            .get("expires_at")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let token_type = inner
            .get("token_type")
            .and_then(|v| v.as_str())
            .unwrap_or("Bearer")
            .to_string();
        let user_id = inner
            .get("user_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let tenant_id = inner
            .get("tenant_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let company_id = inner
            .get("company_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Self {
            access_token,
            refresh_token,
            expires_at,
            token_type,
            user_id,
            tenant_id,
            company_id,
        }
    }
}

/// Lifecycle transitions broadcast over
/// [`crate::OlympusClient::session_events`]. `Clone + Debug` is required by
/// the [`tokio::sync::broadcast`] channel.
#[derive(Debug, Clone)]
pub enum SessionEvent {
    /// A new session was established (e.g. after `login`). Tokens are active
    /// and the silent-refresh task has been (re)scheduled for its `exp`.
    LoggedIn(AuthSession),

    /// The silent-refresh task successfully exchanged the refresh token for
    /// a fresh session. The next refresh has been scheduled.
    Refreshed(AuthSession),

    /// The session is no longer usable. Emitted when the silent-refresh
    /// HTTP call fails or the token has no usable `exp` claim.
    Expired {
        /// Human-readable reason suitable for logging. Not localized.
        reason: String,
    },

    /// The caller explicitly logged out — tokens were cleared and the
    /// silent-refresh task (if any) was aborted.
    LoggedOut,
}
