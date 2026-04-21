//! TenantApi — canonical `/tenant/*` SDK surface (#3403 §4.2 + §4.4).
//!
//! Wraps the Olympus Platform service `tenant_lifecycle` handler (shipped
//! in PR #3410) exposed through the Go API Gateway. Apps use this to
//! self-service onboard (`create`), read/patch their current tenant
//! (`current`/`update`), retire/unretire the organization, list every
//! tenant the signed-in user has access to, and switch between them.
//!
//! # Route map
//!
//! | Method | Route             | SDK method      |
//! |--------|-------------------|-----------------|
//! | POST   | /tenant/create    | [`TenantApi::create`]       |
//! | GET    | /tenant/current   | [`TenantApi::current`]      |
//! | PATCH  | /tenant/current   | [`TenantApi::update`]       |
//! | POST   | /tenant/retire    | [`TenantApi::retire`]       |
//! | POST   | /tenant/unretire  | [`TenantApi::unretire`]     |
//! | GET    | /tenant/mine      | [`TenantApi::my_tenants`]   |
//! | POST   | /tenant/switch    | [`TenantApi::switch_tenant`] — chains auth service |
//!
//! # `switch_tenant` behavior (divergence from task spec, matches backend)
//!
//! The platform service's `POST /tenant/switch` returns a **redirect payload**
//! (`{target_tenant_id, auth_endpoint: "/auth/switch-tenant", instructions}`)
//! rather than minting directly — session signing lives exclusively in the
//! auth service. The SDK transparently chains the two calls so callers still
//! get an [`ExchangedSession`] back: validate access via platform, then POST
//! to `/auth/switch-tenant` using the caller's current bearer token. The SDK
//! also rotates the HTTP client's access/refresh tokens to the fresh pair
//! before returning, so the very next SDK call uses the switched session.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::client::OlympusClient;
use crate::error::{OlympusError, Result};

// ---------------------------------------------------------------------------
// Request / response shapes — mirror backend tenant_lifecycle handler exactly.
// ---------------------------------------------------------------------------

/// First-admin details supplied with [`TenantApi::create`].
///
/// `firebase_link`, when set, tells the backend to link the newly created
/// admin user to the caller's Firebase UID. Omit to create the admin row
/// with email only and link the Firebase identity later via
/// `POST /auth/firebase/link`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TenantFirstAdmin {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firebase_link: Option<String>,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
}

/// Payload for [`TenantApi::create`].
///
/// `idempotency_key` is **required** by the backend — retries within a 24h
/// window return the original [`TenantProvisionResult`] with
/// `idempotent: true`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TenantCreateRequest {
    pub brand_name: String,
    /// Globally unique, validated `[a-z0-9-]{3,63}`.
    pub slug: String,
    /// One of: `starter` | `pro` | `enterprise` | `demo`.
    pub plan: String,
    pub first_admin: TenantFirstAdmin,
    /// Apps to auto-install at create time. Skips the §3 consent ceremony
    /// for the newly-created tenant's first install.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub install_apps: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_id: Option<String>,
    /// Signup-funnel dedupe — the Firebase UID is the canonical choice.
    pub idempotency_key: String,
}

/// Patch payload for [`TenantApi::update`]. Omit any field to leave it
/// unchanged on the server.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TenantUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

/// Exchanged session tokens. Populated by [`TenantApi::switch_tenant`] after
/// the SDK chains `POST /auth/switch-tenant`. On [`TenantApi::create`] the
/// three fields are [`None`] — callers follow up with
/// `POST /auth/firebase/exchange` to mint a session.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExchangedSession {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// RFC3339 UTC timestamp. Kept as a `String` to avoid pulling `chrono`
    /// into the SDK's public surface (consumers can parse at their leisure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_expires_at: Option<String>,
}

/// One entry in [`TenantProvisionResult::installed_apps`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInstall {
    pub app_id: String,
    pub status: String,
    pub installed_at: String,
}

/// Response to [`TenantApi::create`]. `idempotent: true` means the request
/// matched a prior `idempotency_key` within the 24h window — `tenant`
/// reflects the original create, not a new row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantProvisionResult {
    pub tenant: Tenant,
    #[serde(default)]
    pub admin_user_id: String,
    #[serde(default)]
    pub session: ExchangedSession,
    #[serde(default)]
    pub installed_apps: Vec<AppInstall>,
    #[serde(default)]
    pub idempotent: bool,
}

