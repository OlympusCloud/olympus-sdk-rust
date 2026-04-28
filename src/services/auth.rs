use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Authentication and user management service.
///
/// Wraps the Olympus Auth service (Rust, port 8001) via the Go API Gateway.
/// Routes: `/auth/*`, `/platform/users/*`.
pub struct AuthService {
    http: Arc<OlympusHttpClient>,
}

/// Request body for [`AuthService::assign_roles`].
///
/// Serialized as the canonical V1 app-scoped permissions wire shape (gcp#3653 /
/// Epic #3234). The server normalizes both arrays via dedupe + lex-sort before
/// writing to the `platform_app_tenant_grants` ledger — this struct deliberately
/// does NOT pre-normalize. Pass scopes in any order, including duplicates; the
/// server is the single source of truth for normalization.
#[derive(Debug, Clone, Serialize)]
pub struct AssignRolesRequest {
    /// Tenant on whose behalf the grant/revoke is recorded. Must match the
    /// caller's JWT `tenant_id` unless the caller is `platform_admin`
    /// (cross-tenant write).
    pub tenant_id: String,

    /// Target user receiving / losing the scopes. Used to fill the `{user_id}`
    /// path segment — NOT serialized into the JSON body.
    #[serde(skip)]
    pub user_id: String,

    /// Scopes to grant. Empty list serializes as `[]` (NOT `null`) — the server
    /// requires the field to be present.
    pub grant_scopes: Vec<String>,

    /// Scopes to soft-delete in the ledger. Empty list serializes as `[]`.
    pub revoke_scopes: Vec<String>,

