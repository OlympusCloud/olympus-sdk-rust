//! PayService — payment processor routing config (#3312).
//!
//! Today this surface is narrowly scoped to the per-location processor
//! routing CRUD landed via olympus-cloud-gcp PR #3528:
//!
//!   * `POST /platform/pay/routing`
//!   * `GET  /platform/pay/routing/{location_id}`
//!
//! Other payment surfaces (intents, refunds, balance, payouts, terminal)
//! continue to live on the Go gateway via the broader gateway proxy and
//! are not yet wrapped on this Rust SDK.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Per-location processor routing config (#3312).
///
/// `preferred_processor` and entries in `fallback_processors` MUST each
/// be one of: `olympus_pay`, `square`, `adyen`, `worldpay`. The fallback
/// chain cannot include the preferred processor.
///
/// `credentials_secret_ref` is a Secret Manager secret NAME (NOT the
/// credential itself) starting with `olympus-merchant-credentials-`
/// per the canonical secrets schema. Plaintext API keys are rejected
/// at the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    #[serde(default)]
    pub tenant_id: String,
    pub location_id: String,
    pub preferred_processor: String,
    #[serde(default)]
    pub fallback_processors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials_secret_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merchant_id: Option<String>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Parameters for [`PayService::configure_routing`].
///
/// `is_active` defaults to `true` to match the canonical wire shape on
/// the other 4 SDKs (Dart 0.8.3 / TS 0.5.2 / Python 0.5.2 / Go 0.5.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigureRoutingParams {
    pub location_id: String,
    pub preferred_processor: String,
    #[serde(default)]
    pub fallback_processors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials_secret_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merchant_id: Option<String>,
    pub is_active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Default for ConfigureRoutingParams {
    fn default() -> Self {
        Self {
            location_id: String::new(),
            preferred_processor: String::new(),
            fallback_processors: Vec::new(),
            credentials_secret_ref: None,
            merchant_id: None,
            is_active: true,
            notes: None,
        }
    }
}

/// Payment processor routing config service (#3312).
pub struct PayService {
    http: Arc<OlympusHttpClient>,
}

impl PayService {
    /// Creates a new PayService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Configure (upsert) per-location processor routing (#3312).
    pub async fn configure_routing(
        &self,
        params: ConfigureRoutingParams,
    ) -> Result<RoutingConfig> {
        let body = serde_json::to_value(&params)?;
        let resp = self.http.post("/platform/pay/routing", &body).await?;
        Ok(serde_json::from_value(resp)?)
    }

    /// Read the current routing config for a location (#3312).
    ///
    /// Returns 404 (`OlympusError::NotFound`) when no config exists for
    /// the location.
    pub async fn get_routing(&self, location_id: &str) -> Result<RoutingConfig> {
        let path = format!(
            "/platform/pay/routing/{}",
            urlencoding::encode(location_id)
        );
        let resp = self.http.get(&path).await?;
        Ok(serde_json::from_value(resp)?)
    }

    /// List all routing configs for the caller's tenant (#3312 pt2 → gcp PR #3537).
    ///
    /// All filters optional:
    /// - `is_active` — `Some(true)` returns only active configs, `Some(false)`
    ///   only inactive. `None` returns both.
    /// - `processor` — restrict to one of `olympus_pay`, `square`, `adyen`,
    ///   `worldpay`. The server rejects anything else with HTTP 400.
    /// - `limit` — page size 1..=200 (default 100). Pagination by `location_id`
    ///   lands later if any tenant exceeds 200 active locations.
    ///
    /// `RoutingConfigList::total_returned` reflects the count of configs the
    /// server actually returned; compare against the requested `limit` to
    /// detect a capped response.
    pub async fn list_routing(
        &self,
        params: ListRoutingParams,
    ) -> Result<RoutingConfigList> {
        // Build the query slice with owned strings so we can return a
        // lifetime-clean &[(&str, &str)] to get_with_query. Using
        // String::as_str() keeps the allocation explicit and predictable.
        let limit_str: String;
        let mut q: Vec<(&str, &str)> = Vec::new();
        if let Some(active) = params.is_active {
            q.push(("is_active", if active { "true" } else { "false" }));
        }
        if let Some(processor) = params.processor.as_deref() {
            q.push(("processor", processor));
        }
        if let Some(limit) = params.limit {
            limit_str = limit.to_string();
            q.push(("limit", limit_str.as_str()));
        }
        let resp = self
            .http
            .get_with_query("/platform/pay/routing", &q)
            .await?;
        Ok(serde_json::from_value(resp)?)
    }
}

/// Filters for [`PayService::list_routing`].
///
/// `is_active = Some(false)` lets callers explicitly query inactive configs;
/// `None` returns both active + inactive (matches the server default).
#[derive(Debug, Clone, Default)]
pub struct ListRoutingParams {
    pub is_active: Option<bool>,
    pub processor: Option<String>,
    pub limit: Option<u32>,
}

/// Result of [`PayService::list_routing`] (#3312 pt2 → gcp PR #3537).
///
/// `total_returned` is the count of configs the server actually returned;
/// compare against the requested `limit` to detect a capped response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfigList {
    #[serde(default)]
    pub configs: Vec<RoutingConfig>,
    #[serde(default)]
    pub total_returned: u32,
}
