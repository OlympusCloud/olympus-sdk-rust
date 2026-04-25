use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Platform-wide service: tenant lifecycle (signup/cleanup) + scope catalog.
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

    /// List the seeded scope catalog (#3517).
    ///
    /// Optional filters:
    /// - `namespace` — filter to one namespace (e.g. `voice`, `platform`)
    /// - `owner_app_id` — filter by owning app id; pass `Some("")` for the
    ///   explicit "platform-owned only" filter (semantically distinct from
    ///   `None`, which means no filter)
    /// - `include_drafts` — include rows still in pre-`service_ok` workshop
    ///   status (default false → published surface only)
    pub async fn list_scope_registry(
        &self,
        params: ListScopeRegistryParams,
    ) -> Result<ScopeRegistryListing> {
        let mut q: Vec<(&str, &str)> = Vec::new();
        if let Some(ns) = params.namespace.as_deref() {
            q.push(("namespace", ns));
        }
        // Some("") MUST round-trip as `owner_app_id=` (empty) so the
        // server applies the platform-owned-only filter; None omits the key.
        if let Some(app) = params.owner_app_id.as_deref() {
            q.push(("owner_app_id", app));
        }
        if params.include_drafts {
            q.push(("include_drafts", "true"));
        }
        let resp = self
            .http
            .get_with_query("/platform/scope-registry", &q)
            .await?;
        Ok(serde_json::from_value(resp)?)
    }

    /// Fetch the deterministic platform catalog digest (#3517).
    ///
    /// Returns sha256 over `(bit_id, scope, status)` rows of the platform-
    /// tier registry. Matches `scripts/seed_platform_scopes.py` output
    /// byte-for-byte. JWT mints embed this digest so the gateway middleware
    /// can detect stale tokens after a catalog rotation.
    pub async fn get_scope_registry_digest(&self) -> Result<ScopeRegistryDigest> {
        let resp = self.http.get("/platform/scope-registry/digest").await?;
        Ok(serde_json::from_value(resp)?)
    }
}

/// Filters for [`PlatformService::list_scope_registry`] (#3517).
///
/// `owner_app_id = Some("")` is the explicit "platform-owned only" filter,
/// distinct from `None` (no filter). The wrapper preserves both cases on
/// the wire.
#[derive(Debug, Clone, Default)]
pub struct ListScopeRegistryParams {
    pub namespace: Option<String>,
    pub owner_app_id: Option<String>,
    pub include_drafts: bool,
}

/// One row of the platform scope registry (#3517).
///
/// `bit_id` is `None` when the scope hasn't been allocated a bit yet
/// (workshop_status pre-`service_ok`). Pre-allocation rows can still
/// appear in authoring views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeRow {
    pub scope: String,
    pub resource: String,
    pub action: String,
    pub holder: String,
    pub namespace: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_app_id: Option<String>,
    pub description: String,
    #[serde(default)]
    pub is_destructive: bool,
    #[serde(default)]
    pub requires_mfa: bool,
    pub grace_behavior: String,
    pub consent_prompt_copy: String,
    pub workshop_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bit_id: Option<i64>,
}

/// Result of [`PlatformService::list_scope_registry`] (#3517).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeRegistryListing {
    #[serde(default)]
    pub scopes: Vec<ScopeRow>,
    #[serde(default)]
    pub total: usize,
}

/// Result of [`PlatformService::get_scope_registry_digest`] (#3517).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeRegistryDigest {
    /// SHA-256 hex matching `scripts/seed_platform_scopes.py` byte-for-byte.
    #[serde(default)]
    pub platform_catalog_digest: String,
    /// Number of (active|deprecated|reserved_deprecated) rows in the digest.
    #[serde(default)]
    pub row_count: usize,
}
