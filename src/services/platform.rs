use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::Result;
use crate::http::{FormResponse, OlympusHttpClient};

/// Tenant lifecycle service for signup, cleanup, app onboarding, and the
/// App-Scoped Permissions consent / PKCE flows.
///
/// Wraps the Olympus Platform service (Rust, port 8002) via the Go API Gateway.
/// Routes: `/platform/*`.
pub struct PlatformService {
    http: Arc<OlympusHttpClient>,
}

impl PlatformService {
    /// Creates a new PlatformService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Executes the automated tenant signup workflow.
    pub async fn signup(
        &self,
        company_name: &str,
        admin_email: &str,
        admin_name: &str,
        industry: &str,
    ) -> Result<Value> {
        let body = json!({
            "company_name": company_name,
            "admin_email": admin_email,
            "admin_name": admin_name,
            "industry": industry,
            "trial_days": 14,
        });
        self.http.post("/platform/signup", &body).await
    }

    /// Executes the automated tenant cleanup/offboarding workflow.
    pub async fn cleanup(&self, tenant_id: &str, reason: &str, export_data: bool) -> Result<Value> {
        let body = json!({
            "tenant_id": tenant_id,
            "reason": reason,
            "export_data": export_data,
            "grace_period_days": 30,
        });
        self.http.post("/platform/cleanup", &body).await
    }

    // -----------------------------------------------------------------------
    // App-Scoped Permissions — Wave 14c (#3810, #3804, #3808, #3806)
    // -----------------------------------------------------------------------

    /// Run the end-to-end app onboarding ceremony.
    ///
    /// Wraps `POST /platform/apps/onboard` (#3810 / #3281). Validates the
    /// supplied manifest against the server-side `manifest_validator`
    /// (#3249), upserts the `developer_apps` row, issues a fresh `osk_*`
    /// API key (Argon2id-hashed server-side; plaintext returned ONCE),
    /// and publishes `platform.app.onboarded` so the auth service seeds
    /// the EdDSA signing-key pair on its next rotate sweep.
    ///
    /// Idempotent on `app_id` — re-onboarding the same app returns
    /// [`OnboardStatus::AlreadyOnboarded`] and a fresh API key WITHOUT
    /// re-writing the manifest row.
    ///
    /// Requires `platform_admin` on the caller's JWT — internal apps and
    /// verified-partner provisioning go through this endpoint; self-serve
    /// developer-portal flows use a separate human-review pipeline.
    ///
    /// # Errors
    ///
    /// * 422 — manifest validation failure (returns the full error list
    ///   under `OlympusError::Api { message, .. }`).
    /// * 403 — caller lacks `platform_admin`.
    pub async fn onboard_app(&self, req: OnboardRequest) -> Result<OnboardResponse> {
        let body = serde_json::to_value(&req)?;
        let value = self.http.post("/platform/apps/onboard", &body).await?;
        Ok(serde_json::from_value(value)?)
    }

    /// Submit a consent decision against the App-Scoped Permissions
    /// consent screen.
    ///
    /// Wraps `POST /platform/authorize/consent` (#3804 / #3291). The
    /// endpoint accepts a form-encoded body and responds `303 See Other`
    /// to `return_to?grant_id=<new_grant_id>&state=<echoed_csrf>` on
    /// authorize, or `?error=user_cancelled&state=...` on cancel. The SDK
    /// captures the `Location` and surfaces it in [`ConsentResult::location`]
    /// rather than following the deep-link redirect.
    ///
    /// **Browser-flow caveat**: this is a browser endpoint that pairs with
    /// `GET /platform/authorize` to render an HTML consent screen. A
    /// programmatic SDK call must:
    /// 1. Carry an authenticated `Authorization: Bearer <user JWT>` (or
    ///    `olympus_session` cookie) — the consent write is on behalf of
    ///    the consenting user.
    /// 2. Bootstrap the CSRF double-submit cookie (`olympus_authz_csrf`)
    ///    via a prior `GET /platform/authorize`. Without it the server
    ///    rejects with `403 csrf_check_failed`.
    ///
    /// Most callers should drive the user through the rendered HTML flow
    /// instead. This SDK method exists for admin tooling that already
    /// owns both cookies.
    pub async fn submit_consent(&self, form: ConsentForm) -> Result<ConsentResult> {
        let resp = self
            .http
            .post_form_no_redirect("/platform/authorize/consent", &form)
            .await?;
        Ok(ConsentResult::from_form_response(resp))
    }

