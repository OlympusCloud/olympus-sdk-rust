use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// POS voice order integration service.
///
/// Supports Square, Toast, and Clover POS systems (auto-detected from tenant).
/// Routes: `/pos/*`.
pub struct PosService {
    http: Arc<OlympusHttpClient>,
}

impl PosService {
    /// Creates a new PosService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Submits a voice-parsed order to the tenant's POS system.
    pub async fn submit_voice_order(&self, order: Value) -> Result<Value> {
        self.http.post("/pos/voice-order", &order).await
    }

    /// Triggers a menu sync from POS to the voice AI knowledge base.
    pub async fn sync_menu(&self, tenant_id: &str) -> Result<Value> {
        let body = json!({});
        self.http
            .post(&format!("/pos/{}/sync-menu", tenant_id), &body)
            .await
    }
}
