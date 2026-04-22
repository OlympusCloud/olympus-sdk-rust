//! AppsApi — canonical `/apps/*` install ceremony surface (#3413 §3).
//!
//! Wraps the Rust platform service `apps_install` handler (shipped in
//! olympus-cloud-gcp#3422) exposed through the Go API Gateway. Drives the
//! four-state consent ceremony used whenever an extracted app (PizzaOS,
//! BarOS, CallStackAI, …) needs to be installed on a tenant.
//!
//! # Route map
//!
//! | Method | Route                                   | Auth                      | SDK method                          |
//! |--------|-----------------------------------------|---------------------------|-------------------------------------|
//! | POST   | /apps/install                           | tenant_admin + recent MFA | [`AppsApi::install`]                |
//! | GET    | /apps/installed                         | any tenant-scoped JWT     | [`AppsApi::list_installed`]         |
//! | POST   | /apps/uninstall/:app_id                 | tenant_admin + recent MFA | [`AppsApi::uninstall`]              |
//! | GET    | /apps/manifest/:app_id                  | any authenticated         | [`AppsApi::get_manifest`]           |
//! | GET    | /apps/pending_install/:id               | **anonymous**             | [`AppsApi::get_pending_install`]    |
//! | POST   | /apps/pending_install/:id/approve       | tenant_admin              | [`AppsApi::approve_pending_install`]|
//! | POST   | /apps/pending_install/:id/deny          | tenant_admin              | [`AppsApi::deny_pending_install`]   |
//!
//! # Ceremony
//!
//!   1. An app calls [`AppsApi::install`] against the platform. The server
//!      creates a pending-install row with a 10-minute TTL and returns a
//!      [`PendingInstall`] carrying an unguessable `pending_install_id` + a
//!      platform-served `consent_url`.
//!   2. The app redirects the tenant_admin's browser to the consent URL.
//!      That surface calls [`AppsApi::get_pending_install`] anonymously
//!      (the unguessable id IS the bearer) and gets back a
//!      [`PendingInstallDetail`] with an eager-loaded [`AppManifest`] for
//!      the consent screen's required-scope / optional-scope checklists.
//!   3. The tenant_admin clicks Approve or Deny. The consent surface POSTs
//!      to [`AppsApi::approve_pending_install`] (returns the fresh
//!      [`AppInstall`]) or [`AppsApi::deny_pending_install`] (returns
//!      nothing).
//!   4. [`AppsApi::list_installed`] / [`AppsApi::uninstall`] /
//!      [`AppsApi::get_manifest`] cover the steady-state app-management
//!      surface.
//!
//! # MFA gate
//!
//! install + uninstall + approve require the tenant_admin's session to
//! carry a `mfa_verified_at:<epoch>` permission stamp within the last 10
//! minutes. If missing, the server returns 403 with `mfa_required` — the
//! SDK surfaces this as [`crate::error::OlympusError::Api`]; consumers
//! should trigger a step-up flow and retry.
//!
//! # Naming note
//!
//! This module's [`AppInstall`] is the canonical 6-field `/apps/installed`
//! row shape (matches `AppInstall` in olympus-sdk-dart's `apps.dart`). The
//! lean 3-field shape returned inline by `/tenant/create` lives in
//! [`crate::tenant::TenantAppInstall`].

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::client::OlympusClient;
use crate::error::Result;

// ---------------------------------------------------------------------------
// Request / response shapes — mirror backend apps_install handler exactly.
// ---------------------------------------------------------------------------

/// Payload for [`AppsApi::install`].
///
/// `scopes` must be a subset of `manifest.scopes_required ∪
/// manifest.scopes_optional`; the backend rejects unknown scopes with 400
/// before any Spanner write. `return_to` is the post-approval deep-link the
/// consent surface redirects to on Approve/Deny.
///
/// `idempotency_key` is optional — when supplied, retrying the same
/// `(tenant_id, app_id, idempotency_key)` within the 10-minute pending
/// window returns the original [`PendingInstall`] rather than creating a
/// second pending row. Use the calling user's device fingerprint or a UUID
/// generated per "Install" button press to de-dupe retry noise without
/// cross-user collisions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppInstallRequest {
    /// Reverse-DNS app identifier (e.g. `com.pizzaos`).
    pub app_id: String,
    /// Canonical scope strings the app is requesting at install time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    /// Absolute URL the consent surface redirects to on Approve/Deny.
    pub return_to: String,
    /// Optional idempotency key — retries with the same
    /// `(tenant_id, app_id, idempotency_key)` return the original pending row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