    /// Submit the PKCE-required "grant" form.
    ///
    /// Wraps `POST /platform/authorize/grant` (#3808 / #3243). Identical
    /// shape to `/consent` but `code_challenge` + `code_challenge_method`
    /// are MANDATORY (server returns 400 `pkce_required` otherwise) and
    /// the redirect carries `?code=<auth_code>&state=...` instead of
    /// `?grant_id=`. The 5-min auth code is then traded for a mint
    /// ticket via [`Self::exchange_authorization_code`].
    ///
    /// Same browser-flow caveat as [`Self::submit_consent`] — see that
    /// method's docstring for the cookie / session bootstrap requirements.
    pub async fn submit_grant(&self, form: GrantForm) -> Result<GrantResult> {
        let resp = self
            .http
            .post_form_no_redirect("/platform/authorize/grant", &form)
            .await?;
        Ok(GrantResult::from_form_response(resp))
    }

    /// Exchange a PKCE auth code for a `mint_ticket` payload.
    ///
    /// Wraps `POST /platform/authorize/exchange` (#3808 / #3243). The
    /// endpoint is JSON in / JSON out. The returned `mint_ticket` is
    /// forwarded to `POST /auth/app-tokens/mint` to obtain real App
    /// JWTs — this split keeps the platform service out of the JWT
    /// signing business.
    ///
    /// # Errors
    ///
    /// * `code_invalid` (400) — code not found, expired, or already
    ///   consumed.
    /// * `pkce_mismatch` (400) — `code_verifier` does not hash to the
    ///   stored `code_challenge`.
    /// * `app_id_mismatch` (400) — caller `app_id` differs from the one
    ///   recorded with the auth code (cross-app exchange is rejected).
    pub async fn exchange_authorization_code(
        &self,
        req: ExchangeRequest,
    ) -> Result<ExchangeResponse> {
        let body = serde_json::to_value(&req)?;
        let value = self
            .http
            .post("/platform/authorize/exchange", &body)
            .await?;
        Ok(serde_json::from_value(value)?)
    }

    /// Fetch the read-only grant graph projection of
    /// `platform_app_tenant_grants`.
    ///
    /// Wraps `GET /platform/admin/grants/graph` (#3806 / #3250). Returns
    /// the full projection plus an HMAC-SHA256 signature over the
    /// canonical JSON so reviewers can re-sign deterministically. Per
    /// the design doc (§0.4 E.1) the graph is **never** consulted for
    /// enforcement — `platform_app_tenant_grants` is the authoritative
    /// flat-table source for scope checks. This endpoint is for
    /// compliance / audit traversals only.
    ///
    /// Requires `platform_admin` on the caller's JWT. Optional filters
    /// on `tenant_id` and `app_id`; `limit` defaults to 5000 server-side.
    pub async fn get_grants_graph(&self, query: GrantsGraphQuery) -> Result<GraphResponse> {
        let mut pairs: Vec<(&str, String)> = Vec::new();
        if let Some(t) = &query.tenant_id {
            pairs.push(("tenant_id", t.clone()));
        }
        if let Some(a) = &query.app_id {
            pairs.push(("app_id", a.clone()));
        }
        if let Some(l) = query.limit {
            pairs.push(("limit", l.to_string()));
        }
        let borrowed: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let value = self
            .http
            .get_with_query("/platform/admin/grants/graph", &borrowed)
            .await?;
        Ok(serde_json::from_value(value)?)
    }
}

// ---------------------------------------------------------------------------
// App-Scoped Permissions — request / response types — Wave 14c
// ---------------------------------------------------------------------------

// ---- /platform/apps/onboard (#3810) ---------------------------------------

/// Request body for [`PlatformService::onboard_app`]. Mirrors
/// `OnboardRequest` in
/// `backend/rust/platform/src/handlers/apps_onboard.rs`.
#[derive(Debug, Clone, Serialize)]
pub struct OnboardRequest {
    /// Full manifest JSON. The server-side validator runs against this
    /// verbatim — unknown fields are tolerated, the validator subset is
    /// enforced.
    pub manifest: Value,
    /// Optional initial CORS origins (e.g. `https://app.pizzaos.ai`).
    /// Returned in `cors_origins_pending` if the runtime CORS table is
    /// not yet ready to receive per-app origins.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub cors_origins: Vec<String>,
    /// Friendly label for the issued initial API key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_label: Option<String>,
}

