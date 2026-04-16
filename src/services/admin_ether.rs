use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Admin API for managing the Ether AI model catalog at runtime.
///
/// Provides CRUD for models and tiers, plus hot-reload of the catalog cache.
/// Routes: `/admin/ether/*`.
///
/// Requires: admin role (super_admin, platform_admin).
pub struct AdminEtherService {
    http: Arc<OlympusHttpClient>,
}

impl AdminEtherService {
    /// Creates a new AdminEtherService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    // ─── Model CRUD ──────────────────────────────────────────────

    /// Register a new AI model in the Ether catalog.
    pub async fn create_model(&self, model: &Value) -> Result<Value> {
        self.http.post("/admin/ether/models", model).await
    }

    /// Update an existing model's configuration.
    pub async fn update_model(&self, model_id: &str, updates: &Value) -> Result<Value> {
        self.http
            .put(&format!("/admin/ether/models/{}", model_id), updates)
            .await
    }

    /// Remove a model from the catalog.
    pub async fn delete_model(&self, model_id: &str) -> Result<Value> {
        self.http
            .delete(&format!("/admin/ether/models/{}", model_id))
            .await
    }

    /// List models in the catalog, optionally filtered by tier or provider.
    pub async fn list_models(
        &self,
        tier: Option<&str>,
        provider: Option<&str>,
    ) -> Result<Value> {
        let mut params = Vec::new();
        if let Some(t) = tier {
            params.push(("tier", t));
        }
        if let Some(p) = provider {
            params.push(("provider", p));
        }
        if params.is_empty() {
            self.http.get("/admin/ether/models").await
        } else {
            self.http
                .get_with_query("/admin/ether/models", &params)
                .await
        }
    }

    // ─── Tier Management ─────────────────────────────────────────

    /// List all Ether tiers (T1-T6) with current configuration.
    pub async fn list_tiers(&self) -> Result<Value> {
        self.http.get("/admin/ether/tiers").await
    }

    /// Update a tier's configuration (e.g. default model, rate limits).
    pub async fn update_tier(&self, tier_number: u32, updates: &Value) -> Result<Value> {
        self.http
            .put(&format!("/admin/ether/tiers/{}", tier_number), updates)
            .await
    }

    // ─── Cache ───────────────────────────────────────────────────

    /// Force a hot-reload of the model catalog from the backing store.
    pub async fn reload_catalog(&self) -> Result<Value> {
        self.http
            .post("/admin/ether/catalog/reload", &json!({}))
            .await
    }
}