/// Handle returned by [`AppsApi::install`].
///
/// The caller must redirect the tenant_admin to [`Self::consent_url`] before
/// [`Self::expires_at`] (10 minutes after creation). After expiry the
/// consent URL returns 410 Gone and the caller must restart the ceremony.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingInstall {
    /// Server-assigned UUID. Opaque to callers — treat as a pointer to the
    /// pending row, not a user-facing identifier.
    pub pending_install_id: String,
    /// Platform-served consent URL. Apps MUST open this in a real browser
    /// tab (NOT an in-app webview) so the tenant_admin's authenticated
    /// cookie session is visible to the platform domain.
    pub consent_url: String,
    /// Absolute RFC3339 UTC expiry timestamp. Kept as a `String` to avoid
    /// pulling `chrono` into the SDK's public surface (mirrors
    /// [`crate::tenant::ExchangedSession`]).
    pub expires_at: String,
}

/// Versioned manifest row for an app in the platform catalog.
///
/// Returned by [`AppsApi::get_manifest`] (latest version) and eager-loaded
/// onto [`PendingInstallDetail::manifest`] so the consent screen can render
/// the required / optional scope checklists + publisher / privacy / TOS
/// links without a second round-trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppManifest {
    /// Reverse-DNS app identifier. Primary key against the catalog.
    pub app_id: String,
    /// Semver string (e.g. `1.4.0`). Always the latest published row when
    /// multiple versions exist.
    pub version: String,
    /// Human-facing app name for the consent screen header.
    pub name: String,
    /// Human-facing publisher name (e.g. `NëbusAI`).
    pub publisher: String,
    /// Optional URL to the app's square icon (typically CDN-hosted PNG/SVG).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
    /// Canonical scope strings the app CANNOT operate without.
    #[serde(default)]
    pub scopes_required: Vec<String>,
    /// Canonical scope strings the app can operate without but may request.
    #[serde(default)]
    pub scopes_optional: Vec<String>,
    /// Optional URL to the app's privacy policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy_url: Option<String>,
    /// Optional URL to the app's terms of service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tos_url: Option<String>,
}

/// Full pending-install row — returned by [`AppsApi::get_pending_install`].
///
/// **That endpoint is anonymous** — no JWT required. The unguessable id IS
/// the bearer, and the row expires 10 minutes after creation. Rendered by
/// the platform's consent surface to drive the Approve / Deny buttons.
///
/// [`Self::status`] values: `pending` (active ceremony row) | `approved` |
/// `denied`. Rows in a terminal state still return 200 (not 410) so the
/// consent UI can show a clear "already approved / already denied" state
/// instead of a generic expiry message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingInstallDetail {
    pub id: String,
    pub app_id: String,
    pub tenant_id: String,
    /// Scopes the app asked for on [`AppsApi::install`] — always a subset of
    /// `manifest.scopes_required ∪ manifest.scopes_optional` at create
    /// time. Re-validated at approve time in case the manifest changed.
    #[serde(default)]
    pub requested_scopes: Vec<String>,
    /// Post-approval deep-link the app provided.
    #[serde(default)]
    pub return_to: String,
    /// `pending` | `approved` | `denied`.
    #[serde(default)]
    pub status: String,
    pub expires_at: String,
    /// Server-side eager-loaded manifest for the consent UI. `None` only in
    /// the rare case the manifest was delisted between create and read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<AppManifest>,
}

/// A row from `tenant_app_installs`. Returned by
/// [`AppsApi::list_installed`] and as the result of
/// [`AppsApi::approve_pending_install`].
///
/// This is the canonical `AppInstall` shape for the `/apps/*` ceremony —
/// parity with `AppInstall` in olympus-sdk-dart's `apps.dart`. The lean
/// 3-field shape returned inline by `/tenant/create` is the distinct
/// [`crate::tenant::TenantAppInstall`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInstall {
    pub tenant_id: String,
    pub app_id: String,
    pub installed_at: String,
    /// User UUID of the tenant_admin who approved the install. On an install
    /// created via `/tenant/create` auto-install this is the newly-minted
    /// first admin's id.
    #[serde(default)]
    pub installed_by: String,
    /// Scopes granted at approval time. A subset of the manifest's
    /// required ∪ optional sets — the admin may have opted out of
    /// individual optional scopes.
    #[serde(default)]
    pub scopes_granted: Vec<String>,
    /// `active` during normal operation; `uninstalled` after
    /// [`AppsApi::uninstall`]. `list_installed` filters out uninstalled
    /// rows server-side.
    #[serde(default)]
    pub status: String,
}

// ---------------------------------------------------------------------------
// AppsApi
// ---------------------------------------------------------------------------