/// Canonical tenant record. Matches `backend/rust/platform/src/models.rs::Tenant`
/// — every optional field is `Option<_>`, and the opaque JSON blobs
/// (`settings`, `features`, `branding`, `metadata`) are kept as
/// [`serde_json::Value`] so the SDK doesn't couple to their shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    pub id: String,
    pub slug: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legal_name: Option<String>,
    #[serde(default)]
    pub industry: String,
    #[serde(default)]
    pub subscription_tier: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default)]
    pub settings: Value,
    #[serde(default)]
    pub features: Value,
    #[serde(default)]
    pub branding: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stripe_customer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stripe_connect_account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trial_ends_at: Option<String>,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub is_suspended: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suspension_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company_id: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub is_nebusai_company: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    /// New column from #3410 — soft-delete timestamp for
    /// [`TenantApi::retire`] / [`TenantApi::unretire`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retired_at: Option<String>,
}

/// One row returned by [`TenantApi::my_tenants`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantOption {
    pub tenant_id: String,
    pub slug: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

// ---------------------------------------------------------------------------
// TenantApi
// ---------------------------------------------------------------------------

/// Access to `/tenant/*` endpoints. Obtain via [`OlympusClient::tenant`].
///
/// Borrow pattern — holds a shared reference to the parent client so the
/// API can rotate tokens on [`TenantApi::switch_tenant`] without cloning an
/// extra [`std::sync::Arc`]. Drop when you're done; cheap to construct per
/// call site.
pub struct TenantApi<'a> {
    client: &'a OlympusClient,
}

impl<'a> TenantApi<'a> {
    /// Constructs a new `TenantApi`. Usually obtained via
    /// [`OlympusClient::tenant`] rather than directly.
    pub fn new(client: &'a OlympusClient) -> Self {
        Self { client }
    }

    /// `POST /tenant/create` — self-service tenant provisioning (§2 + §4.4).
    ///
    /// Idempotent on `req.idempotency_key` within a 24h window. The returned
    /// [`TenantProvisionResult::idempotent`] distinguishes a fresh create
    /// from a retry that matched an existing row.
    pub async fn create(&self, req: TenantCreateRequest) -> Result<TenantProvisionResult> {
        let body = serde_json::to_value(&req)?;
        let raw = self.client.http().post("/tenant/create", &body).await?;
        Ok(serde_json::from_value(raw)?)
    }

    /// `GET /tenant/current` — fetch the tenant scoped by the current session.
    pub async fn current(&self) -> Result<Tenant> {
        let raw = self.client.http().get("/tenant/current").await?;
        Ok(serde_json::from_value(raw)?)
    }

    /// `PATCH /tenant/current` — partial update of the tenant scoped by the
    /// current session. Requires `tenant_admin` on the server.
    pub async fn update(&self, patch: TenantUpdate) -> Result<Tenant> {
        let body = serde_json::to_value(&patch)?;
        let raw = self.client.http().patch("/tenant/current", &body).await?;
        Ok(serde_json::from_value(raw)?)
    }

    /// `POST /tenant/retire` — soft-delete the tenant with a 30-day grace
    /// window. `confirmation_slug` MUST equal the tenant's slug (typed-to-
    /// confirm UX). Server requires a recent MFA attestation on the session;
    /// on `403 mfa_required` the caller should trigger step-up and retry.
    ///
    /// The server accepts an optional `reason` — this wrapper passes
    /// `reason: None`. Apps that want to record a retirement reason can
    /// call [`TenantApi::retire_with_reason`] instead.
    pub async fn retire(&self, confirmation_slug: &str) -> Result<()> {
        self.retire_with_reason(confirmation_slug, None).await
    }

    /// `POST /tenant/retire` with an optional reason. Callers that want to
    /// record a retirement reason in the `tenant.retired` event payload
    /// use this; everyone else should use [`TenantApi::retire`] for the
    /// simpler signature.
    pub async fn retire_with_reason(
        &self,
        confirmation_slug: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        let body = json!({
            "confirmation_slug": confirmation_slug,
            "reason": reason,
        });
        self.client.http().post("/tenant/retire", &body).await?;
        Ok(())
    }

    /// `POST /tenant/unretire` — reverse a prior [`TenantApi::retire`] while
    /// still inside the 30-day grace window. Returns an error if the grace
    /// window has expired.
    pub async fn unretire(&self) -> Result<()> {
        self.client
            .http()
            .post("/tenant/unretire", &json!({}))
            .await?;
        Ok(())
    }

    /// `GET /tenant/mine` — every tenant the signed-in user has access to,
    /// across tenants. Server-side this fans out via the caller's email
    /// claim on the auth_users table.
    pub async fn my_tenants(&self) -> Result<Vec<TenantOption>> {
        let raw = self.client.http().get("/tenant/mine").await?;
        Ok(serde_json::from_value(raw)?)
    }

