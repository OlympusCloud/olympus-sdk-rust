use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Admin operations service for platform-level management.
///
/// Wraps the Olympus Admin Operations endpoints (Python) via the Go API Gateway.
/// Routes: `/admin/*`, `/devbox/*`.
///
/// Related issues: #243 (Admin Operations Suite)
pub struct AdminOpsService {
    http: Arc<OlympusHttpClient>,
}

impl AdminOpsService {
    /// Creates a new AdminOpsService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    // ─── Impersonation ────────────────────────────────────────────

    /// Start an impersonation session (super admin only).
    pub async fn start_impersonation(
        &self,
        target_user_id: &str,
        reason: &str,
    ) -> Result<Value> {
        let body = json!({
            "target_user_id": target_user_id,
            "reason": reason,
        });
        self.http.post("/admin/impersonate", &body).await
    }

    /// End an active impersonation session.
    pub async fn end_impersonation(&self) -> Result<Value> {
        self.http
            .post("/admin/impersonate/end", &json!({}))
            .await
    }

    // ─── Billing ──────────────────────────────────────────────────

    /// Get the platform billing overview.
    pub async fn billing_overview(&self, tenant_id: Option<&str>) -> Result<Value> {
        let mut path = "/admin/billing/overview".to_string();
        if let Some(tid) = tenant_id {
            path.push_str(&format!("?tenant_id={}", tid));
        }
        self.http.get(&path).await
    }

    /// Apply a billing adjustment (credit, refund, discount).
    pub async fn billing_adjust(
        &self,
        tenant_id: &str,
        amount_cents: i64,
        reason: &str,
        adjustment_type: &str,
    ) -> Result<Value> {
        let body = json!({
            "tenant_id": tenant_id,
            "amount_cents": amount_cents,
            "reason": reason,
            "adjustment_type": adjustment_type,
        });
        self.http.post("/admin/billing/adjust", &body).await
    }

    // ─── Sales ────────────────────────────────────────────────────

    /// Get the sales pipeline metrics.
    pub async fn sales_pipeline(&self) -> Result<Value> {
        self.http.get("/admin/sales/pipeline").await
    }

    /// Create a new sales prospect.
    pub async fn create_prospect(
        &self,
        company_name: &str,
        contact_email: &str,
        source: &str,
    ) -> Result<Value> {
        let body = json!({
            "company_name": company_name,
            "contact_email": contact_email,
            "source": source,
        });
        self.http.post("/admin/sales/prospect", &body).await
    }

    // ─── Support ──────────────────────────────────────────────────

    /// List support tickets with optional filters.
    pub async fn list_support_tickets(
        &self,
        status: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Value> {
        let mut path = "/admin/support/tickets".to_string();
        let mut params = Vec::new();
        if let Some(s) = status {
            params.push(format!("status={}", s));
        }
        if let Some(l) = limit {
            params.push(format!("limit={}", l));
        }
        if !params.is_empty() {
            path.push_str(&format!("?{}", params.join("&")));
        }
        self.http.get(&path).await
    }

    /// Create a new support ticket.
    pub async fn create_support_ticket(
        &self,
        subject: &str,
        description: &str,
        priority: &str,
    ) -> Result<Value> {
        let body = json!({
            "subject": subject,
            "description": description,
            "priority": priority,
        });
        self.http.post("/admin/support/tickets", &body).await
    }

    // ─── Onboarding ──────────────────────────────────────────────

    /// Get onboarding status for a tenant.
    pub async fn onboarding_status(&self, tenant_id: &str) -> Result<Value> {
        self.http
            .get(&format!("/admin/onboarding/status?tenant_id={}", tenant_id))
            .await
    }

    /// Mark an onboarding step as complete.
    pub async fn complete_onboarding_step(
        &self,
        tenant_id: &str,
        step: &str,
    ) -> Result<Value> {
        let body = json!({
            "tenant_id": tenant_id,
            "step": step,
        });
        self.http.post("/admin/onboarding/complete", &body).await
    }

    // ─── Devbox ──────────────────────────────────────────────────

    /// List stale devbox sandboxes.
    pub async fn list_stale_devboxes(&self) -> Result<Value> {
        self.http.get("/devbox/stale").await
    }

    /// Clean up a specific devbox sandbox.
    pub async fn cleanup_devbox(&self, devbox_id: &str) -> Result<Value> {
        let body = json!({ "devbox_id": devbox_id });
        self.http.post("/devbox/cleanup", &body).await
    }
}
