use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Voice order placement service (#2999).
///
/// AI voice agents collect orders by phone. This service validates prices
/// against the menu, stores orders in Spanner, and prepares them for POS push
/// and SMS confirmation.
///
/// Routes: `/voice-orders/*` (proxied via Go Gateway to Python).
pub struct VoiceOrdersService {
    http: Arc<OlympusHttpClient>,
}

/// Filter options for listing voice orders.
#[derive(Default)]
pub struct ListVoiceOrdersOptions<'a> {
    /// Filter by caller phone number.
    pub caller_phone: Option<&'a str>,
    /// Filter by order status (e.g. "pending", "confirmed").
    pub status: Option<&'a str>,
    /// Filter by location ID.
    pub location_id: Option<&'a str>,
    /// Maximum number of results (default 20, max 100).
    pub limit: Option<u32>,
}

impl VoiceOrdersService {
    /// Creates a new VoiceOrdersService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Creates a voice order -- validates item prices against the menu and stores
    /// the order in Spanner.
    ///
    /// * `location_id` -- The location placing the order.
    /// * `items` -- Array of order items, each with `menu_item_id`, `name`,
    ///   `quantity`, `unit_price`, and optional `modifiers`/`special_instructions`.
    /// * `fulfillment` -- `"pickup"` or `"delivery"` (default `"pickup"`).
    /// * `extra` -- Optional additional fields (delivery_address, caller_phone,
    ///   caller_name, payment_method, call_sid, agent_id, metadata).
    pub async fn create(
        &self,
        location_id: &str,
        items: Value,
        fulfillment: Option<&str>,
        extra: Option<Value>,
    ) -> Result<Value> {
        let mut body = json!({
            "location_id": location_id,
            "items": items,
            "fulfillment": fulfillment.unwrap_or("pickup"),
        });
        if let Some(Value::Object(map)) = extra {
            for (k, v) in map {
                body[k] = v;
            }
        }
        self.http.post("/voice-orders", &body).await
    }

    /// Creates a voice order from a pre-built JSON object.
    ///
    /// Dart-parity equivalent of `voiceOrders.create(order)`. Callers supply
    /// the full body (`location_id`, `items`, `fulfillment`, etc.) as a single
    /// [`Value`] instead of the typed field-by-field [`create`](Self::create).
    pub async fn create_raw(&self, order: Value) -> Result<Value> {
        self.http.post("/voice-orders", &order).await
    }

    /// Retrieves a single voice order by ID.
    pub async fn get(&self, order_id: &str) -> Result<Value> {
        self.http.get(&format!("/voice-orders/{}", order_id)).await
    }

    /// Lists voice orders with optional filters.
    pub async fn list(&self, opts: ListVoiceOrdersOptions<'_>) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(phone) = opts.caller_phone {
            query.push(("caller_phone", phone.to_string()));
        }
        if let Some(s) = opts.status {
            query.push(("status", s.to_string()));
        }
        if let Some(loc) = opts.location_id {
            query.push(("location_id", loc.to_string()));
        }
        if let Some(l) = opts.limit {
            query.push(("limit", l.to_string()));
        }
        let query_refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();

        if query_refs.is_empty() {
            self.http.get("/voice-orders").await
        } else {
            self.http.get_with_query("/voice-orders", &query_refs).await
        }
    }

    /// Pushes a voice order to the tenant's POS system (Toast/Square/Clover).
    ///
    /// Updates the order status to "confirmed" and sets `pos_push_status`
    /// to "submitted".
    pub async fn push_to_pos(&self, order_id: &str) -> Result<Value> {
        let body = json!({});
        self.http
            .post(&format!("/voice-orders/{}/push-pos", order_id), &body)
            .await
    }
}
