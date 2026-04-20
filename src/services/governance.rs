//! GovernanceService — policy exception framework surface.
//!
//! olympus-cloud-gcp#3254 for the Rust SDK. See §17 of
//! docs/platform/APP-SCOPED-PERMISSIONS.md.
//!
//! Narrow scope — two policy keys at launch:
//!   - `session_ttl_role_ceiling` — extend role TTL for a specific app+role
//!   - `grace_policy_category`    — override whole-app grace policy
//!
//! No approve/deny/revoke in the SDK — those are Cockpit-only actions
//! requiring `platform_admin` JWT. SDK callers file + list + get status.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{OlympusError, Result};
use crate::http::OlympusHttpClient;

/// A policy exception record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionRequest {
    pub exception_id: String,
    pub app_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    pub policy_key: String,
    pub requested_value: Value,
    pub justification: String,
    pub risk_tier: String,
    pub risk_score: f64,
    pub risk_rationale: String,
    pub status: String,
    pub expires_at: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer_notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoke_reason: Option<String>,
}

/// Governance surface — narrow exception framework.
pub struct GovernanceService {
    http: Arc<OlympusHttpClient>,
}

impl GovernanceService {
    pub(crate) fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// File a new policy exception request.
    ///
    /// Platform auto-scores and routes to `auto_approved` (low risk) or
    /// `pending_review` (medium/high). `justification` must be ≥ 100 chars —
    /// this method enforces client-side to save a round-trip on short
    /// justifications that the server would reject with 400.
    pub async fn request_exception(
        &self,
        policy_key: &str,
        requested_value: Value,
        justification: &str,
        tenant_id: Option<&str>,
    ) -> Result<ExceptionRequest> {
        if justification.len() < 100 {
            return Err(OlympusError::Config(format!(
                "justification must be >= 100 chars (got {}); server validator rejects shorter",
                justification.len()
            )));
        }
        let mut body = json!({
            "policy_key": policy_key,
            "requested_value": requested_value,
            "justification": justification,
        });
        if let (Some(obj), Some(tid)) = (body.as_object_mut(), tenant_id) {
            obj.insert("tenant_id".into(), Value::String(tid.into()));
        }
        let resp = self.http.post("/api/v1/platform/exceptions", &body).await?;
        Ok(serde_json::from_value(resp)?)
    }

    /// List exceptions, optionally filtered by `app_id` + `status`.
    pub async fn list_exceptions(
        &self,
        app_id: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<ExceptionRequest>> {
        let mut q: Vec<(&str, &str)> = Vec::new();
        if let Some(a) = app_id {
            q.push(("app_id", a));
        }
        if let Some(s) = status {
            q.push(("status", s));
        }
        let body = if q.is_empty() {
            self.http.get("/api/v1/platform/exceptions").await?
        } else {
            self.http
                .get_with_query("/api/v1/platform/exceptions", &q)
                .await?
        };
        let rows = body
            .get("exceptions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(rows
            .into_iter()
            .filter_map(|row| serde_json::from_value(row).ok())
            .collect())
    }

    /// Fetch a single exception by ID.
    pub async fn get_exception(&self, exception_id: &str) -> Result<ExceptionRequest> {
        let path = format!(
            "/api/v1/platform/exceptions/{}",
            urlencoding::encode(exception_id)
        );
        let body = self.http.get(&path).await?;
        Ok(serde_json::from_value(body)?)
    }
}
