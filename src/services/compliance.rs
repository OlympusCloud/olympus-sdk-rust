//! ComplianceService — dram-shop compliance ledger (#3316).
//!
//! Wraps the platform dram-shop endpoints used cross-app by BarOS and
//! PizzaOS:
//!
//!   * `POST /platform/compliance/dram-shop-events`
//!   * `GET  /platform/compliance/dram-shop-events`
//!   * `GET  /platform/compliance/dram-shop-rules`
//!
//! Tenant-scoped append-only ledger of dram-shop liability events
//! (id-check passed/failed, service refused, over-serve warning, incident
//! filed) plus the currently-effective platform/app rule-set used by the
//! future BAC estimator and incident-packet exporter (#3310).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Single row from `platform_dram_shop_events` (#3316).
///
/// `event_type` is one of `id_check_passed`, `id_check_failed`,
/// `service_refused`, `over_serve_warning`, `incident_filed`.
/// `customer_ref` is a hashed/opaque key — never raw PII. Both
/// `bac_inputs` and `vertical_extensions` are app-defined JSON
/// payloads consumed by the BAC estimator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DramShopEvent {
    pub event_id: String,
    pub tenant_id: String,
    pub location_id: String,
    pub event_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub customer_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staff_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_bac: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bac_inputs: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_extensions: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub occurred_at: String,
    pub created_at: String,
}

/// Response envelope for `GET /platform/compliance/dram-shop-events`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DramShopEventList {
    #[serde(default)]
    pub events: Vec<DramShopEvent>,
    #[serde(default)]
    pub total_returned: u64,
}

/// Single row from `platform_dram_shop_rules` (#3316).
///
/// `rule_payload` is an arbitrary JSON object describing the rule (for
/// example, jurisdiction-specific BAC thresholds). `override_app_id`
/// is set when the rule is an app-specific override of a platform default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DramShopRule {
    pub tenant_id: String,
    pub rule_id: String,
    pub jurisdiction_code: String,
    pub rule_type: String,
    #[serde(default)]
    pub rule_payload: Option<Value>,
    pub effective_from: String,
    #[serde(default)]
    pub effective_until: Option<String>,
    #[serde(default)]
    pub override_app_id: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    pub created_at: String,
}

/// Parameters for [`ComplianceService::record_dram_shop_event`].
///
/// `location_id` and `event_type` are required; everything else is
/// optional. `event_type` MUST be one of the canonical values listed on
/// [`DramShopEvent`].
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecordDramShopEventParams {
    pub location_id: String,
    pub event_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub customer_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staff_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_bac: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bac_inputs: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_extensions: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// RFC3339 timestamp string (e.g. `2026-04-25T13:00:00Z`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurred_at: Option<String>,
}

/// Parameters for [`ComplianceService::list_dram_shop_events`].
///
/// `from` / `to` are RFC3339 timestamp strings. `limit` is clamped
/// server-side to `1..=500` (default 100).
#[derive(Debug, Clone, Default)]
pub struct ListDramShopEventsParams {
    pub location_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub event_type: Option<String>,
    pub limit: Option<u32>,
}

/// Parameters for [`ComplianceService::list_dram_shop_rules`].
///
/// When `app_id` is supplied, the response includes the app's overrides
/// PLUS the platform default rules. Without `app_id`, only platform
/// defaults are returned.
#[derive(Debug, Clone, Default)]
pub struct ListDramShopRulesParams {
    pub jurisdiction_code: Option<String>,
    pub app_id: Option<String>,
    pub rule_type: Option<String>,
}

/// Dram-shop compliance ledger surface (#3316).
pub struct ComplianceService {
    http: Arc<OlympusHttpClient>,
}

impl ComplianceService {
    /// Creates a new ComplianceService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Record a dram-shop compliance event (#3316).
    ///
    /// Returns the persisted event with server-set `event_id`,
    /// `tenant_id`, and `created_at`.
    pub async fn record_dram_shop_event(
        &self,
        params: RecordDramShopEventParams,
    ) -> Result<DramShopEvent> {
        let body = serde_json::to_value(&params)?;
        let resp = self
            .http
            .post("/platform/compliance/dram-shop-events", &body)
            .await?;
        Ok(serde_json::from_value(resp)?)
    }

    /// List dram-shop events for the current tenant with optional filters
    /// (#3316).
    pub async fn list_dram_shop_events(
        &self,
        params: ListDramShopEventsParams,
    ) -> Result<DramShopEventList> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(v) = &params.location_id {
            query.push(("location_id", v.clone()));
        }
        if let Some(v) = &params.from {
            query.push(("from", v.clone()));
        }
        if let Some(v) = &params.to {
            query.push(("to", v.clone()));
        }
        if let Some(v) = &params.event_type {
            query.push(("event_type", v.clone()));
        }
        if let Some(v) = params.limit {
            query.push(("limit", v.to_string()));
        }
        let path = "/platform/compliance/dram-shop-events";
        let resp = if query.is_empty() {
            self.http.get(path).await?
        } else {
            let q: Vec<(&str, &str)> =
                query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            self.http.get_with_query(path, &q).await?
        };
        Ok(serde_json::from_value(resp)?)
    }

    /// List currently-effective dram-shop rules (#3316).
    pub async fn list_dram_shop_rules(
        &self,
        params: ListDramShopRulesParams,
    ) -> Result<Vec<DramShopRule>> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(v) = &params.jurisdiction_code {
            query.push(("jurisdiction_code", v.clone()));
        }
        if let Some(v) = &params.app_id {
            query.push(("app_id", v.clone()));
        }
        if let Some(v) = &params.rule_type {
            query.push(("rule_type", v.clone()));
        }
        let path = "/platform/compliance/dram-shop-rules";
        let resp = if query.is_empty() {
            self.http.get(path).await?
        } else {
            let q: Vec<(&str, &str)> =
                query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            self.http.get_with_query(path, &q).await?
        };
        let rows = resp
            .get("rules")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(rows
            .into_iter()
            .filter_map(|row| serde_json::from_value(row).ok())
            .collect())
    }
}