    /// Optional human-readable rationale (reserved for future audit-event
    /// payload enrichment; not persisted in V1). Omitted from the body when
    /// `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl AssignRolesRequest {
    /// Build a new request. `grant_scopes` / `revoke_scopes` default to empty
    /// vecs and `note` defaults to `None`; mutate the fields directly or use
    /// [`Self::with_grants`] / [`Self::with_revokes`] / [`Self::with_note`].
    pub fn new(tenant_id: impl Into<String>, user_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            user_id: user_id.into(),
            grant_scopes: Vec::new(),
            revoke_scopes: Vec::new(),
            note: None,
        }
    }

    /// Builder helper: set the scopes to grant.
    pub fn with_grants(mut self, grants: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.grant_scopes = grants.into_iter().map(Into::into).collect();
        self
    }

    /// Builder helper: set the scopes to revoke.
    pub fn with_revokes(mut self, revokes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.revoke_scopes = revokes.into_iter().map(Into::into).collect();
        self
    }

    /// Builder helper: set the optional note.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
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

    /// Grant and/or revoke app-scoped permissions on a user (V1 contract).
    ///
    /// Wraps `POST /platform/users/{user_id}/roles/assign` (gcp#3653 / Epic
    /// #3234). Both `grant_scopes` and `revoke_scopes` are optional but at
    /// least one of them should be non-empty for the call to do useful work.
    /// Scope strings follow the canonical `<resource>.<action>@<holder>`
    /// shape (e.g. `platform.user.read@tenant`); the server validates each
    /// entry and returns `400 ROLES_VALIDATION_ERROR` if any string is
    /// malformed.
    ///
    /// The server normalizes both arrays via dedupe + lex-sort before writing
    /// to the `platform_app_tenant_grants` ledger, so callers may pass the
    /// same scope twice or in any order without producing duplicate audit
    /// rows. We deliberately do NOT pre-normalize on the client — the wire
    /// contract is the server's normalization, and client-side dedup would
    /// mask correctness regressions if the contract ever loosens.
    ///
    /// # Errors
    ///
    /// Returns [`crate::OlympusError::Api`] with `code` set to one of:
    ///
    /// * `ROLES_VALIDATION_ERROR` (400) — bad/missing fields, malformed scope
    ///   strings, or grant/revoke overlap.
    /// * `INSUFFICIENT_PERMISSIONS` (403) — caller lacks `tenant_admin` role
    ///   on `tenant_id`.
    /// * `USER_NOT_FOUND` (404) — target user does not exist in this tenant.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use olympus_sdk::OlympusClient;
    /// use olympus_sdk::services::auth::AssignRolesRequest;
    ///
    /// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = OlympusClient::new("com.my-app", "oc_live_...");
    /// client
    ///     .auth()
    ///     .assign_roles(
    ///         AssignRolesRequest::new("tenant-123", "user-456")
    ///             .with_grants(["platform.user.read@tenant"])
    ///             .with_note("onboarding sweep"),
    ///     )
    ///     .await?;
    /// # Ok(()) }
    /// ```
    pub async fn assign_roles(&self, req: AssignRolesRequest) -> Result<()> {
        let path = format!("/platform/users/{}/roles/assign", req.user_id);
        let body = serde_json::to_value(&req)?;
        self.http.post(&path, &body).await?;
        Ok(())
    }

    /// Revoke app-scoped permissions on a user.
    ///
    /// Convenience wrapper that calls [`Self::assign_roles`] with empty
    /// `grant_scopes` and the supplied `scopes` in `revoke_scopes`. The
    /// underlying V1 backend contract (gcp#3653) ships a single
    /// `/roles/assign` endpoint that handles both grant and revoke through
    /// the request body shape — there is no separate `/roles/revoke` path.
    ///
    /// `scopes` must be non-empty for the call to be meaningful (an empty
    /// slice round-trips to `ROLES_VALIDATION_ERROR` from the server).
    /// `note` is forwarded as the optional rationale.
    pub async fn revoke_roles(
        &self,
        tenant_id: impl Into<String>,
        user_id: impl Into<String>,
        scopes: impl IntoIterator<Item = impl Into<String>>,
        note: Option<String>,
    ) -> Result<()> {
        let mut req = AssignRolesRequest::new(tenant_id, user_id).with_revokes(scopes);
        if let Some(n) = note {
            req = req.with_note(n);
        }
        self.assign_roles(req).await
    }

    // -----------------------------------------------------------------------
    // App-Scoped Permissions — Wave 14c (#3781, #3788)
    // -----------------------------------------------------------------------

    /// Mint a fresh App JWT pair from a Firebase App Check attestation.
    ///
    /// Wraps `POST /auth/app-tokens/mint` (#3781 / #3238). The endpoint is
    /// attestation-only — the App Check signature IS the proof of origin —
    /// so no `Authorization` header is required. The handler chain on the
    /// server is App Check verify → per-install rate limit → JTI replay
    /// check → `developer_apps` lookup → scope catalog freshness gate →
    /// mint pair → record refresh family.
    ///
    /// Pass `firebase_install_id` whenever it is available
    /// (`firebase_installations.getId()`); the server uses it for a
    /// per-install 60 mints/hour bucket. Without it only the IP-level
    /// rate limit applies.
    ///
    /// # Errors
    ///
    /// * `app_check_invalid` (401) — App Check verification failed.
    /// * `replay_detected` (400) — JTI was already presented inside the
    ///   replay window.
    /// * `app_not_registered` (404) — verified App Check but no
    ///   `developer_apps` row matches the resolved Firebase app id.
    /// * `rate_limited` (429) — per-install bucket exhausted; honor
    ///   `Retry-After` from the gateway envelope.
    /// * `scope_catalog_stale` (503) — auth service has not yet refreshed
    ///   from Spanner. Retry after a short backoff.
    pub async fn mint_app_token(&self, req: MintAppTokenRequest) -> Result<AppTokenResponse> {
        let body = serde_json::to_value(&req)?;
        let value = self.http.post("/auth/app-tokens/mint", &body).await?;
        Ok(serde_json::from_value(value)?)
    }

    /// Rotate an App JWT pair using a single-use refresh token.
    ///
    /// Wraps `POST /auth/app-tokens/refresh` (#3781 / #3238). Refresh
    /// tokens rotate on every successful exchange — re-using a token
    /// detects the rotation reuse and burns the entire refresh family for
    /// that session. Surfaced as `refresh_reuse_detected` (401).
    ///
    /// Server re-bitsets scopes against the live catalog on rotation, so
    /// scopes revoked between the original mint and the rotation are NOT
    /// preserved.
    pub async fn refresh_app_token(&self, req: RefreshAppTokenRequest) -> Result<AppTokenResponse> {
        let body = serde_json::to_value(&req)?;
        let value = self.http.post("/auth/app-tokens/refresh", &body).await?;
        Ok(serde_json::from_value(value)?)
    }

    /// Fetch the public JWKS for an Olympus app's signing keys.
    ///
    /// Wraps `GET /.well-known/app-keys/{app_id}` (#3788 / #3239). Public
    /// endpoint — no auth required. Returns every non-retired EdDSA key
    /// plus retired keys still inside the 7-day overlap window so JWT
    /// verifiers can keep validating tokens issued just before a
    /// rotation. Conforms to RFC 8037 (`kty=OKP, crv=Ed25519`).
    ///
    /// The server emits `Cache-Control: public, max-age=300,
    /// stale-while-revalidate=60`; SDK consumers SHOULD cache this
    /// response for that window rather than re-fetching on every JWT
    /// validation.
    pub async fn get_app_jwks(&self, app_id: &str) -> Result<AppJwksResponse> {
        let path = format!("/.well-known/app-keys/{}", app_id);
        let value = self.http.get(&path).await?;
        Ok(serde_json::from_value(value)?)
    }
}

