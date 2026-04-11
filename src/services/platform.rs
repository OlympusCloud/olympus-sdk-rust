use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Tenant lifecycle service for signup and cleanup workflows.
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
    pub async fn cleanup(
        &self,
        tenant_id: &str,
        reason: &str,
        export_data: bool,
    ) -> Result<Value> {
        let body = json!({
            "tenant_id": tenant_id,
            "reason": reason,
            "export_data": export_data,
            "grace_period_days": 30,
        });
        self.http.post("/platform/cleanup", &body).await
    }
}
