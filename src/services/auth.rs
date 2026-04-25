use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;
use crate::services::firebase_auth::{FirebaseLinkResult, LoginWithFirebaseOptions};

/// Authentication and user management service.
///
/// Wraps the Olympus Auth service (Rust, port 8001) via the Go API Gateway.
/// Routes: `/auth/*`, `/platform/users/*`.
pub struct AuthService {
    http: Arc<OlympusHttpClient>,
}

impl AuthService {
    /// Creates a new AuthService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Authenticates a user with email and password.
    pub async fn login(&self, email: &str, password: &str) -> Result<Value> {
        let body = json!({
            "email": email,
            "password": password,
        });
        self.http.post("/auth/login", &body).await
    }

    /// Registers a new user account.
    pub async fn register(&self, email: &str, password: &str, name: &str) -> Result<Value> {
        let body = json!({
            "email": email,
            "password": password,
            "name": name,
        });
        self.http.post("/auth/register", &body).await
    }

    /// Validates an access token and returns the associated user.
    pub async fn validate(&self, token: &str) -> Result<Value> {
        let body = json!({
            "token": token,
        });
        self.http.post("/auth/validate", &body).await
    }

    /// Exchanges a refresh token for a new token pair.
    pub async fn refresh(&self, refresh_token: &str) -> Result<Value> {
        let body = json!({
            "refresh_token": refresh_token,
        });
        self.http.post("/auth/refresh", &body).await
    }

    /// Authenticates via a Firebase ID token (#3275).
    ///
    /// When `tenant_slug` is omitted the backend auto-resolves the tenant
    /// from the Firebase UID's identity link. When the lookup matches more
    /// than one tenant the call returns
    /// [`crate::error::OlympusError::TenantAmbiguous`]; apps render a
    /// picker from the candidates and retry with the chosen slug.
    ///
    /// Other typed failures (all variants of `OlympusError`):
    /// `IdentityUnlinked`, `NoTenantMatch`, `InvalidFirebaseToken`.
    pub async fn login_with_firebase(
        &self,
        firebase_id_token: &str,
        opts: LoginWithFirebaseOptions,
    ) -> Result<Value> {
        let mut body = serde_json::Map::new();
        body.insert(
            "firebase_id_token".to_string(),
            Value::String(firebase_id_token.to_string()),
        );
        if let Some(slug) = opts.tenant_slug {
            body.insert("tenant_slug".to_string(), Value::String(slug));
        }
        if let Some(tok) = opts.invite_token {
            body.insert("invite_token".to_string(), Value::String(tok));
        }
        self.http
            .post("/auth/firebase/exchange", &Value::Object(body))
            .await
    }

    /// Links a Firebase UID to the currently-authenticated Olympus identity.
    ///
    /// Idempotent: re-linking the same `(firebase_uid, caller)` returns the
    /// ORIGINAL `linked_at` timestamp, not "now".
    ///
    /// Returns [`crate::error::OlympusError::FirebaseUidAlreadyLinked`]
    /// (409) when the UID is already bound to a different Olympus user in
    /// the caller's tenant.
    pub async fn link_firebase(
        &self,
        firebase_id_token: &str,
    ) -> Result<FirebaseLinkResult> {
        let body = json!({ "firebase_id_token": firebase_id_token });
        let raw = self.http.post("/auth/firebase/link", &body).await?;
        serde_json::from_value(raw).map_err(crate::error::OlympusError::Json)
    }
}
