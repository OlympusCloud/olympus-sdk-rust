use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Orders, catalog, and commerce operations service.
///
/// Wraps the Olympus Commerce service (Rust, port 8003) via the Go API Gateway.
/// Routes: `/commerce/*`.
pub struct CommerceService {
    http: Arc<OlympusHttpClient>,
}

impl CommerceService {
    /// Creates a new CommerceService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Lists orders with optional status filter.
    pub async fn list_orders(&self, status: Option<&str>) -> Result<Value> {
        match status {
            Some(s) => {
                self.http
                    .get_with_query("/commerce/orders", &[("status", s)])
                    .await
            }
            None => self.http.get("/commerce/orders").await,
        }
    }

    /// Creates a new order.
    pub async fn create_order(&self, items: Value, source: &str) -> Result<Value> {
        let body = json!({
            "items": items,
            "source": source,
        });
        self.http.post("/commerce/orders", &body).await
    }

    /// Retrieves a single order by ID.
    pub async fn get_order(&self, order_id: &str) -> Result<Value> {
        self.http
            .get(&format!("/commerce/orders/{}", order_id))
            .await
    }
}
