use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Admin API for gating / feature flag management.
///
/// Provides CRUD for feature definitions, plan-level feature assignment,
/// resource limits, and evaluation. Distinct from the tenant-facing policy
/// evaluation API.
/// Routes: `/admin/gating/*`.
///
/// Requires: admin role (super_admin, platform_admin).
pub struct AdminGatingService {
    http: Arc<OlympusHttpClient>,
}

impl AdminGatingService {
    /// Creates a new AdminGatingService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    // ─── Feature Definitions ─────────────────────────────────────

    /// Define a new feature flag.
    pub async fn define_feature(
        &self,
        key: &str,
        description: Option<&str>,
        enabled: bool,
    ) -> Result<Value> {
        let body = json!({
            "key": key,
            "description": description,
            "enabled": enabled,
        });
        self.http.post("/admin/gating/features", &body).await
    }

    /// Update an existing feature flag.
    pub async fn update_feature(&self, key: &str, updates: &Value) -> Result<Value> {
        self.http
            .put(&format!("/admin/gating/features/{}", key), updates)
            .await
    }

    /// List all defined feature flags.
    pub async fn list_features(&self) -> Result<Value> {
        self.http.get("/admin/gating/features").await
    }

    // ─── Plan-Level Feature Assignment ───────────────────────────

    /// Set the list of feature keys enabled for a billing plan.
    pub async fn set_plan_features(
        &self,
        plan_id: &str,
        feature_keys: &[String],
    ) -> Result<Value> {
        let body = json!({ "feature_keys": feature_keys });
        self.http
            .put(
                &format!("/admin/gating/plans/{}/features", plan_id),
                &body,
            )
            .await
    }

    /// Get the features assigned to a billing plan.
    pub async fn get_plan_features(&self, plan_id: &str) -> Result<Value> {
        self.http
            .get(&format!("/admin/gating/plans/{}/features", plan_id))
            .await
    }

    // ─── Resource Limits ─────────────────────────────────────────

    /// Set a resource limit for a billing plan (e.g. max_agents, max_voice_min).
    pub async fn set_resource_limit(
        &self,
        plan_id: &str,
        resource: &str,
        limit: i64,
    ) -> Result<Value> {
        let body = json!({ "limit": limit });
        self.http
            .put(
                &format!("/admin/gating/plans/{}/limits/{}", plan_id, resource),
                &body,
            )
            .await
    }

    // ─── Evaluation ──────────────────────────────────────────────

    /// Evaluate a feature flag with optional tenant/user context.
    pub async fn evaluate_feature(
        &self,
        feature_key: &str,
        tenant_id: Option<&str>,
        user_id: Option<&str>,
    ) -> Result<Value> {
        let mut body = json!({ "feature_key": feature_key });
        if let Some(tid) = tenant_id {
            body["tenant_id"] = Value::String(tid.to_string());
        }
        if let Some(uid) = user_id {
            body["user_id"] = Value::String(uid.to_string());
        }
        self.http.post("/admin/gating/evaluate", &body).await
    }
}