// ---------------------------------------------------------------------------
// App-Scoped Permissions request / response types — Wave 14c
// ---------------------------------------------------------------------------

/// Request body for [`AuthService::mint_app_token`].
///
/// Mirrors `MintAppTokenRequest` in
/// `backend/rust/auth/src/handlers/app_tokens.rs` — keep in sync.
#[derive(Debug, Clone, Serialize)]
pub struct MintAppTokenRequest {
    /// Firebase App Check JWT — server verifies the signature against the
    /// live JWKS plus `aud=projects/<n>` per #3238.
    pub app_check_token: String,
    /// Optional Firebase install id. When present, drives the per-install
    /// 60 mints/hour bucket. Most clients pass this from
    /// `firebase_installations.getId()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firebase_install_id: Option<String>,
}

impl MintAppTokenRequest {
    /// Construct a mint request with only the App Check token.
    pub fn new(app_check_token: impl Into<String>) -> Self {
        Self {
            app_check_token: app_check_token.into(),
            firebase_install_id: None,
        }
    }

    /// Builder helper: attach a Firebase install id to bind the per-install
    /// rate-limit bucket.
    pub fn with_firebase_install_id(mut self, id: impl Into<String>) -> Self {
        self.firebase_install_id = Some(id.into());
        self
    }
}

/// Request body for [`AuthService::refresh_app_token`].
#[derive(Debug, Clone, Serialize)]
pub struct RefreshAppTokenRequest {
    /// Olympus refresh token previously returned from
    /// [`AuthService::mint_app_token`] or a prior refresh. Single-use:
    /// the server rotates on every success and replay burns the family.
    pub refresh_token: String,
}

impl RefreshAppTokenRequest {
    /// Build a refresh request from a previously-issued refresh token.
    pub fn new(refresh_token: impl Into<String>) -> Self {
        Self {
            refresh_token: refresh_token.into(),
        }
    }
}

/// Response from [`AuthService::mint_app_token`] and
/// [`AuthService::refresh_app_token`].
///
/// Mirrors `AppTokenResponse` in
/// `backend/rust/auth/src/handlers/app_tokens.rs`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppTokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    /// Access-token TTL in seconds. App tokens are short — 15 minutes per
    /// the canonical config (`JwtService::default_app_access_ttl`).
    pub expires_in: i64,
    /// Always `"App-JWT"` for tokens minted from these endpoints. Lets
    /// the SDK switch on token kind without decoding the JWT.
    pub token_type: String,
}

/// Single JWK row in [`AppJwksResponse`]. Mirrors `AppJwk` in
/// `backend/rust/auth/src/services/signing_keys.rs` (RFC 8037 OKP/Ed25519).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppJwk {
    /// Always `"OKP"` for Ed25519 per RFC 8037.
    pub kty: String,
    /// Always `"Ed25519"`.
    pub crv: String,
    /// Public key bytes, base64url-no-pad.
    pub x: String,
    /// Always `"EdDSA"`.
    pub alg: String,
    /// Always `"sig"` — these keys are NEVER usable for encryption.
    #[serde(rename = "use")]
    pub use_: String,
    /// Stable key identifier matching the JWT `kid` header.
    pub kid: String,
    /// Optional non-standard `status` extension emitted by the auth
    /// service (`active`, `retired_overlap`). Verifiers SHOULD ignore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Top-level JWKS response from
/// [`AuthService::get_app_jwks`]. Mirrors `AppJwksResponse` in
/// `backend/rust/auth/src/services/signing_keys.rs`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppJwksResponse {
    pub keys: Vec<AppJwk>,
}
