use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Admin API for billing plan management, add-ons, minute packs, and usage metering.
///
/// Distinct from the tenant-facing billing API. This service manages the global
/// plan catalog and usage recording.
/// Routes: `/admin/billing/*`.
///
/// Requires: admin role (super_admin, platform_admin).
pub struct AdminBillingService {
    http: Arc<OlympusHttpClient>,
}

impl AdminBillingService {
    /// Creates a new AdminBillingService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    // ─── Plan CRUD ───────────────────────────────────────────────

    /// Create a new billing plan in the catalog.
    pub async fn create_plan(&self, plan: &Value) -> Result<Value> {
        self.http.post("/admin/billing/plans", plan).await
    }

    /// Update an existing billing plan.
    pub async fn update_plan(&self, plan_id: &str, updates: &Value) -> Result<Value> {
        self.http
            .put(&format!("/admin/billing/plans/{}", plan_id), updates)
            .await
    }

    /// Delete a billing plan. Fails if tenants are actively subscribed.
    pub async fn delete_plan(&self, plan_id: &str) -> Result<Value> {
        self.http
            .delete(&format!("/admin/billing/plans/{}", plan_id))
            .await
    }

    /// List all billing plans in the catalog.
    pub async fn list_plans(&self) -> Result<Value> {
        self.http.get("/admin/billing/plans").await
    }

    // ─── Add-ons & Minute Packs ──────────────────────────────────

    /// Create a purchasable add-on (e.g. extra SMS bundle, premium support).
    pub async fn create_addon(&self, addon: &Value) -> Result<Value> {
        self.http.post("/admin/billing/addons", addon).await
    }

    /// Create a minute pack (pre-paid voice minutes bundle).
    pub async fn create_minute_pack(&self, pack: &Value) -> Result<Value> {
        self.http.post("/admin/billing/minute-packs", pack).await
    }

    // ─── Usage Metering ──────────────────────────────────────────

    /// Get usage data for a tenant, optionally filtered by meter type.
    pub async fn get_usage(
        &self,
        tenant_id: &str,
        meter_type: Option<&str>,
    ) -> Result<Value> {
        let path = format!("/admin/billing/usage/{}", tenant_id);
        if let Some(mt) = meter_type {
            self.http
                .get_with_query(&path, &[("meter_type", mt)])
                .await
        } else {
            self.http.get(&path).await
        }
    }

    /// Record a usage event for a tenant's meter.
    pub async fn record_usage(
        &self,
        tenant_id: &str,
        meter_type: &str,
        quantity: f64,
    ) -> Result<Value> {
        let body = json!({
            "meter_type": meter_type,
            "quantity": quantity,
        });
        self.http
            .post(&format!("/admin/billing/usage/{}", tenant_id), &body)
            .await
    }
}
