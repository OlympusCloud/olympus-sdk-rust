use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Admin API for managing CPaaS provider configuration and health.
///
/// Controls the Telnyx-primary / Twilio-fallback routing layer, provider
/// preferences per scope (tenant, brand, location), and circuit-breaker health.
/// Routes: `/admin/cpaas/*`.
///
/// Requires: admin role (super_admin, platform_admin).
pub struct AdminCpaasService {
    http: Arc<OlympusHttpClient>,
}

impl AdminCpaasService {
    /// Creates a new AdminCpaasService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Set the preferred CPaaS provider for a given scope.
    ///
    /// `scope` is one of `tenant`, `brand`, or `location`.
    /// `scope_id` is the ID of the scoped entity.
    /// `provider` is `telnyx` or `twilio`.
    pub async fn set_provider_preference(
        &self,
        scope: &str,
        scope_id: &str,
        provider: &str,
    ) -> Result<Value> {
        let body = json!({
            "scope": scope,
            "scope_id": scope_id,
            "provider": provider,
        });
        self.http
            .put("/admin/cpaas/provider-preference", &body)
            .await
    }

    /// Get the current health status of all CPaaS providers, including
    /// circuit-breaker state, latency, and failure counts.
    pub async fn get_provider_health(&self) -> Result<Value> {
        self.http.get("/admin/cpaas/health").await
    }
}