impl OnboardRequest {
    /// Build an onboard request from a manifest JSON value.
    pub fn new(manifest: Value) -> Self {
        Self {
            manifest,
            cors_origins: Vec::new(),
            api_key_label: None,
        }
    }

    /// Builder helper: set the optional CORS origins.
    pub fn with_cors_origins(
        mut self,
        origins: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.cors_origins = origins.into_iter().map(Into::into).collect();
        self
    }

    /// Builder helper: set the optional API-key label.
    pub fn with_api_key_label(mut self, label: impl Into<String>) -> Self {
        self.api_key_label = Some(label.into());
        self
    }
}

/// Response from [`PlatformService::onboard_app`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OnboardResponse {
    pub app_id: String,
    pub status: OnboardStatus,
    /// Plaintext API key — returned ONCE. Subsequent rotates require a
    /// new issue ceremony.
    pub api_key: String,
    pub api_key_prefix: String,
    pub api_key_id: String,
    /// Origins that could not be persisted into the runtime CORS table
    /// yet. Empty when fully persisted.
    #[serde(default)]
    pub cors_origins_pending: Vec<String>,
    #[serde(default)]
    pub cors_followup_issue_url: Option<String>,
    /// `false` when the publisher is in emulator/no-op mode.
    pub signing_key_seed_event_published: bool,
    #[serde(default)]
    pub validator_warnings: Vec<Value>,
}

/// Onboarding terminal status. Mirrors `OnboardStatus` server-side.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OnboardStatus {
    /// First-time onboarding — manifest written, key issued, event
    /// published.
    Onboarded,
    /// Idempotent re-onboard. The existing manifest row was kept; a
    /// fresh API key was still issued.
    AlreadyOnboarded,
}

// ---- /platform/authorize/consent (#3804) -----------------------------------

/// Form body for [`PlatformService::submit_consent`]. Mirrors
/// `ConsentForm` in
/// `backend/rust/platform/src/handlers/authorize.rs`.
#[derive(Debug, Clone, Serialize)]
pub struct ConsentForm {
    pub app_id: String,
    /// Comma-separated list of scopes the user is consenting to.
    pub scopes: String,
    pub return_to: String,
    /// CSRF state — echoed back in the redirect URL.
    pub state: String,
    /// UUID v4 — pinned to the `olympus_authz_csrf` cookie.
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_challenge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_challenge_method: Option<String>,
    /// `"authorize"` or `"cancel"`.
    pub action: String,
    /// Tickbox confirming destructive scopes — required when any
    /// requested scope is destructive in `platform_scopes`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_ack: Option<String>,
}

/// Result of a [`PlatformService::submit_consent`] call. Captures the
/// 303 redirect surface so callers can pick out `grant_id` / `state` /
/// `error`.
#[derive(Debug, Clone)]
pub struct ConsentResult {
    pub status: u16,
    /// Full `Location` header value (the `return_to` URL with appended
    /// query parameters).
    pub location: Option<String>,
    /// `grant_id` parsed out of the `Location` query string, if present.
    pub grant_id: Option<String>,
    /// `state` echoed back in the `Location` query string.
    pub state: Option<String>,
    /// `error` — set to e.g. `"user_cancelled"` on the cancel branch.
    pub error: Option<String>,
}

impl ConsentResult {
    fn from_form_response(resp: FormResponse) -> Self {
        let (grant_id, state, error) = match resp.location.as_deref() {
            Some(loc) => {
                let g = extract_query_param(loc, "grant_id");
                let s = extract_query_param(loc, "state");
                let e = extract_query_param(loc, "error");
                (g, s, e)
            }
            None => (None, None, None),
        };
        Self {
            status: resp.status,
            location: resp.location,
            grant_id,
            state,
            error,
        }
    }
}

// ---- /platform/authorize/grant + /exchange (#3808) -------------------------

/// Form body for [`PlatformService::submit_grant`]. Mirrors
/// `GrantForm` in
/// `backend/rust/platform/src/handlers/authorize_oauth.rs`.
///
/// PKCE is REQUIRED — both `code_challenge` and `code_challenge_method`
/// (`"S256"`) must be non-empty.
#[derive(Debug, Clone, Serialize)]
pub struct GrantForm {
    pub app_id: String,
    /// Comma-separated scopes.
    pub scopes: String,
    pub return_to: String,
    pub state: String,
    pub request_id: String,
    pub code_challenge: String,
    /// Always `"S256"` for v1 — the server rejects anything else.
    pub code_challenge_method: String,
    /// `"authorize"` or `"cancel"`.
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_ack: Option<String>,
}