/// Access to `/apps/*` endpoints. Obtain via [`OlympusClient::apps`].
///
/// Borrow pattern — holds a shared reference to the parent client. Drop
/// when done; cheap to construct per call site.
pub struct AppsApi<'a> {
    client: &'a OlympusClient,
}

impl<'a> AppsApi<'a> {
    /// Constructs a new `AppsApi`. Usually obtained via
    /// [`OlympusClient::apps`] rather than directly.
    pub fn new(client: &'a OlympusClient) -> Self {
        Self { client }
    }

    /// `POST /apps/install` — initiate the install ceremony.
    ///
    /// Server creates a pending-install row, validates `req.scopes` against
    /// the latest [`AppManifest`], and returns a [`PendingInstall`] with a
    /// consent URL the caller should open in a real browser tab (NOT an
    /// in-app webview — the consent screen needs the tenant_admin's
    /// authenticated cookie session on the platform domain).
    ///
    /// Requires tenant_admin role + recent MFA on the session. Surfaces
    /// 403 `mfa_required` when the MFA stamp is stale.
    pub async fn install(&self, req: AppInstallRequest) -> Result<PendingInstall> {
        let body = serde_json::to_value(&req)?;
        let raw = self.client.http().post("/apps/install", &body).await?;
        Ok(serde_json::from_value(raw)?)
    }

    /// `GET /apps/installed` — list every active app installed on the
    /// caller's tenant. Uninstalled rows are filtered out server-side.
    ///
    /// Safe to call on any tenant-scoped JWT; no role requirement.
    pub async fn list_installed(&self) -> Result<Vec<AppInstall>> {
        let raw = self.client.http().get("/apps/installed").await?;
        Ok(serde_json::from_value(raw)?)
    }

    /// `POST /apps/uninstall/:app_id` — mark the install `uninstalled` and
    /// emit `platform.app.uninstalled` on Pub/Sub. The auth service consumer
    /// for that event kicks session revocation for every JWT carrying this
    /// `(tenant_id, app_id)` pair (AC-7 on #3413: 60-second contract).
    ///
    /// Requires tenant_admin role + recent MFA. The server returns a
    /// `UninstallResult` body but the SDK discards it — parity with
    /// `olympus-sdk-dart`'s `uninstall` signature (`Future<void>`).
    pub async fn uninstall(&self, app_id: &str) -> Result<()> {
        let path = format!("/apps/uninstall/{}", urlencoding::encode(app_id));
        self.client.http().post(&path, &json!({})).await?;
        Ok(())
    }

    /// `GET /apps/manifest/:app_id` — fetch the latest published
    /// [`AppManifest`] for `app_id`. Useful for "available apps" browsers
    /// outside the ceremony flow.
    pub async fn get_manifest(&self, app_id: &str) -> Result<AppManifest> {
        let path = format!("/apps/manifest/{}", urlencoding::encode(app_id));
        let raw = self.client.http().get(&path).await?;
        Ok(serde_json::from_value(raw)?)
    }

    /// `GET /apps/pending_install/:id` — fetch the pending-install row.
    ///
    /// **Anonymous — no JWT required.** The id is an unguessable UUID with
    /// a 10-minute TTL, issued by the server on [`Self::install`]. The
    /// consent surface uses this call to render the Approve/Deny screen
    /// with the eager-loaded [`PendingInstallDetail::manifest`] (no second
    /// round-trip needed).
    ///
    /// Returns an API error with status 410 (Gone) if the pending row has
    /// expired or doesn't exist — the server masks "not found" as "gone" so
    /// an attacker can't enumerate ids.
    ///
    /// Safe to call with or without a session — the server ignores the
    /// Authorization header on this route.
    pub async fn get_pending_install(
        &self,
        pending_install_id: &str,
    ) -> Result<PendingInstallDetail> {
        let path = format!(
            "/apps/pending_install/{}",
            urlencoding::encode(pending_install_id)
        );
        let raw = self.client.http().get(&path).await?;
        Ok(serde_json::from_value(raw)?)
    }

    /// `POST /apps/pending_install/:id/approve` — approve a pending install.
    ///
    /// Server runs one Spanner transaction that resolves the pending row
    /// (`status=approved`) and upserts the `tenant_app_installs` row —
    /// returns the fresh [`AppInstall`]. Also emits `platform.app.installed`
    /// on Pub/Sub for downstream consumers (billing activation, analytics,
    /// welcome email, …).
    ///
    /// Requires tenant_admin role on the **target** tenant (the pending
    /// row's `tenant_id`, which may differ from the session's `tenant_id`
    /// if an admin is completing consent on a device scoped to a different
    /// tenant) + a recent MFA stamp. Server re-validates the requested
    /// scopes against the latest manifest — if the manifest was updated to
    /// remove a scope between install and approve, the call fails with 400.
    pub async fn approve_pending_install(&self, pending_install_id: &str) -> Result<AppInstall> {
        let path = format!(
            "/apps/pending_install/{}/approve",
            urlencoding::encode(pending_install_id)
        );
        let raw = self.client.http().post(&path, &json!({})).await?;
        Ok(serde_json::from_value(raw)?)
    }