    /// Chain `POST /tenant/switch` → `POST /auth/switch-tenant` to switch the
    /// active session to a different tenant.
    ///
    /// On success the SDK rotates the HTTP client's access + refresh tokens
    /// to the freshly-minted pair, so the very next SDK call is scoped to
    /// the new tenant automatically. Callers that want to observe the
    /// transition can subscribe via [`OlympusClient::session_events`] —
    /// rotation does NOT fire a session event on its own because a refresh
    /// token rotation is not a login and not a silent refresh.
    pub async fn switch_tenant(&self, tenant_id: &str) -> Result<ExchangedSession> {
        // 1. Validate access via platform.
        let switch_body = json!({ "tenant_id": tenant_id });
        let platform_resp = self
            .client
            .http()
            .post("/tenant/switch", &switch_body)
            .await?;
        let auth_endpoint = platform_resp
            .get("auth_endpoint")
            .and_then(|v| v.as_str())
            .unwrap_or("/auth/switch-tenant");

        // 2. Chain to auth to mint the switched session.
        let auth_body = json!({ "tenant_id": tenant_id });
        let token_resp = self.client.http().post(auth_endpoint, &auth_body).await?;
        let session = parse_token_response(&token_resp)?;

        // 3. Rotate the HTTP client's tokens so subsequent calls are scoped
        //    to the new tenant. `access_token` and `refresh_token` are
        //    always present on a successful /auth/switch-tenant response.
        if let Some(access) = session.access_token.as_deref() {
            self.client.set_access_token(access);
        }
        if let Some(refresh) = session.refresh_token.as_deref() {
            self.client.set_refresh_token(refresh);
        }

        Ok(session)
    }
}

/// Parse the subset of `/auth/switch-tenant`'s `TokenResponse` the SDK cares
/// about — access_token + refresh_token. `expires_in` (seconds) is combined
/// with the current wall clock to synthesize an `access_expires_at` RFC3339
/// string when the server didn't stamp one directly.
fn parse_token_response(value: &Value) -> Result<ExchangedSession> {
    let obj: &Map<String, Value> = value.as_object().ok_or_else(|| OlympusError::Api {
        status: 500,
        message: "expected JSON object from /auth/switch-tenant".into(),
    })?;

    let access_token = obj
        .get("access_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let refresh_token = obj
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // `expires_in` is documented on `TokenResponse` as seconds-from-now.
    // We don't synthesize the RFC3339 timestamp here to avoid pulling the
    // `chrono` dependency into the SDK's public surface — leave it to
    // consumers if they need it.
    let access_expires_at = obj
        .get("access_expires_at")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(ExchangedSession {
        access_token,
        refresh_token,
        access_expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenant_create_request_serializes_snake_case_matching_backend() {
        let req = TenantCreateRequest {
            brand_name: "Test Co".into(),
            slug: "test-co".into(),
            plan: "starter".into(),
            first_admin: TenantFirstAdmin {
                firebase_link: Some("fb-uid".into()),
                email: "admin@test.co".into(),
                first_name: "Admin".into(),
                last_name: "User".into(),
            },
            install_apps: vec!["pizza-os".into()],
            billing_address: Some("123 Main St".into()),
            tax_id: None,
            idempotency_key: "idem-123".into(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["brand_name"], "Test Co");
        assert_eq!(json["first_admin"]["firebase_link"], "fb-uid");
        assert_eq!(json["idempotency_key"], "idem-123");
        // `tax_id: None` is skipped.
        assert!(json.get("tax_id").is_none());
    }

    #[test]
    fn tenant_option_deserializes_with_optional_role() {
        let value = json!({
            "tenant_id": "t1",
            "slug": "t-one",
            "name": "Tenant One"
        });
        let opt: TenantOption = serde_json::from_value(value).unwrap();
        assert_eq!(opt.slug, "t-one");
        assert!(opt.role.is_none());
    }

    #[test]
    fn parse_token_response_extracts_access_and_refresh() {
        let value = json!({
            "access_token": "new-access",
            "refresh_token": "new-refresh",
            "token_type": "Bearer",
            "expires_in": 3600,
            "user": {"id": "u1"},
        });
        let session = parse_token_response(&value).unwrap();
        assert_eq!(session.access_token.as_deref(), Some("new-access"));
        assert_eq!(session.refresh_token.as_deref(), Some("new-refresh"));
    }
}