/// Result of [`PlatformService::submit_grant`]. The 303 redirect carries
/// `?code=<auth_code>&state=<echoed>` on success, or
/// `?error=...&state=...` on cancel.
#[derive(Debug, Clone)]
pub struct GrantResult {
    pub status: u16,
    pub location: Option<String>,
    /// PKCE auth code (5-min TTL) — feed to
    /// [`PlatformService::exchange_authorization_code`].
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

impl GrantResult {
    fn from_form_response(resp: FormResponse) -> Self {
        let (code, state, error) = match resp.location.as_deref() {
            Some(loc) => {
                let c = extract_query_param(loc, "code");
                let s = extract_query_param(loc, "state");
                let e = extract_query_param(loc, "error");
                (c, s, e)
            }
            None => (None, None, None),
        };
        Self {
            status: resp.status,
            location: resp.location,
            code,
            state,
            error,
        }
    }
}

/// Request body for [`PlatformService::exchange_authorization_code`].
/// Mirrors `ExchangeRequest` in
/// `backend/rust/platform/src/handlers/authorize_oauth.rs`.
#[derive(Debug, Clone, Serialize)]
pub struct ExchangeRequest {
    /// Auth code returned in the `?code=` redirect from
    /// [`PlatformService::submit_grant`].
    pub code: String,
    /// PKCE pre-image of the original `code_challenge` (the verifier the
    /// client generated alongside the challenge).
    pub code_verifier: String,
    /// MUST equal the `app_id` recorded with the auth code.
    pub app_id: String,
}

impl ExchangeRequest {
    /// Build an exchange request.
    pub fn new(
        code: impl Into<String>,
        code_verifier: impl Into<String>,
        app_id: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            code_verifier: code_verifier.into(),
            app_id: app_id.into(),
        }
    }
}

/// Response from [`PlatformService::exchange_authorization_code`].
/// Mirrors `ExchangeResponse` in
/// `backend/rust/platform/src/handlers/authorize_oauth.rs`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExchangeResponse {
    pub mint_ticket: MintTicket,
    /// Seconds until [`MintTicket::exp`] expires. Clients SHOULD
    /// immediately POST the ticket to `/auth/app-tokens/mint`.
    pub expires_in: i64,
}

/// Mint-ticket envelope returned alongside [`ExchangeResponse`]. Mirrors
/// `MintTicket` in
/// `backend/rust/platform/src/handlers/authorize_oauth.rs`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MintTicket {
    pub app_id: String,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub granted_scopes: Vec<String>,
    /// Unix seconds.
    pub issued_at: i64,
    /// Unix seconds — typically `issued_at + 300`.
    pub exp: i64,
    /// 32-byte base64url nonce — replay protection on the auth-side
    /// mint endpoint.
    pub nonce: String,
}

// ---- /platform/admin/grants/graph (#3806) ----------------------------------

/// Optional filters for [`PlatformService::get_grants_graph`]. Mirrors
/// `GraphQuery` in
/// `backend/rust/platform/src/handlers/grant_graph_projection.rs`.
#[derive(Debug, Clone, Default)]
pub struct GrantsGraphQuery {
    pub tenant_id: Option<String>,
    pub app_id: Option<String>,
    /// Defensive cap on edge counts. Defaults to 5000 server-side.
    pub limit: Option<usize>,
}

impl GrantsGraphQuery {
    /// Empty query — returns the unfiltered graph projection.
    pub fn all() -> Self {
        Self::default()
    }

    /// Filter the projection to a single tenant.
    pub fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Filter the projection to a single app.
    pub fn with_app_id(mut self, app_id: impl Into<String>) -> Self {
        self.app_id = Some(app_id.into());
        self
    }

    /// Override the default 5000-edge cap.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// Tenant node in [`GrantGraphSnapshot`].
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TenantNode {
    pub tenant_id: String,
}

/// App node in [`GrantGraphSnapshot`].
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppNode {
    pub app_id: String,
}

/// `(Tenant)-[GRANTED]->(App)` projection edge. Mirrors `GrantedEdge`
/// server-side.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GrantedEdge {
    pub tenant_id: String,
    pub app_id: String,
    /// Empty for direct grants; non-empty = cross-app grant where the
    /// data holder is `source_app_id`.
    pub source_app_id: String,
    pub active_scope_count: u64,
    pub last_granted_at: DateTime<Utc>,
    pub first_granted_at: DateTime<Utc>,
    /// `active_scope_count + recency_score` ∈ `[count, count + 1)`.
    pub weight: f64,
}