    /// `POST /apps/pending_install/:id/deny` — deny a pending install.
    ///
    /// Marks the pending row `status=denied` and emits
    /// `platform.app.install_denied` for analytics / funnel tracking. No
    /// install record to surface on a deny.
    ///
    /// Requires tenant_admin role on the target tenant. Does NOT require
    /// fresh MFA — deny-by-default is always safe.
    pub async fn deny_pending_install(&self, pending_install_id: &str) -> Result<()> {
        let path = format!(
            "/apps/pending_install/{}/deny",
            urlencoding::encode(pending_install_id)
        );
        self.client.http().post(&path, &json!({})).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_install_request_serializes_snake_case_and_skips_none() {
        let req = AppInstallRequest {
            app_id: "com.pizzaos".into(),
            scopes: vec!["commerce.orders.read".into()],
            return_to: "https://pizza.shop/settings".into(),
            idempotency_key: None,
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["app_id"], "com.pizzaos");
        assert_eq!(v["scopes"][0], "commerce.orders.read");
        assert_eq!(v["return_to"], "https://pizza.shop/settings");
        assert!(v.get("idempotency_key").is_none());
    }

    #[test]
    fn app_install_request_skips_empty_scopes() {
        let req = AppInstallRequest {
            app_id: "com.barOS".into(),
            scopes: vec![],
            return_to: "https://bar.shop/x".into(),
            idempotency_key: Some("idem-1".into()),
        };
        let v = serde_json::to_value(&req).unwrap();
        assert!(v.get("scopes").is_none());
        assert_eq!(v["idempotency_key"], "idem-1");
    }

    #[test]
    fn pending_install_deserializes() {
        let value = json!({
            "pending_install_id": "7a3b...-uuid",
            "consent_url": "https://platform.olympuscloud.ai/apps/consent/7a3b...-uuid",
            "expires_at": "2026-04-21T00:10:00Z",
        });
        let p: PendingInstall = serde_json::from_value(value).unwrap();
        assert_eq!(p.pending_install_id, "7a3b...-uuid");
        assert_eq!(p.expires_at, "2026-04-21T00:10:00Z");
    }

    #[test]
    fn pending_install_detail_with_manifest_deserializes() {
        let value = json!({
            "id": "p1",
            "app_id": "com.pizzaos",
            "tenant_id": "t_pizza",
            "requested_scopes": ["a.read", "b.write"],
            "return_to": "https://pizza.shop/done",
            "status": "pending",
            "expires_at": "2026-04-21T00:10:00Z",
            "manifest": {
                "app_id": "com.pizzaos",
                "version": "1.0.0",
                "name": "PizzaOS",
                "publisher": "NëbusAI",
                "scopes_required": ["a.read"],
                "scopes_optional": ["b.write"],
            },
        });
        let detail: PendingInstallDetail = serde_json::from_value(value).unwrap();
        assert_eq!(detail.status, "pending");
        let manifest = detail.manifest.expect("manifest eager-loaded");
        assert_eq!(manifest.name, "PizzaOS");
        assert_eq!(manifest.scopes_required, vec!["a.read".to_string()]);
    }

    #[test]
    fn app_install_deserializes_with_defaults() {
        // status/installed_by/scopes_granted absent → defaults hold.
        let value = json!({
            "tenant_id": "t1",
            "app_id": "com.pizzaos",
            "installed_at": "2026-04-21T00:00:00Z",
        });
        let a: AppInstall = serde_json::from_value(value).unwrap();
        assert_eq!(a.status, "");
        assert!(a.scopes_granted.is_empty());
        assert_eq!(a.installed_by, "");
    }

    #[test]
    fn app_manifest_deserializes_without_optional_urls() {
        let value = json!({
            "app_id": "com.barOS",
            "version": "0.9.0",
            "name": "BarOS",
            "publisher": "NëbusAI",
            "scopes_required": [],
            "scopes_optional": [],
        });
        let m: AppManifest = serde_json::from_value(value).unwrap();
        assert!(m.logo_url.is_none());
        assert!(m.privacy_url.is_none());
        assert!(m.tos_url.is_none());
    }
}
