use std::collections::BTreeSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{OlympusError, Result};
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

    /// Assigns or revokes scopes for `user_id` within `tenant_id`.
    ///
    /// W12-1 / olympus-cloud-gcp#3599. Mirrors the canonical Dart contract
    /// shipped in olympus-sdk-dart#45. Server-side: writes the scope mask,
    /// fires the FCM topic the platform-side IntentBus broker subscribes
    /// to so every open app on the target user's device sees an
    /// `identity.scopes.granted` / `identity.scopes.revoked`
    /// `CrossAppIntent`.
    ///
    /// Errors (all surfaced as [`OlympusError::Api`]):
    /// - 400 `ROLES_VALIDATION_ERROR` — empty grant + revoke sets, or
    ///   unknown scope
    /// - 403 `INSUFFICIENT_PERMISSIONS` — caller lacks
    ///   `platform.founder.roles.assign@tenant`
    /// - 404 `USER_NOT_FOUND` — `user_id` is not a member of `tenant_id`
    pub async fn assign_roles(&self, req: AssignRolesRequest<'_>) -> Result<()> {
        // Dedupe + sort for deterministic JSON output. BTreeSet gives both.
        let grant_scopes: BTreeSet<&str> = req.grant_scopes.iter().copied().collect();
        let revoke_scopes: BTreeSet<&str> = req.revoke_scopes.iter().copied().collect();
        let mut body = serde_json::Map::new();
        body.insert(
            "tenant_id".to_string(),
            Value::String(req.tenant_id.to_string()),
        );
        body.insert(
            "grant_scopes".to_string(),
            Value::Array(
                grant_scopes
                    .iter()
                    .map(|s| Value::String((*s).to_string()))
                    .collect(),
            ),
        );
        body.insert(
            "revoke_scopes".to_string(),
            Value::Array(
                revoke_scopes
                    .iter()
                    .map(|s| Value::String((*s).to_string()))
                    .collect(),
            ),
        );
        if let Some(note) = req.note {
            body.insert("note".to_string(), Value::String(note.to_string()));
        }
        let path = format!("/platform/users/{}/roles/assign", req.user_id);
        let _ = self.http.post(&path, &Value::Object(body)).await?;
        Ok(())
    }

    /// Lists teammates the caller can manage.
    ///
    /// Server-side filters by caller's `platform.founder.roles.assign` scope.
    /// Mirrors the canonical Dart contract shipped in
    /// olympus-sdk-dart#45 (W12-1 / olympus-cloud-gcp#3599).
    ///
    /// Tolerates both `{"data": [...]}` envelope and bare-array responses.
    pub async fn list_teammates(
        &self,
        tenant_id: Option<&str>,
    ) -> Result<Vec<OlympusTeammate>> {
        let value = if let Some(tid) = tenant_id {
            self.http
                .get_with_query("/platform/teammates", &[("tenant_id", tid)])
                .await?
        } else {
            self.http.get("/platform/teammates").await?
        };
        let rows: Vec<Value> = match value {
            Value::Array(arr) => arr,
            Value::Object(obj) => obj
                .get("data")
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let teammate: OlympusTeammate =
                serde_json::from_value(row).map_err(OlympusError::Json)?;
            out.push(teammate);
        }
        Ok(out)
    }
}

/// Input to [`AuthService::assign_roles`].
///
/// Mirrors the canonical Dart contract from olympus-sdk-dart#45 (W12-1 /
/// olympus-cloud-gcp#3599). Borrowed string slices keep the call site
/// allocation-free; pass empty slices for "no grants" or "no revokes".
#[derive(Debug, Clone, Default)]
pub struct AssignRolesRequest<'a> {
    pub user_id: &'a str,
    pub tenant_id: &'a str,
    pub grant_scopes: &'a [&'a str],
    pub revoke_scopes: &'a [&'a str],
    pub note: Option<&'a str>,
}

/// A teammate listed by the Auth/Platform service.
///
/// Returned by [`AuthService::list_teammates`]. Mirrors the canonical Dart
/// contract shipped in olympus-sdk-dart#45 (W12-1 / olympus-cloud-gcp#3599).
/// `assigned_scopes` is a [`BTreeSet`] so callers can do membership checks
/// in O(log n) and the wire payload is sorted.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OlympusTeammate {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub assigned_scopes: BTreeSet<String>,
}