/// `(App)-[REQUIRES]->(Scope)` projection edge.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequiresEdge {
    pub app_id: String,
    pub scope: String,
    pub is_destructive: bool,
    pub granted_by_tenants: u64,
}

/// Captured snapshot of the projection. Mirrors `GrantGraphSnapshot`
/// server-side.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GrantGraphSnapshot {
    pub captured_at: DateTime<Utc>,
    pub tenants: Vec<TenantNode>,
    pub apps: Vec<AppNode>,
    pub granted_edges: Vec<GrantedEdge>,
    pub requires_edges: Vec<RequiresEdge>,
    pub total_active_grants: u64,
}

/// Response from [`PlatformService::get_grants_graph`]. Carries the
/// HMAC-SHA256 signature plus the canonical JSON used for signing so
/// reviewers can re-sign deterministically.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GraphResponse {
    pub snapshot: GrantGraphSnapshot,
    pub signature_hex: String,
    pub canonical_json: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Pull a query-string parameter value out of a URL. Returns `None` when
/// the URL has no query, the key is missing, or the URL is malformed.
fn extract_query_param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?').map(|(_, q)| q)?;
    let stripped = query.split_once('#').map(|(q, _)| q).unwrap_or(query);
    for pair in stripped.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k == key {
            return Some(percent_decode(v));
        }
    }
    None
}

/// Minimal percent-decoder for query-string values. Handles `+` → space
/// and `%XX` byte escapes; falls back to the raw input on malformed
/// escapes. We avoid a full URL crate dependency for this single helper.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = &input[i + 1..i + 3];
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_query_param_finds_value() {
        let url = "pizzaos://settings?grant_id=abc-123&state=xyz";
        assert_eq!(
            extract_query_param(url, "grant_id"),
            Some("abc-123".to_string())
        );
        assert_eq!(extract_query_param(url, "state"), Some("xyz".to_string()));
        assert_eq!(extract_query_param(url, "missing"), None);
    }

    #[test]
    fn extract_query_param_handles_missing_query() {
        assert_eq!(extract_query_param("pizzaos://settings", "code"), None);
    }

    #[test]
    fn extract_query_param_strips_fragment() {
        let url = "https://app.example.com/cb?code=abc#fragment-here";
        assert_eq!(extract_query_param(url, "code"), Some("abc".to_string()));
    }

    #[test]
    fn percent_decode_handles_plus_and_pct() {
        assert_eq!(percent_decode("hello+world"), "hello world");
        assert_eq!(percent_decode("a%20b%2Fc"), "a b/c");
        assert_eq!(percent_decode("plain"), "plain");
        // Malformed escape — falls back to raw byte.
        assert_eq!(percent_decode("a%2"), "a%2");
    }

    #[test]
    fn consent_result_parses_grant_id_and_state() {
        let resp = FormResponse {
            status: 303,
            location: Some(
                "pizzaos://settings/voice-agents?grant_id=g-42&state=csrf-abc".to_string(),
            ),
            body: String::new(),
        };
        let result = ConsentResult::from_form_response(resp);
        assert_eq!(result.grant_id.as_deref(), Some("g-42"));
        assert_eq!(result.state.as_deref(), Some("csrf-abc"));
        assert_eq!(result.error, None);
    }

    #[test]
    fn consent_result_parses_cancel_error() {
        let resp = FormResponse {
            status: 303,
            location: Some("pizzaos://cancel?error=user_cancelled&state=xyz".to_string()),
            body: String::new(),
        };
        let result = ConsentResult::from_form_response(resp);
        assert_eq!(result.error.as_deref(), Some("user_cancelled"));
        assert_eq!(result.grant_id, None);
    }

    #[test]
    fn grant_result_parses_code_and_state() {
        let resp = FormResponse {
            status: 303,
            location: Some("pizzaos://oauth-cb?code=auth-code-99&state=csrf-9".to_string()),
            body: String::new(),
        };
        let result = GrantResult::from_form_response(resp);
        assert_eq!(result.code.as_deref(), Some("auth-code-99"));
        assert_eq!(result.state.as_deref(), Some("csrf-9"));
    }
}
