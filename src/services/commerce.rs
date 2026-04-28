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

    // -----------------------------------------------------------------------
    // 86 (out-of-stock) kill-switch — RC1 #3690 / #3695
    //
    // Manager-role-gated. Voice agent reads the 86'd lists at session
    // bootstrap and rejects affected items with a substitute suggestion.
    // -----------------------------------------------------------------------

    /// 86's a menu item (marks out-of-stock). Voice agent immediately stops
    /// offering it.
    ///
    /// `body` is a JSON object with optional `reason` (string), `until`
    /// (ISO-8601 timestamp), and `remaining_quantity` (i64) fields. Pass
    /// `serde_json::json!({})` for an unconditional 86.
    ///
    /// Route: `POST /commerce/menus/items/{id}/86` — RC1 #3690.
    pub async fn eighty_six_item(&self, item_id: &str, body: &Value) -> Result<Value> {
        self.http
            .post(&format!("/commerce/menus/items/{}/86", item_id), body)
            .await
    }

    /// Un-86's a menu item (restores availability).
    ///
    /// Route: `DELETE /commerce/menus/items/{id}/86`.
    pub async fn un_eighty_six_item(&self, item_id: &str) -> Result<Value> {
        self.http
            .delete(&format!("/commerce/menus/items/{}/86", item_id))
            .await
    }

    /// 86's an ingredient. Cascades to all items containing it.
    ///
    /// Route: `POST /commerce/menus/ingredients/{id}/86` — RC1 #3695.
    pub async fn eighty_six_ingredient(
        &self,
        ingredient_id: &str,
        body: &Value,
    ) -> Result<Value> {
        self.http
            .post(
                &format!("/commerce/menus/ingredients/{}/86", ingredient_id),
                body,
            )
            .await
    }

    /// Un-86's an ingredient (restores availability).
    ///
    /// Route: `DELETE /commerce/menus/ingredients/{id}/86`.
    pub async fn un_eighty_six_ingredient(&self, ingredient_id: &str) -> Result<Value> {
        self.http
            .delete(&format!("/commerce/menus/ingredients/{}/86", ingredient_id))
            .await
    }

    /// Lists currently 86'd menu items for the tenant.
    ///
    /// Route: `GET /commerce/menus/items/86`.
    pub async fn list_eighty_sixed_items(&self) -> Result<Value> {
        self.http.get("/commerce/menus/items/86").await
    }

    /// Lists currently 86'd ingredients for the tenant (#3695).
    ///
    /// Route: `GET /commerce/menus/ingredients/86`.
    pub async fn list_eighty_sixed_ingredients(&self) -> Result<Value> {
        self.http.get("/commerce/menus/ingredients/86").await
    }

    /// Fetches the 86 audit log.
    ///
    /// Route: `GET /commerce/menus/86/log`.
    pub async fn get_eighty_six_log(
        &self,
        entity_id: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(eid) = entity_id {
            query.push(("entity_id", eid.to_string()));
        }
        if let Some(lim) = limit {
            query.push(("limit", lim.to_string()));
        }
        if query.is_empty() {
            self.http.get("/commerce/menus/86/log").await
        } else {
            let refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            self.http
                .get_with_query("/commerce/menus/86/log", &refs)
                .await
        }
    }

    // -----------------------------------------------------------------------
    // Combos lifecycle — RC1 #3707
    //
    // Voice combo matcher (#3701) reads via `GET /commerce/combos` with
    // `active=true` + `location_id` at session bootstrap.
    // -----------------------------------------------------------------------

    /// Creates a combo deal.
    ///
    /// `body` should match the `CreateComboRequest` shape: `location_id`,
    /// `name`, `combo_price` (string for Decimal precision), `component_items`
    /// (array of `{menu_item_id, quantity}`), with optional `description`,
    /// `valid_from`, `valid_until`, `active`.
    ///
    /// Route: `POST /commerce/combos` — RC1 #3707.
    pub async fn create_combo(&self, body: &Value) -> Result<Value> {
        self.http.post("/commerce/combos", body).await
    }

    /// Lists combos with optional `location_id` and `active` filters.
    ///
    /// The voice combo matcher (#3701) calls with `location_id` + `active=true`
    /// at session bootstrap.
    ///
    /// Route: `GET /commerce/combos`.
    pub async fn list_combos(
        &self,
        location_id: Option<&str>,
        active: Option<bool>,
    ) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(loc) = location_id {
            query.push(("location_id", loc.to_string()));
        }
        if let Some(a) = active {
            query.push(("active", a.to_string()));
        }
        if query.is_empty() {
            self.http.get("/commerce/combos").await
        } else {
            let refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            self.http.get_with_query("/commerce/combos", &refs).await
        }
    }

    /// Updates a combo. `body` should contain only the fields to change.
    ///
    /// Route: `PATCH /commerce/combos/{id}`.
    pub async fn update_combo(&self, combo_id: &str, body: &Value) -> Result<Value> {
        self.http
            .patch(&format!("/commerce/combos/{}", combo_id), body)
            .await
    }

    /// Soft-deletes a combo (sets `active=false`).
    ///
    /// Route: `DELETE /commerce/combos/{id}`.
    pub async fn delete_combo(&self, combo_id: &str) -> Result<Value> {
        self.http
            .delete(&format!("/commerce/combos/{}", combo_id))
            .await
    }
}
